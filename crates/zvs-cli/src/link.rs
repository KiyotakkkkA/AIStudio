use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};
use zvs_contract::{
    AppError, AppErrorCode, CLI_PROTOCOL_VERSION, CliClientMessage, CliNotice, CliServerMessage,
    HostEvent,
};

use crate::copy;

#[derive(Default)]
struct Waiters {
    closed: bool,
    waiting: HashMap<u64, oneshot::Sender<Value>>,
}

type Pending = Arc<Mutex<Waiters>>;

#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Event(HostEvent),
    Notice(CliNotice),
    Closed,
}

#[derive(Clone)]
pub struct Link {
    outgoing: mpsc::UnboundedSender<CliClientMessage>,
    pending: Pending,
    next: Arc<AtomicU64>,
    closed: Arc<AtomicBool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    Closed,
    Protocol,
}

fn disconnected() -> AppError {
    AppError::new(AppErrorCode::Unknown, copy::DISCONNECTED)
}

fn unwrap_envelope(mut envelope: Value) -> Result<Value, AppError> {
    match envelope.get("ok").and_then(Value::as_bool) {
        Some(true) => Ok(envelope
            .get_mut("data")
            .map(Value::take)
            .unwrap_or_default()),
        Some(false) => Err(envelope
            .get_mut("error")
            .map(Value::take)
            .and_then(|error| serde_json::from_value(error).ok())
            .unwrap_or_else(disconnected)),
        None => Err(disconnected()),
    }
}

impl Link {
    pub async fn open<S>(stream: S) -> Result<(Self, mpsc::UnboundedReceiver<Incoming>), LinkError>
    where
        S: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();
        let hello = serde_json::to_string(&CliClientMessage::Hello {
            version: CLI_PROTOCOL_VERSION,
        })
        .map_err(|_| LinkError::Protocol)?;
        writer
            .write_all(format!("{hello}\n").as_bytes())
            .await
            .map_err(|_| LinkError::Closed)?;
        let welcome = lines
            .next_line()
            .await
            .map_err(|_| LinkError::Closed)?
            .ok_or(LinkError::Closed)?;
        match serde_json::from_str::<CliServerMessage>(&welcome) {
            Ok(CliServerMessage::Welcome { version }) if version == CLI_PROTOCOL_VERSION => {}
            _ => return Err(LinkError::Protocol),
        }

        let (outgoing, mut queue) = mpsc::unbounded_channel::<CliClientMessage>();
        let (incoming, events) = mpsc::unbounded_channel();
        let pending: Pending = Arc::default();

        tokio::spawn(async move {
            while let Some(message) = queue.recv().await {
                let Ok(line) = serde_json::to_string(&message) else {
                    continue;
                };
                if writer
                    .write_all(format!("{line}\n").as_bytes())
                    .await
                    .is_err()
                    || writer.flush().await.is_err()
                {
                    break;
                }
            }
        });

        let reading = Arc::clone(&pending);
        let closed = Arc::new(AtomicBool::new(false));
        let ended = Arc::clone(&closed);
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                match serde_json::from_str::<CliServerMessage>(&line) {
                    Ok(CliServerMessage::Result { id, envelope }) => {
                        let waiter = reading
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .waiting
                            .remove(&id);
                        if let Some(waiter) = waiter {
                            let _ = waiter.send(envelope);
                        }
                    }
                    Ok(CliServerMessage::Event { event }) => {
                        if incoming.send(Incoming::Event(event)).is_err() {
                            break;
                        }
                    }
                    Ok(CliServerMessage::Notice { notice }) => {
                        if incoming.send(Incoming::Notice(notice)).is_err() {
                            break;
                        }
                    }
                    Ok(CliServerMessage::Welcome { .. }) | Err(_) => {}
                }
            }
            {
                let mut waiters = reading.lock().unwrap_or_else(PoisonError::into_inner);
                waiters.closed = true;
                waiters.waiting.clear();
            }
            ended.store(true, Ordering::SeqCst);
            let _ = incoming.send(Incoming::Closed);
        });

        Ok((
            Self {
                outgoing,
                pending,
                next: Arc::new(AtomicU64::new(1)),
                closed,
            },
            events,
        ))
    }

    pub async fn call_value(&self, channel: &str, payload: Value) -> Result<Value, AppError> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = oneshot::channel();
        {
            let mut waiters = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
            if waiters.closed {
                return Err(disconnected());
            }
            waiters.waiting.insert(id, sender);
        }
        self.outgoing
            .send(CliClientMessage::Call {
                id,
                channel: channel.to_owned(),
                payload,
            })
            .map_err(|_| disconnected())?;
        let envelope = receiver.await.map_err(|_| disconnected())?;
        unwrap_envelope(envelope)
    }

    pub fn focus_verification(&self) {
        let _ = self.outgoing.send(CliClientMessage::FocusVerification);
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub async fn call<I: Serialize, O: DeserializeOwned>(
        &self,
        channel: &str,
        input: &I,
    ) -> Result<O, AppError> {
        let payload = serde_json::to_value(input).map_err(|_| disconnected())?;
        let value = self.call_value(channel, payload).await?;
        serde_json::from_value(value).map_err(|error| {
            AppError::new(AppErrorCode::Unknown, copy::DISCONNECTED)
                .with_detail("reason", error.to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    use super::*;

    async fn serve_once(
        server: tokio::io::DuplexStream,
        answer: impl Fn(&str, Value) -> Value + Send + 'static,
    ) {
        let (reader, mut writer) = tokio::io::split(server);
        let mut lines = BufReader::new(reader).lines();
        let _ = lines.next_line().await;
        writer
            .write_all(b"{\"type\":\"welcome\",\"version\":1}\n")
            .await
            .expect("writes");
        while let Ok(Some(line)) = lines.next_line().await {
            let message: CliClientMessage = serde_json::from_str(&line).expect("parses");
            if let CliClientMessage::Call {
                id,
                channel,
                payload,
            } = message
            {
                let reply = CliServerMessage::Result {
                    id,
                    envelope: answer(&channel, payload),
                };
                let mut out = serde_json::to_vec(&reply).expect("serialises");
                out.push(b'\n');
                writer.write_all(&out).await.expect("writes");
            }
        }
    }

    #[tokio::test]
    async fn calls_are_matched_to_their_results_and_errors_surface() {
        let (client, server) = tokio::io::duplex(4096);
        tokio::spawn(serve_once(server, |channel, payload| match channel {
            "system.ping" => json!({ "ok": true, "data": payload }),
            _ => json!({ "ok": false, "error": { "code": "NOT_FOUND", "message": "Нет" } }),
        }));
        let (link, _events) = Link::open(client).await.expect("opens");
        let echoed: u32 = link.call("system.ping", &7).await.expect("answers");
        assert_eq!(echoed, 7);
        let failed = link
            .call::<_, Value>("chat.conversations.get", &json!({}))
            .await
            .expect_err("fails");
        assert_eq!(failed.code, AppErrorCode::NotFound);
    }

    #[tokio::test]
    async fn a_wrong_greeting_is_a_protocol_error() {
        let (client, server) = tokio::io::duplex(4096);
        tokio::spawn(async move {
            let (reader, mut writer) = tokio::io::split(server);
            let mut lines = BufReader::new(reader).lines();
            let _ = lines.next_line().await;
            let _ = writer
                .write_all(b"{\"type\":\"welcome\",\"version\":99}\n")
                .await;
        });
        assert_eq!(Link::open(client).await.err(), Some(LinkError::Protocol));
    }

    #[tokio::test]
    async fn a_closed_server_ends_the_event_stream_and_fails_calls() {
        let (client, server) = tokio::io::duplex(4096);
        tokio::spawn(async move {
            let (reader, mut writer) = tokio::io::split(server);
            let mut lines = BufReader::new(reader).lines();
            let _ = lines.next_line().await;
            let _ = writer
                .write_all(b"{\"type\":\"welcome\",\"version\":1}\n")
                .await;
        });
        let (link, mut events) = Link::open(client).await.expect("opens");
        assert_eq!(events.recv().await, Some(Incoming::Closed));
        assert!(link.call_value("system.ping", Value::Null).await.is_err());
    }
}
