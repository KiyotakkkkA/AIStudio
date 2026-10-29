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

use super::notices::CliNotices;
use crate::ipc::IpcServer;
use crate::platform::events::{EventBus, EventSink};
use crate::platform::logger::{LogLevel, SharedLogger};

pub const CLI_CHANNELS: &[&str] = &[
    "chat.answer",
    "chat.attach",
    "chat.bindFolder",
    "chat.cancel",
    "chat.context",
    "chat.conversations.create",
    "chat.conversations.get",
    "chat.conversations.list",
    "chat.send",
    "chat.skip",
    "cli.open",
    "execPolicy.allowRule",
    "execPolicy.get",
    "execPolicy.set",
    "folders.answer",
    "mcp.list",
    "mcp.start",
    "mcp.stop",
    "providers.get",
    "providers.list",
    "runs.approve",
    "runs.deny",
    "skills.list",
    "skills.setEnabled",
    "system.ping",
];
pub const CLI_CHANNEL_REFUSED: &str = "Канал недоступен консольной версии";
const OUTGOING: usize = 4096;
const LOG_SOURCE: &str = "cli";

#[async_trait]
pub trait CliDispatch: Send + Sync {
    async fn dispatch(&self, channel: &str, payload: Value) -> Value;
}

#[async_trait]
impl CliDispatch for IpcServer {
    async fn dispatch(&self, channel: &str, payload: Value) -> Value {
        Self::dispatch(self, channel, payload).await
    }
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
) {
    loop {
        let message = tokio::select! {
            message = outgoing.recv() => message,
            () = closed.cancelled() => None,
        };
        let Some(message) = message else {
            break;
        };
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
    let writing = tokio::spawn(write_messages(writer, queue, closed.clone()));
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
    let subscription = events.subscribe(Arc::new(QueueSink {
        outgoing: outgoing.clone(),
        closed: closed.clone(),
    }));
    logger.log(LogLevel::Info, LOG_SOURCE, "A CLI client connected", &[]);
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
                    let envelope = if CLI_CHANNELS.contains(&channel.as_str()) {
                        dispatch.dispatch(&channel, payload).await
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
    closed.cancel();
    drop(outgoing);
    let _ = writing.await;
    logger.log(LogLevel::Info, LOG_SOURCE, "A CLI client disconnected", &[]);
}

#[cfg(test)]
mod tests;
