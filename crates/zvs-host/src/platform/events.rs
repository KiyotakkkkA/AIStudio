use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use serde_json::json;
use zvs_contract::{
    AppError, AppErrorCode, EventSeq, HostEvent, HostEventDraft, RunOutcomeDto, StreamId,
};

use super::clock::{SharedClock, SystemClock};
use super::ids::{IdGenerator, system_ids};
use super::logger::{LogLevel, SharedLogger, silent_logger};

pub trait EventSink: Send + Sync {
    fn deliver(&self, event: &HostEvent) -> Result<(), String>;
}

pub trait EventRecorder: Send + Sync {
    fn append(&self, event: &HostEvent);
    fn close(&self);
}

pub type RecordHook = Box<dyn Fn(&HostEvent) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EventBusError {
    #[error("The event bus is disposed")]
    Disposed,
    #[error("The stream is already open")]
    AlreadyOpen,
}

impl From<EventBusError> for AppError {
    fn from(error: EventBusError) -> Self {
        Self::new(AppErrorCode::Unknown, error.to_string())
    }
}

#[derive(Default)]
pub struct StreamOptions {
    pub id: Option<StreamId>,
    pub next_seq: Option<EventSeq>,
    pub record: Option<RecordHook>,
}

pub struct EventBusOptions {
    pub logger: SharedLogger,
    pub clock: SharedClock,
    pub ids: IdGenerator,
    pub recorder: Option<Box<dyn EventRecorder>>,
}

impl Default for EventBusOptions {
    fn default() -> Self {
        Self {
            logger: silent_logger(),
            clock: Arc::new(SystemClock),
            ids: system_ids(),
            recorder: None,
        }
    }
}

struct BusInner {
    logger: SharedLogger,
    clock: SharedClock,
    ids: IdGenerator,
    recorder: Option<Box<dyn EventRecorder>>,
    sink: RwLock<Option<Arc<dyn EventSink>>>,
    live: Mutex<HashSet<StreamId>>,
    disposed: AtomicBool,
}

impl BusInner {
    fn deliver(&self, event: &HostEvent) {
        if let Some(recorder) = &self.recorder {
            recorder.append(event);
        }
        let sink = self.sink.read().ok().and_then(|slot| slot.clone());
        let Some(sink) = sink else {
            self.logger.log(
                LogLevel::Debug,
                "events",
                "Dropped an event, no window is open",
                &[
                    ("streamId", json!(event.stream_id)),
                    ("type", json!(type_of(&event.body))),
                    ("seq", json!(event.seq)),
                ],
            );
            return;
        };
        if let Err(error) = sink.deliver(event) {
            self.logger.log(
                LogLevel::Error,
                "events",
                "Could not deliver event",
                &[
                    ("streamId", json!(event.stream_id)),
                    ("error", json!(error)),
                ],
            );
        }
    }

    fn release(&self, id: StreamId) {
        if let Ok(mut live) = self.live.lock() {
            live.remove(&id);
        }
    }
}

#[derive(Clone)]
pub struct EventBus {
    inner: Arc<BusInner>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(EventBusOptions::default())
    }
}

impl EventBus {
    pub fn new(options: EventBusOptions) -> Self {
        Self {
            inner: Arc::new(BusInner {
                logger: options.logger,
                clock: options.clock,
                ids: options.ids,
                recorder: options.recorder,
                sink: RwLock::new(None),
                live: Mutex::new(HashSet::new()),
                disposed: AtomicBool::new(false),
            }),
        }
    }

    pub fn attach(&self, sink: Arc<dyn EventSink>) {
        if let Ok(mut slot) = self.inner.sink.write() {
            *slot = Some(sink);
        }
    }

    pub fn detach(&self) {
        if let Ok(mut slot) = self.inner.sink.write() {
            *slot = None;
        }
    }

    pub fn open(&self) -> usize {
        self.inner.live.lock().map_or(0, |live| live.len())
    }

    pub fn open_stream(&self) -> Result<StreamHandle, EventBusError> {
        self.open_stream_with(StreamOptions::default())
    }

    pub fn open_stream_with(&self, options: StreamOptions) -> Result<StreamHandle, EventBusError> {
        if self.inner.disposed.load(Ordering::SeqCst) {
            return Err(EventBusError::Disposed);
        }
        let id = options
            .id
            .unwrap_or_else(|| StreamId::new((self.inner.ids)()));
        {
            let mut live = self
                .inner
                .live
                .lock()
                .map_err(|_| EventBusError::Disposed)?;
            if !live.insert(id) {
                return Err(EventBusError::AlreadyOpen);
            }
        }
        Ok(StreamHandle {
            id,
            bus: Arc::clone(&self.inner),
            record: options.record,
            state: Mutex::new(StreamState {
                seq: options.next_seq.unwrap_or(0),
                closed: false,
            }),
        })
    }

    pub fn dispose(&self) {
        self.inner.disposed.store(true, Ordering::SeqCst);
        if let Ok(mut live) = self.inner.live.lock() {
            live.clear();
        }
        if let Some(recorder) = &self.inner.recorder {
            recorder.close();
        }
    }
}

struct StreamState {
    seq: EventSeq,
    closed: bool,
}

pub struct StreamHandle {
    id: StreamId,
    bus: Arc<BusInner>,
    record: Option<RecordHook>,
    state: Mutex<StreamState>,
}

impl StreamHandle {
    pub const fn id(&self) -> StreamId {
        self.id
    }

    pub fn closed(&self) -> bool {
        self.state.lock().map_or(true, |state| state.closed)
    }

    pub fn emit(&self, draft: HostEventDraft) {
        self.publish(draft, false);
    }

    pub fn end(&self, outcome: RunOutcomeDto) {
        self.publish(HostEventDraft::End { outcome }, true);
    }

    pub fn publish_with<E>(
        &self,
        drafts: Vec<HostEventDraft>,
        persist: impl FnOnce(&[HostEvent]) -> Result<(), E>,
    ) -> Result<Vec<HostEvent>, E> {
        let Ok(mut state) = self.state.lock() else {
            return persist(&[]).map(|()| Vec::new());
        };
        let mut seq = state.seq;
        let mut closed = state.closed;
        let mut events = Vec::with_capacity(drafts.len());
        for draft in drafts {
            if closed {
                self.bus.logger.log(
                    LogLevel::Error,
                    "events",
                    "Emitted on a stream that already ended",
                    &[
                        ("streamId", json!(self.id)),
                        ("type", json!(type_of(&draft))),
                    ],
                );
                continue;
            }
            closed = matches!(draft, HostEventDraft::End { .. });
            events.push(HostEvent::new(self.id, seq, self.bus.clock.now(), draft));
            seq += 1;
        }
        persist(&events)?;
        state.seq = seq;
        state.closed = closed;
        for event in &events {
            if let Some(record) = &self.record {
                record(event);
            }
            if matches!(event.body, HostEventDraft::End { .. }) {
                self.bus.release(self.id);
            }
            self.bus.deliver(event);
        }
        Ok(events)
    }

    fn publish(&self, draft: HostEventDraft, ending: bool) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.closed {
            let kind = if ending { "end" } else { type_of(&draft) };
            self.bus.logger.log(
                LogLevel::Error,
                "events",
                "Emitted on a stream that already ended",
                &[("streamId", json!(self.id)), ("type", json!(kind))],
            );
            return;
        }
        let event = HostEvent::new(self.id, state.seq, self.bus.clock.now(), draft);
        state.seq += 1;
        if let Some(record) = &self.record {
            record(&event);
        }
        if ending {
            state.closed = true;
            self.bus.release(self.id);
        }
        self.bus.deliver(&event);
    }
}

pub const fn type_of(draft: &HostEventDraft) -> &'static str {
    match draft {
        HostEventDraft::Token { .. } => "token",
        HostEventDraft::Progress { .. } => "progress",
        HostEventDraft::Step { .. } => "step",
        HostEventDraft::Log { .. } => "log",
        HostEventDraft::Approval { .. } => "approval",
        HostEventDraft::Question { .. } => "question",
        HostEventDraft::End { .. } => "end",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use uuid::Uuid;
    use zvs_contract::{RunOutcomeStatus, Validate};

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::testing::{MemoryLogger, MemorySink};

    struct FailingSink;

    impl EventSink for FailingSink {
        fn deliver(&self, _event: &HostEvent) -> Result<(), String> {
            Err("webview gone".to_owned())
        }
    }

    #[derive(Default)]
    struct MemoryRecorder {
        events: Mutex<Vec<HostEvent>>,
        closed: AtomicBool,
    }

    impl EventRecorder for Arc<MemoryRecorder> {
        fn append(&self, event: &HostEvent) {
            self.events.lock().expect("events").push(event.clone());
        }

        fn close(&self) {
            self.closed.store(true, Ordering::SeqCst);
        }
    }

    fn uuids() -> IdGenerator {
        let next = AtomicUsize::new(1);
        Arc::new(move || {
            let index = next.fetch_add(1, Ordering::SeqCst);
            Uuid::parse_str(&format!("0199aa11-1111-7111-8111-{index:012}")).expect("uuid")
        })
    }

    fn outcome(status: RunOutcomeStatus) -> RunOutcomeDto {
        RunOutcomeDto {
            status,
            code: None,
            message: None,
        }
    }

    fn token(delta: &str) -> HostEventDraft {
        HostEventDraft::Token {
            delta: delta.to_owned(),
            kind: None,
        }
    }

    fn bus_with(logger: SharedLogger, recorder: Option<Box<dyn EventRecorder>>) -> EventBus {
        EventBus::new(EventBusOptions {
            logger,
            clock: Arc::new(FakeClock::default()),
            ids: uuids(),
            recorder,
        })
    }

    #[test]
    fn seq_is_monotonic_per_stream_and_never_shared_between_streams() {
        let sink = Arc::new(MemorySink::default());
        let bus = bus_with(silent_logger(), None);
        bus.attach(sink.clone());
        let first = bus.open_stream().expect("opens");
        let second = bus.open_stream().expect("opens");
        first.emit(token("a"));
        second.emit(token("b"));
        first.emit(token("c"));
        second.end(outcome(RunOutcomeStatus::Ok));
        first.end(outcome(RunOutcomeStatus::Cancelled));

        let sent = sink.events();
        let seq_of = |id: StreamId| -> Vec<EventSeq> {
            sent.iter()
                .filter(|event| event.stream_id == id)
                .map(|event| event.seq)
                .collect()
        };
        assert_eq!(seq_of(first.id()), [0, 1, 2]);
        assert_eq!(seq_of(second.id()), [0, 1]);
        assert_eq!(bus.open(), 0);
        for event in &sent {
            let wire = serde_json::to_value(event).expect("serialises");
            let parsed: HostEvent = serde_json::from_value(wire).expect("parses");
            assert!(parsed.validate().is_ok());
            assert_eq!(&parsed, event);
            assert_eq!(event.ts, crate::platform::clock::CLOCK_ORIGIN);
        }
    }

    #[test]
    fn emitting_on_an_ended_stream_is_logged_not_thrown() {
        let sink = Arc::new(MemorySink::default());
        let logger = MemoryLogger::shared();
        let bus = bus_with(logger.clone(), None);
        bus.attach(sink.clone());
        let stream = bus.open_stream().expect("opens");
        stream.end(outcome(RunOutcomeStatus::Ok));
        stream.emit(token("late"));
        stream.end(outcome(RunOutcomeStatus::Ok));

        assert_eq!(sink.events().len(), 1);
        assert!(stream.closed());
        let errors: Vec<_> = logger
            .entries()
            .into_iter()
            .filter(|entry| entry.level == LogLevel::Error)
            .collect();
        assert_eq!(errors.len(), 2);
        assert!(errors[0].message.contains("already ended"));
        assert_eq!(errors[0].fields["type"], json!("token"));
        assert_eq!(errors[1].fields["type"], json!("end"));
    }

    #[test]
    fn with_no_sink_attached_events_are_dropped_at_debug_level_but_still_recorded() {
        let logger = MemoryLogger::shared();
        let recorder = Arc::new(MemoryRecorder::default());
        let bus = bus_with(logger.clone(), Some(Box::new(recorder.clone())));
        let stream = bus.open_stream().expect("opens");
        stream.emit(HostEventDraft::Progress { done: 1, total: 2 });
        stream.end(outcome(RunOutcomeStatus::Ok));

        assert_eq!(recorder.events.lock().expect("events").len(), 2);
        let dropped = logger.matching("no window");
        assert_eq!(dropped.len(), 2);
        assert!(dropped.iter().all(|entry| entry.level == LogLevel::Debug));
        bus.dispose();
        assert!(recorder.closed.load(Ordering::SeqCst));
    }

    #[test]
    fn a_detached_sink_stops_delivery() {
        let sink = Arc::new(MemorySink::default());
        let bus = bus_with(silent_logger(), None);
        bus.attach(sink.clone());
        let stream = bus.open_stream().expect("opens");
        stream.emit(token("a"));
        bus.detach();
        stream.emit(token("b"));
        assert_eq!(sink.events().len(), 1);
    }

    #[test]
    fn a_failing_sink_is_logged_and_the_stream_goes_on() {
        let logger = MemoryLogger::shared();
        let bus = bus_with(logger.clone(), None);
        bus.attach(Arc::new(FailingSink));
        let stream = bus.open_stream().expect("opens");
        stream.emit(token("a"));
        stream.end(outcome(RunOutcomeStatus::Ok));
        let failures = logger.matching("Could not deliver event");
        assert_eq!(failures.len(), 2);
        assert_eq!(failures[0].fields["error"], json!("webview gone"));
        assert!(stream.closed());
    }

    #[test]
    fn an_explicit_id_and_starting_seq_are_honoured_and_hooks_see_every_event() {
        let sink = Arc::new(MemorySink::default());
        let bus = bus_with(silent_logger(), None);
        bus.attach(sink.clone());
        let id = StreamId::parse("0199aa11-1111-7111-8111-000000000099").expect("uuid");
        let hooked = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&hooked);
        let stream = bus
            .open_stream_with(StreamOptions {
                id: Some(id),
                next_seq: Some(5),
                record: Some(Box::new(move |event: &HostEvent| {
                    seen.lock().expect("seen").push(event.seq);
                })),
            })
            .expect("opens");
        assert_eq!(stream.id(), id);
        assert_eq!(
            bus.open_stream_with(StreamOptions {
                id: Some(id),
                ..StreamOptions::default()
            })
            .err(),
            Some(EventBusError::AlreadyOpen)
        );
        stream.emit(token("a"));
        stream.end(outcome(RunOutcomeStatus::Ok));
        assert_eq!(*hooked.lock().expect("seen"), [5, 6]);
        assert_eq!(
            sink.events()
                .iter()
                .map(|event| event.seq)
                .collect::<Vec<_>>(),
            [5, 6]
        );
        assert!(
            bus.open_stream_with(StreamOptions {
                id: Some(id),
                ..StreamOptions::default()
            })
            .is_ok()
        );
    }

    #[test]
    fn a_batch_is_persisted_before_delivery_and_a_failed_persist_publishes_nothing() {
        let sink = Arc::new(MemorySink::default());
        let bus = bus_with(silent_logger(), None);
        bus.attach(sink.clone());
        let stream = bus.open_stream().expect("opens");
        let refused = stream.publish_with(vec![token("lost")], |events| {
            assert_eq!(events[0].seq, 0);
            Err("disk full")
        });
        assert_eq!(refused.err(), Some("disk full"));
        assert!(sink.events().is_empty());

        let mut persisted = Vec::new();
        let published = stream
            .publish_with(
                vec![
                    token("a"),
                    HostEventDraft::End {
                        outcome: outcome(RunOutcomeStatus::Ok),
                    },
                    token("late"),
                ],
                |events| {
                    persisted.extend(events.iter().map(|event| event.seq));
                    Ok::<(), ()>(())
                },
            )
            .expect("publishes");
        assert_eq!(persisted, [0, 1]);
        assert_eq!(published.len(), 2);
        assert_eq!(sink.events(), published);
        assert!(stream.closed());
        assert_eq!(bus.open(), 0);
        let after = stream
            .publish_with(vec![token("b")], |events| {
                assert!(events.is_empty());
                Ok::<(), ()>(())
            })
            .expect("runs the persist step");
        assert!(after.is_empty());
    }

    #[test]
    fn a_disposed_bus_refuses_new_streams() {
        let bus = bus_with(silent_logger(), None);
        let _stream = bus.open_stream().expect("opens");
        assert_eq!(bus.open(), 1);
        bus.dispose();
        assert_eq!(bus.open(), 0);
        assert_eq!(bus.open_stream().err(), Some(EventBusError::Disposed));
        let error: AppError = EventBusError::Disposed.into();
        assert_eq!(error.code, AppErrorCode::Unknown);
    }
}
