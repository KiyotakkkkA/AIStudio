use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, CLI_LINE_MAX, CLI_PROTOCOL_VERSION, CliClientMessage, CliServerMessage,
    HostEvent, ResultEnvelope,
};

use super::connections::{CliClient, NoticeSender};
use super::gate;
use super::notices::CliNotices;
use crate::ipc::{CLI_CHANNEL_REFUSED, Caller, IpcServer, policy};
use crate::platform::events::{EventBus, EventSink};
use crate::platform::logger::{LogLevel, SharedLogger};

const OUTGOING: usize = 4096;
const LOG_SOURCE: &str = "cli";

#[async_trait]
pub trait CliDispatch: Send + Sync {
    async fn dispatch(&self, caller: Caller, channel: &str, payload: Value) -> Value;

    fn connect(&self, client: CliClient, notify: NoticeSender) -> (u64, CancellationToken);

    fn disconnect(&self, connection: u64);

    fn sees(&self, connection: u64, event: &HostEvent) -> bool;
}

#[async_trait]
impl CliDispatch for IpcServer {
    async fn dispatch(&self, caller: Caller, channel: &str, payload: Value) -> Value {
        Self::dispatch(self, caller, channel, payload).await
    }

    fn connect(&self, client: CliClient, notify: NoticeSender) -> (u64, CancellationToken) {
        self.services().cli_connections.open(client, notify)
    }

    fn disconnect(&self, connection: u64) {
        self.services().cli_connections.close(connection);
    }

    fn sees(&self, connection: u64, event: &HostEvent) -> bool {
        gate::sees(self.services(), connection, event.stream_id)
    }
}

fn console_channel(channel: &str) -> bool {
    policy(channel).is_some_and(|row| row.callers.cli)
}

struct QueueSink {
    outgoing: mpsc::Sender<CliServerMessage>,
    closed: CancellationToken,
}

impl EventSink for QueueSink {
    fn deliver(&self, event: &HostEvent) -> Result<(), String> {
        self.outgoing
            .try_send(CliServerMessage::Event {
                event: event.clone(),
            })
            .map_err(|error| {
                self.closed.cancel();
                error.to_string()
            })
    }
}

fn refused(channel: &str) -> Value {
    serde_json::to_value(ResultEnvelope::<()>::Err(
        AppError::new(AppErrorCode::PermissionDenied, CLI_CHANNEL_REFUSED)
            .with_detail("channel", channel),
    ))
    .unwrap_or_else(|_| json!({ "ok": false }))
}

async fn read_message<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    buffer: &mut Vec<u8>,
) -> Option<Result<CliClientMessage, String>> {
    buffer.clear();
    let limit = u64::try_from(CLI_LINE_MAX).unwrap_or(u64::MAX) + 1;
    match (&mut *reader).take(limit).read_until(b'\n', buffer).await {
        Ok(0) | Err(_) => None,
        Ok(_) if buffer.len() > CLI_LINE_MAX => Some(Err("line too long".to_owned())),
        Ok(_) => Some(serde_json::from_slice(buffer).map_err(|error| error.to_string())),
    }
}

async fn write_messages<W: AsyncWrite + Unpin>(
    mut writer: W,
    mut outgoing: mpsc::Receiver<CliServerMessage>,
    closed: CancellationToken,
    visible: impl Fn(&HostEvent) -> bool,
) {
    loop {
        let message = tokio::select! {
            message = outgoing.recv() => message,
            () = closed.cancelled() => None,
        };
        let Some(message) = message else {
            break;
        };
        if let CliServerMessage::Event { event } = &message
            && !visible(event)
        {
            continue;
        }
        let Ok(mut line) = serde_json::to_vec(&message) else {
            continue;
        };
        line.push(b'\n');
        if writer.write_all(&line).await.is_err() || writer.flush().await.is_err() {
            closed.cancel();
            break;
        }
    }
    let _ = writer.shutdown().await;
}

pub async fn serve<S>(
    stream: S,
    client: CliClient,
    dispatch: Arc<dyn CliDispatch>,
    events: EventBus,
    notices: CliNotices,
    logger: SharedLogger,
) where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (reader, writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);
    let (outgoing, queue) = mpsc::channel(OUTGOING);
    let closed = CancellationToken::new();
    let connection = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let writing = {
        let dispatch = Arc::clone(&dispatch);
        let connection = Arc::clone(&connection);
        tokio::spawn(write_messages(
            writer,
            queue,
            closed.clone(),
            move |event| {
                let connection = connection.load(std::sync::atomic::Ordering::SeqCst);
                connection != 0 && dispatch.sees(connection, event)
            },
        ))
    };
    let mut buffer = Vec::new();

    let hello = tokio::select! {
        message = read_message(&mut reader, &mut buffer) => message,
        () = closed.cancelled() => None,
    };
    match hello {
        Some(Ok(CliClientMessage::Hello { version })) if version == CLI_PROTOCOL_VERSION => {}
        other => {
            logger.log(
                LogLevel::Warn,
                LOG_SOURCE,
                "A CLI client did not greet with this protocol version",
                &[("received", json!(format!("{other:?}")))],
            );
            let _ = outgoing
                .send(CliServerMessage::Welcome {
                    version: CLI_PROTOCOL_VERSION,
                })
                .await;
            drop(outgoing);
            let _ = writing.await;
            return;
        }
    }
    if outgoing
        .send(CliServerMessage::Welcome {
            version: CLI_PROTOCOL_VERSION,
        })
        .await
        .is_err()
    {
        return;
    }
    let notify: NoticeSender = {
        let outgoing = outgoing.clone();
        Arc::new(move |notice| {
            let _ = outgoing.try_send(CliServerMessage::Notice { notice });
        })
    };
    let (connection_id, dropped) = dispatch.connect(client.clone(), notify);
    connection.store(connection_id, std::sync::atomic::Ordering::SeqCst);
    {
        let closed = closed.clone();
        tokio::spawn(async move {
            tokio::select! {
                () = dropped.cancelled() => closed.cancel(),
                () = closed.cancelled() => {}
            }
        });
    }
    let subscription = events.subscribe(Arc::new(QueueSink {
        outgoing: outgoing.clone(),
        closed: closed.clone(),
    }));
    logger.log(
        LogLevel::Info,
        LOG_SOURCE,
        "A CLI client connected",
        &[
            ("connection", json!(connection_id)),
            ("pid", json!(client.pid)),
            ("image", json!(client.image)),
            ("recognised", json!(client.recognised)),
        ],
    );
    for notice in notices.current() {
        let _ = outgoing.send(CliServerMessage::Notice { notice }).await;
    }
    {
        let mut updates = notices.subscribe();
        let outgoing = outgoing.clone();
        let closed = closed.clone();
        tokio::spawn(async move {
            loop {
                let notice = tokio::select! {
                    notice = updates.recv() => notice,
                    () = closed.cancelled() => break,
                };
                match notice {
                    Ok(notice) => {
                        if outgoing
                            .send(CliServerMessage::Notice { notice })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    loop {
        let message = tokio::select! {
            message = read_message(&mut reader, &mut buffer) => message,
            () = closed.cancelled() => None,
        };
        match message {
            None => break,
            Some(Err(reason)) => {
                logger.log(
                    LogLevel::Warn,
                    LOG_SOURCE,
                    "Dropped a CLI client that sent a malformed message",
                    &[("reason", json!(reason))],
                );
                break;
            }
            Some(Ok(CliClientMessage::Hello { .. })) => {}
            Some(Ok(CliClientMessage::FocusVerification)) => notices.focus(),
            Some(Ok(CliClientMessage::Call {
                id,
                channel,
                payload,
            })) => {
                let outgoing = outgoing.clone();
                let dispatch = Arc::clone(&dispatch);
                tokio::spawn(async move {
                    let envelope = if console_channel(&channel) {
                        dispatch
                            .dispatch(
                                Caller::Cli {
                                    connection: connection_id,
                                },
                                &channel,
                                payload,
                            )
                            .await
                    } else {
                        refused(&channel)
                    };
                    let _ = outgoing
                        .send(CliServerMessage::Result { id, envelope })
                        .await;
                });
            }
        }
    }

    events.unsubscribe(subscription);
    dispatch.disconnect(connection_id);
    closed.cancel();
    drop(outgoing);
    let _ = writing.await;
    logger.log(
        LogLevel::Info,
        LOG_SOURCE,
        "A CLI client disconnected",
        &[("connection", json!(connection_id))],
    );
}

#[cfg(test)]
mod tests;
