use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use zvs_contract::{AppError, AppErrorCode, HostEvent, HostEventDraft, StreamId, Timestamp};

use crate::data::{DataError, Database, Repositories};
use crate::platform::events::StreamHandle;

pub const TOKEN_FLUSH_MS: Timestamp = 250;

pub type RunObserver = Arc<dyn Fn(&HostEvent) + Send + Sync>;
pub type RunWatcher = Arc<dyn Fn(&str, &HostEvent) + Send + Sync>;

#[derive(Default)]
pub struct Observers {
    observers: Mutex<HashMap<String, Vec<(u64, RunObserver)>>>,
    watchers: Mutex<Vec<RunWatcher>>,
    next: AtomicU64,
}

impl Observers {
    pub fn add(&self, run_id: &str, observer: RunObserver) -> u64 {
        let token = self.next.fetch_add(1, Ordering::SeqCst);
        self.observers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(run_id.to_owned())
            .or_default()
            .push((token, observer));
        token
    }

    pub fn watch(&self, watcher: RunWatcher) {
        self.watchers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(watcher);
    }

    pub fn remove(&self, run_id: &str, token: u64) {
        let mut observers = self
            .observers
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(list) = observers.get_mut(run_id) {
            list.retain(|(candidate, _)| *candidate != token);
            if list.is_empty() {
                observers.remove(run_id);
            }
        }
    }

    pub fn clear(&self) {
        self.observers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.watchers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    pub fn count(&self, run_id: &str) -> usize {
        self.observers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(run_id)
            .map_or(0, Vec::len)
    }

    fn notify(&self, run_id: &str, event: &HostEvent) {
        let ending = matches!(event.body, HostEventDraft::End { .. });
        let current: Vec<RunObserver> = {
            let mut observers = self
                .observers
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let current = observers
                .get(run_id)
                .map(|list| {
                    list.iter()
                        .map(|(_, observer)| Arc::clone(observer))
                        .collect()
                })
                .unwrap_or_default();
            if ending {
                observers.remove(run_id);
            }
            current
        };
        for observer in current {
            observer(event);
        }
        let watchers = self
            .watchers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        for watcher in watchers {
            watcher(run_id, event);
        }
    }
}

pub struct Subscription {
    observers: Arc<Observers>,
    run_id: String,
    token: u64,
}

impl Subscription {
    pub(crate) const fn new(observers: Arc<Observers>, run_id: String, token: u64) -> Self {
        Self {
            observers,
            run_id,
            token,
        }
    }

    pub fn unsubscribe(self) {
        self.observers.remove(&self.run_id, self.token);
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TokenBuffer {
    merged: Option<HostEvent>,
    since: Timestamp,
}

impl TokenBuffer {
    pub fn push(&mut self, event: &HostEvent) -> Vec<HostEvent> {
        let HostEventDraft::Token { delta, kind } = &event.body else {
            return self.take().into_iter().chain([event.clone()]).collect();
        };
        let mut flushed = Vec::new();
        match &mut self.merged {
            Some(merged) => match &mut merged.body {
                HostEventDraft::Token {
                    delta: buffered,
                    kind: buffered_kind,
                } if buffered_kind == kind => {
                    buffered.push_str(delta);
                    merged.seq = event.seq;
                    merged.ts = event.ts;
                }
                _ => {
                    flushed.extend(self.take());
                    self.start(event);
                }
            },
            None => self.start(event),
        }
        if event.ts - self.since >= TOKEN_FLUSH_MS {
            flushed.extend(self.take());
        }
        flushed
    }

    pub fn take(&mut self) -> Option<HostEvent> {
        self.merged.take()
    }

    pub const fn pending(&self) -> Option<&HostEvent> {
        self.merged.as_ref()
    }

    fn start(&mut self, event: &HostEvent) {
        self.merged = Some(event.clone());
        self.since = event.ts;
    }
}

pub struct RunStream {
    run_id: String,
    handle: StreamHandle,
    database: Arc<Database>,
    observers: Arc<Observers>,
    gate: Mutex<()>,
    tokens: Mutex<TokenBuffer>,
}

impl RunStream {
    pub fn new(
        run_id: String,
        handle: StreamHandle,
        database: Arc<Database>,
        observers: Arc<Observers>,
    ) -> Self {
        Self {
            run_id,
            handle,
            database,
            observers,
            gate: Mutex::new(()),
            tokens: Mutex::new(TokenBuffer::default()),
        }
    }

    pub const fn stream_id(&self) -> StreamId {
        self.handle.id()
    }

    pub fn closed(&self) -> bool {
        self.handle.closed()
    }

    pub fn emit(&self, draft: HostEventDraft) -> Result<(), AppError> {
        if matches!(draft, HostEventDraft::Token { .. }) {
            return self.token(draft);
        }
        self.commit(vec![draft], |_| Ok(()))
    }

    fn buffer(&self) -> std::sync::MutexGuard<'_, TokenBuffer> {
        self.tokens.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn persist(
        &self,
        repositories: &Repositories<'_>,
        events: &[HostEvent],
    ) -> Result<(), DataError> {
        for event in events {
            repositories.runs.append_event(&self.run_id, event)?;
        }
        Ok(())
    }

    fn token(&self, draft: HostEventDraft) -> Result<(), AppError> {
        let _gate = self.gate.lock().unwrap_or_else(PoisonError::into_inner);
        let events = self.handle.publish_with(vec![draft], |events| {
            let mut buffer = self.buffer().clone();
            let flushed: Vec<HostEvent> =
                events.iter().flat_map(|event| buffer.push(event)).collect();
            if !flushed.is_empty() {
                self.database.transaction(|transaction| {
                    self.persist(&Repositories::new(transaction), &flushed)
                })?;
            }
            *self.buffer() = buffer;
            Ok::<(), AppError>(())
        })?;
        for event in &events {
            self.observers.notify(&self.run_id, event);
        }
        Ok(())
    }

    pub fn commit<T>(
        &self,
        drafts: Vec<HostEventDraft>,
        work: impl FnOnce(&Repositories<'_>) -> Result<T, DataError>,
    ) -> Result<T, AppError> {
        let _gate = self.gate.lock().unwrap_or_else(PoisonError::into_inner);
        let mut value = None;
        let events = self.handle.publish_with(drafts, |events| {
            let mut buffer = self.buffer().clone();
            let mut stored: Vec<HostEvent> = buffer.take().into_iter().collect();
            stored.extend(events.iter().cloned());
            self.database.transaction(|transaction| {
                let repositories = Repositories::new(transaction);
                value = Some(work(&repositories)?);
                self.persist(&repositories, &stored)
            })?;
            *self.buffer() = buffer;
            Ok::<(), AppError>(())
        })?;
        for event in &events {
            self.observers.notify(&self.run_id, event);
        }
        value.ok_or_else(|| AppError::new(AppErrorCode::Unknown, "The unit of work did not run"))
    }

    pub fn flush(&self) -> Result<(), AppError> {
        self.commit(Vec::new(), |_| Ok(()))
    }

    pub fn record(&self) -> Result<Vec<HostEvent>, AppError> {
        let _gate = self.gate.lock().unwrap_or_else(PoisonError::into_inner);
        let mut events = self.database.with_connection(|connection| {
            Ok(Repositories::new(connection).runs.events(&self.run_id)?)
        })?;
        events.extend(self.buffer().pending().cloned());
        Ok(events)
    }
}

impl Drop for RunStream {
    fn drop(&mut self) {
        if self.buffer().pending().is_some() {
            let _ = self.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use zvs_contract::{DeltaKind, GraphNode, RunGraph, RunKind, RunStatus};

    use super::*;
    use crate::data::repositories::RunDraft;
    use crate::platform::events::EventBus;
    use crate::testing::temporary_database;

    fn token(delta: &str, kind: Option<DeltaKind>) -> HostEventDraft {
        HostEventDraft::Token {
            delta: delta.to_owned(),
            kind,
        }
    }

    fn stored(database: &Database) -> Vec<HostEvent> {
        database
            .with_connection(|connection| Ok(Repositories::new(connection).runs.events("r1")?))
            .expect("reads")
    }

    #[test]
    fn tokens_stream_one_by_one_but_are_stored_merged_and_a_live_record_sees_the_tail() {
        let temporary = temporary_database();
        let database = temporary.shared();
        database
            .with_connection(|connection| {
                Repositories::new(connection).runs.create(&RunDraft {
                    id: "r1".to_owned(),
                    kind: RunKind::Job,
                    subject_id: None,
                    title: None,
                    retry_of_id: None,
                    status: RunStatus::Running,
                    graph: RunGraph::new(vec![GraphNode::new("a", "work").normalized()]),
                    input: Value::Null,
                    stream_id: "s".to_owned(),
                    concurrency: 1,
                    created_at: 1,
                    started_at: None,
                })?;
                Ok(())
            })
            .expect("creates a run");
        let bus = EventBus::default();
        let observers = Arc::new(Observers::default());
        let seen = Arc::new(Mutex::new(0));
        let counter = Arc::clone(&seen);
        observers.add(
            "r1",
            Arc::new(move |_| *counter.lock().unwrap_or_else(PoisonError::into_inner) += 1),
        );
        let stream = RunStream::new(
            "r1".to_owned(),
            bus.open_stream().expect("a stream"),
            Arc::clone(&database),
            observers,
        );

        for delta in ["a", "b", "c"] {
            stream.emit(token(delta, None)).expect("emits");
        }
        stream
            .emit(token("r", Some(DeltaKind::Reasoning)))
            .expect("emits");

        assert_eq!(*seen.lock().unwrap_or_else(PoisonError::into_inner), 4);
        let merged = stored(&database);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].seq, 2);
        assert_eq!(merged[0].body, token("abc", None));
        let live = stream.record().expect("records");
        assert_eq!(live.len(), 2);
        assert_eq!(live[1].seq, 3);
        assert_eq!(live[1].body, token("r", Some(DeltaKind::Reasoning)));

        stream
            .emit(HostEventDraft::Progress { done: 1, total: 1 })
            .expect("emits");
        let flushed = stored(&database);
        assert_eq!(
            flushed.iter().map(|event| event.seq).collect::<Vec<_>>(),
            [2, 3, 4]
        );
        assert_eq!(stream.record().expect("records"), flushed);
    }

    #[test]
    fn a_buffer_older_than_the_window_is_flushed_with_the_token_that_crossed_it() {
        let event = |seq: u64, ts: Timestamp| {
            HostEvent::new(
                StreamId::new(crate::platform::ids::new_id()),
                seq,
                ts,
                token("x", None),
            )
        };
        let mut buffer = TokenBuffer::default();
        assert!(buffer.push(&event(0, 0)).is_empty());
        assert!(buffer.push(&event(1, TOKEN_FLUSH_MS - 1)).is_empty());
        let flushed = buffer.push(&event(2, TOKEN_FLUSH_MS));
        assert_eq!(flushed.len(), 1);
        assert_eq!(flushed[0].seq, 2);
        assert_eq!(flushed[0].body, token("xxx", None));
        assert_eq!(buffer.pending(), None);
    }
}
