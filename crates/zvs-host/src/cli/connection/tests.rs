use std::sync::Mutex;

use tokio::io::{
    AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf,
};
use zvs_contract::{HostEventDraft, RunOutcomeDto, RunOutcomeStatus};

use super::*;
use crate::platform::logger::silent_logger;

struct Recorded {
    calls: Mutex<Vec<(Caller, String)>>,
    hidden: Mutex<Vec<zvs_contract::StreamId>>,
    closed: Mutex<Vec<u64>>,
    dropped: CancellationToken,
    notify: Mutex<Option<NoticeSender>>,
}

impl Default for Recorded {
    fn default() -> Self {
        Self {
            calls: Mutex::default(),
            hidden: Mutex::default(),
            closed: Mutex::default(),
            dropped: CancellationToken::new(),
            notify: Mutex::default(),
        }
    }
}

impl Recorded {
    fn channels(&self) -> Vec<String> {
        self.calls
            .lock()
            .expect("calls")
            .iter()
            .map(|(_, channel)| channel.clone())
            .collect()
    }
}

#[async_trait]
impl CliDispatch for Recorded {
    async fn dispatch(&self, caller: Caller, channel: &str, payload: Value) -> Value {
        self.calls
            .lock()
            .expect("calls")
            .push((caller, channel.to_owned()));
        json!({ "ok": true, "data": payload })
    }

    fn connect(&self, _client: CliClient, notify: NoticeSender) -> (u64, CancellationToken) {
        *self.notify.lock().expect("notify") = Some(notify);
        (7, self.dropped.clone())
    }

    fn disconnect(&self, connection: u64) {
        self.closed.lock().expect("closed").push(connection);
    }

    fn sees(&self, _connection: u64, event: &HostEvent) -> bool {
        !self
            .hidden
            .lock()
            .expect("hidden")
            .contains(&event.stream_id)
    }
}

struct Client {
    lines: Lines<BufReader<ReadHalf<DuplexStream>>>,
    writer: WriteHalf<DuplexStream>,
}

impl Client {
    async fn send(&mut self, message: Value) {
        let mut line = serde_json::to_vec(&message).expect("serialises");
        line.push(b'\n');
        self.writer.write_all(&line).await.expect("writes");
    }

    async fn next(&mut self) -> Option<CliServerMessage> {
        self.lines
            .next_line()
            .await
            .expect("reads")
            .map(|line| serde_json::from_str(&line).expect("parses"))
    }
}

fn connect(dispatch: Arc<Recorded>, events: EventBus) -> (Client, tokio::task::JoinHandle<()>) {
    let (client, server) = tokio::io::duplex(64 * 1024);
    let serving = tokio::spawn(serve(
        server,
        CliClient::default(),
        dispatch,
        events,
        CliNotices::default(),
        silent_logger(),
    ));
    let (reader, writer) = tokio::io::split(client);
    (
        Client {
            lines: BufReader::new(reader).lines(),
            writer,
        },
        serving,
    )
}

fn hello() -> Value {
    json!({ "type": "hello", "version": CLI_PROTOCOL_VERSION })
}

#[tokio::test]
async fn a_client_that_does_not_greet_is_answered_and_closed() {
    let dispatch = Arc::new(Recorded::default());
    let (mut client, serving) = connect(Arc::clone(&dispatch), EventBus::default());
    client
        .send(json!({ "type": "call", "id": 1, "channel": "system.ping" }))
        .await;

    assert_eq!(
        client.next().await,
        Some(CliServerMessage::Welcome {
            version: CLI_PROTOCOL_VERSION
        })
    );
    assert_eq!(client.next().await, None);
    serving.await.expect("serves");
    assert!(dispatch.channels().is_empty());
}

#[tokio::test]
async fn a_newer_protocol_is_refused_without_events() {
    let events = EventBus::default();
    let (mut client, serving) = connect(Arc::new(Recorded::default()), events.clone());
    client.send(json!({ "type": "hello", "version": 99 })).await;
    assert!(matches!(
        client.next().await,
        Some(CliServerMessage::Welcome { .. })
    ));
    assert_eq!(client.next().await, None);
    serving.await.expect("serves");
    assert_eq!(events.subscribers(), 0);
}

#[tokio::test]
async fn allowed_channels_are_dispatched_and_others_refused() {
    let dispatch = Arc::new(Recorded::default());
    let (mut client, _serving) = connect(Arc::clone(&dispatch), EventBus::default());
    client.send(hello()).await;
    assert!(matches!(
        client.next().await,
        Some(CliServerMessage::Welcome { .. })
    ));

    client
        .send(json!({ "type": "call", "id": 1, "channel": "system.ping", "payload": 5 }))
        .await;
    assert_eq!(
        client.next().await,
        Some(CliServerMessage::Result {
            id: 1,
            envelope: json!({ "ok": true, "data": 5 }),
        })
    );

    client
        .send(json!({ "type": "call", "id": 2, "channel": "secrets.get", "payload": {} }))
        .await;
    let Some(CliServerMessage::Result { id, envelope }) = client.next().await else {
        panic!("a result");
    };
    assert_eq!(id, 2);
    assert_eq!(envelope["ok"], json!(false));
    assert_eq!(envelope["error"]["code"], json!("PERMISSION_DENIED"));
    assert_eq!(envelope["error"]["message"], json!(CLI_CHANNEL_REFUSED));
    assert_eq!(
        *dispatch.calls.lock().expect("calls"),
        [(Caller::Cli { connection: 7 }, "system.ping".to_owned())]
    );
}

#[tokio::test]
async fn events_flow_after_the_greeting_and_stop_on_disconnect() {
    let events = EventBus::default();
    let (mut client, serving) = connect(Arc::new(Recorded::default()), events.clone());
    client.send(hello()).await;
    assert!(matches!(
        client.next().await,
        Some(CliServerMessage::Welcome { .. })
    ));
    assert_eq!(events.subscribers(), 1);

    let stream = events.open_stream().expect("opens");
    stream.emit(HostEventDraft::Token {
        delta: "привет".to_owned(),
        kind: None,
    });
    let Some(CliServerMessage::Event { event }) = client.next().await else {
        panic!("an event");
    };
    assert_eq!(event.stream_id, stream.id());

    drop(client);
    serving.await.expect("serves");
    assert_eq!(events.subscribers(), 0);
    stream.end(RunOutcomeDto {
        status: RunOutcomeStatus::Ok,
        code: None,
        message: None,
    });
}

#[tokio::test]
async fn a_malformed_line_drops_the_client() {
    let (mut client, serving) = connect(Arc::new(Recorded::default()), EventBus::default());
    client.send(hello()).await;
    assert!(matches!(
        client.next().await,
        Some(CliServerMessage::Welcome { .. })
    ));
    client.writer.write_all(b"{ nope\n").await.expect("writes");
    assert_eq!(client.next().await, None);
    serving.await.expect("serves");
}

#[tokio::test]
async fn only_events_of_the_connections_dialogs_are_written() {
    let events = EventBus::default();
    let dispatch = Arc::new(Recorded::default());
    let (mut client, _serving) = connect(Arc::clone(&dispatch), events.clone());
    client.send(hello()).await;
    assert!(matches!(
        client.next().await,
        Some(CliServerMessage::Welcome { .. })
    ));
    let foreign = events.open_stream().expect("opens");
    let own = events.open_stream().expect("opens");
    dispatch.hidden.lock().expect("hidden").push(foreign.id());
    foreign.emit(HostEventDraft::Token {
        delta: "чужой".to_owned(),
        kind: None,
    });
    own.emit(HostEventDraft::Token {
        delta: "свой".to_owned(),
        kind: None,
    });
    let Some(CliServerMessage::Event { event }) = client.next().await else {
        panic!("an event");
    };
    assert_eq!(event.stream_id, own.id());
}

#[tokio::test]
async fn a_disconnect_from_the_studio_closes_the_pipe_and_a_held_change_is_announced() {
    let dispatch = Arc::new(Recorded::default());
    let (mut client, serving) = connect(Arc::clone(&dispatch), EventBus::default());
    client.send(hello()).await;
    assert!(matches!(
        client.next().await,
        Some(CliServerMessage::Welcome { .. })
    ));
    let notify = dispatch
        .notify
        .lock()
        .expect("notify")
        .clone()
        .expect("a notice sender");
    notify(zvs_contract::CliNotice::Consent { pending: true });
    assert_eq!(
        client.next().await,
        Some(CliServerMessage::Notice {
            notice: zvs_contract::CliNotice::Consent { pending: true }
        })
    );
    dispatch.dropped.cancel();
    assert_eq!(client.next().await, None);
    serving.await.expect("serves");
    assert_eq!(*dispatch.closed.lock().expect("closed"), [7]);
}

#[tokio::test]
async fn a_shown_verification_reaches_a_new_client_and_focus_is_forwarded() {
    let notices = CliNotices::default();
    notices.verification("https://chat.qwen.ai", true);
    let focused = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seen = Arc::clone(&focused);
    notices.on_focus(move || seen.store(true, std::sync::atomic::Ordering::SeqCst));

    let (client, server) = tokio::io::duplex(64 * 1024);
    tokio::spawn(serve(
        server,
        CliClient::default(),
        Arc::new(Recorded::default()),
        EventBus::default(),
        notices.clone(),
        silent_logger(),
    ));
    let (reader, writer) = tokio::io::split(client);
    let mut client = Client {
        lines: BufReader::new(reader).lines(),
        writer,
    };
    client.send(hello()).await;
    assert!(matches!(
        client.next().await,
        Some(CliServerMessage::Welcome { .. })
    ));
    assert!(matches!(
        client.next().await,
        Some(CliServerMessage::Notice {
            notice: zvs_contract::CliNotice::AccountVerification { shown: true, .. }
        })
    ));
    notices.verification("https://chat.qwen.ai", false);
    assert!(matches!(
        client.next().await,
        Some(CliServerMessage::Notice {
            notice: zvs_contract::CliNotice::AccountVerification { shown: false, .. }
        })
    ));
    client.send(json!({ "type": "focusVerification" })).await;
    client
        .send(json!({ "type": "call", "id": 1, "channel": "system.ping" }))
        .await;
    assert!(matches!(
        client.next().await,
        Some(CliServerMessage::Result { id: 1, .. })
    ));
    assert!(focused.load(std::sync::atomic::Ordering::SeqCst));
}
