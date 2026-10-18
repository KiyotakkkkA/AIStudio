use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use rmcp::model::ServerJsonRpcMessage;
use serde_json::{Value, json};
use zvs_contract::McpLogLevel;
use zvs_contract::validate::js_slice;

pub const MALFORMED_SENTINEL: &str = "zvs:malformed-mcp-message";
pub const MALFORMED_LOG: &str = "Сервер прислал сообщение, не отвечающее протоколу";
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
pub const STOPPED: &str = "stopped";

pub type LogHook = Arc<dyn Fn(McpLogLevel, &str) + Send + Sync>;
pub type MalformedHook = Arc<dyn Fn(&str) + Send + Sync>;
pub type ClosedHook = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone)]
pub struct TransportHooks {
    pub log: LogHook,
    pub malformed: MalformedHook,
    closed: Arc<Closer>,
}

struct Closer {
    fired: AtomicBool,
    reason: Mutex<Option<String>>,
    hook: ClosedHook,
}

impl TransportHooks {
    pub fn new(log: LogHook, malformed: MalformedHook, closed: ClosedHook) -> Self {
        Self {
            log,
            malformed,
            closed: Arc::new(Closer {
                fired: AtomicBool::new(false),
                reason: Mutex::new(None),
                hook: closed,
            }),
        }
    }

    pub fn silent() -> Self {
        Self::new(Arc::new(|_, _| {}), Arc::new(|_| {}), Arc::new(|_| {}))
    }

    pub fn log(&self, level: McpLogLevel, line: &str) {
        (self.log)(level, line);
    }

    pub fn report(&self, payload: &str) {
        (self.malformed)(js_slice(payload, 4000));
        self.log(McpLogLevel::Error, MALFORMED_LOG);
    }

    pub fn close(&self, reason: &str) {
        if self.closed.fired.swap(true, Ordering::SeqCst) {
            return;
        }
        *self
            .closed
            .reason
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(reason.to_owned());
        (self.closed.hook)(reason);
    }

    pub fn closed_reason(&self) -> Option<String> {
        self.closed
            .reason
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn is_closed(&self) -> bool {
        self.closed.fired.load(Ordering::SeqCst)
    }
}

pub fn decode_message(value: Value, hooks: &TransportHooks) -> Option<ServerJsonRpcMessage> {
    match serde_json::from_value::<ServerJsonRpcMessage>(value.clone()) {
        Ok(message) => Some(message),
        Err(_) => {
            if value.get("method").is_some() {
                return None;
            }
            hooks.report(&value.to_string());
            let id = value
                .get("id")
                .filter(|id| id.is_string() || id.is_number())?;
            serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32603, "message": MALFORMED_SENTINEL },
            }))
            .ok()
        }
    }
}

pub fn decode_text(text: &str, hooks: &TransportHooks) -> Option<ServerJsonRpcMessage> {
    match serde_json::from_str::<Value>(text) {
        Ok(value) => decode_message(value, hooks),
        Err(_) => {
            hooks.log(
                McpLogLevel::Error,
                &format!("Нечитаемый ответ: {}", js_slice(text, 500)),
            );
            None
        }
    }
}
