use std::sync::Arc;

use futures::StreamExt;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AdapterFamily, AppError, AppErrorCode, TunableParameter};

use super::wire::{normalise, open_frames};
use crate::drivers::ai::capabilities::AdapterCapabilities;
use crate::drivers::ai::errors::is_session_expired;
use crate::drivers::ai::ports::{ChatRole, GenerateRequest};
use crate::drivers::ai::sse::{FrameStream, sse_data};
use crate::drivers::ai::transport::{RequestSpec, SharedTransport, TransportResponse};
use crate::platform::clock::{SharedClock, SystemClock};
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};

pub const MALFORMED_MODELS_MESSAGE: &str = "Вендор вернул неизвестный формат списка моделей";
pub const MALFORMED_TURN_MESSAGE: &str = "Вендор вернул неизвестный формат ответа";
pub const TRUNCATED_STREAM_MESSAGE: &str = "Поток вендора оборвался";

pub type NewId = Arc<dyn Fn() -> String + Send + Sync>;
pub type SessionExpiredHook = Arc<dyn Fn(&AppError) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VendorObservation {
    pub source: &'static str,
    pub observed_at: &'static str,
}

pub const fn role_label(role: ChatRole) -> &'static str {
    match role {
        ChatRole::System => "System",
        ChatRole::User => "User",
        ChatRole::Assistant => "Assistant",
        ChatRole::Tool => "Tool",
    }
}

#[derive(Clone)]
pub struct AccountAdapterOptions {
    pub transport: SharedTransport,
    pub logger: SharedLogger,
    pub clock: SharedClock,
    pub new_id: NewId,
    pub on_session_expired: Option<SessionExpiredHook>,
}

impl AccountAdapterOptions {
    pub fn new(transport: SharedTransport) -> Self {
        Self {
            transport,
            logger: silent_logger(),
            clock: Arc::new(SystemClock),
            new_id: Arc::new(|| new_id().hyphenated().to_string()),
            on_session_expired: None,
        }
    }
}

fn details(family: AdapterFamily, endpoint: &str) -> Vec<(&'static str, Value)> {
    vec![
        ("family", json!(family.as_str())),
        ("endpoint", json!(endpoint)),
    ]
}

fn described(code: AppErrorCode, message: &str, fields: Vec<(&'static str, Value)>) -> AppError {
    fields
        .into_iter()
        .fold(AppError::new(code, message), |error, (key, value)| {
            error.with_detail(key, value)
        })
}

pub fn malformed_payload(family: AdapterFamily, endpoint: &str, logger: &SharedLogger) -> AppError {
    let fields = details(family, endpoint);
    logger.log(LogLevel::Warn, "ai", MALFORMED_MODELS_MESSAGE, &fields);
    described(AppErrorCode::Unknown, MALFORMED_MODELS_MESSAGE, fields)
}

pub fn malformed_turn(family: AdapterFamily, endpoint: &str, logger: &SharedLogger) -> AppError {
    let fields = details(family, endpoint);
    logger.log(LogLevel::Warn, "ai", MALFORMED_TURN_MESSAGE, &fields);
    described(AppErrorCode::Unknown, MALFORMED_TURN_MESSAGE, fields)
}

pub fn truncated_stream(family: AdapterFamily, endpoint: &str) -> AppError {
    described(
        AppErrorCode::ProviderUnreachable,
        TRUNCATED_STREAM_MESSAGE,
        details(family, endpoint),
    )
}

pub fn flatten_prompt(request: &GenerateRequest) -> String {
    let system = request.system.as_deref().map(str::trim).unwrap_or_default();
    if system.is_empty()
        && let [only] = request.messages.as_slice()
        && only.role == ChatRole::User
    {
        return only.content.clone();
    }
    let mut parts = Vec::new();
    if !system.is_empty() {
        parts.push(format!("{}: {system}", role_label(ChatRole::System)));
    }
    for message in &request.messages {
        parts.push(format!("{}: {}", role_label(message.role), message.content));
    }
    parts.join("\n\n")
}

#[derive(Clone)]
pub struct AccountAdapterCore {
    pub transport: SharedTransport,
    pub logger: SharedLogger,
    pub clock: SharedClock,
    pub new_id: NewId,
    pub capabilities: AdapterCapabilities,
    on_session_expired: Option<SessionExpiredHook>,
}

impl AccountAdapterCore {
    pub fn new(options: AccountAdapterOptions, capabilities: AdapterCapabilities) -> Self {
        Self {
            transport: options.transport,
            logger: options.logger,
            clock: options.clock,
            new_id: options.new_id,
            capabilities,
            on_session_expired: options.on_session_expired,
        }
    }

    pub fn normalise(&self, error: AppError) -> AppError {
        let failure = normalise(error);
        if is_session_expired(&failure)
            && let Some(hook) = &self.on_session_expired
        {
            hook(&failure);
        }
        failure
    }

    pub async fn request(
        &self,
        spec: &RequestSpec,
        cancel: &CancellationToken,
    ) -> Result<TransportResponse, AppError> {
        self.transport
            .request(spec, cancel)
            .await
            .map_err(|error| self.normalise(error))
    }

    pub fn frames(&self, spec: RequestSpec, cancel: &CancellationToken) -> FrameStream {
        let core = self.clone();
        open_frames(self.transport.clone(), spec, cancel.clone(), sse_data)
            .map(move |frame| frame.map_err(|error| core.normalise(error)))
            .boxed()
    }

    pub fn decode(&self, text: &str, endpoint: &str) -> Result<Value, AppError> {
        serde_json::from_str(text)
            .map_err(|_| malformed_payload(self.capabilities.family, endpoint, &self.logger))
    }

    pub fn drop_unsupported(&self, request: &GenerateRequest) -> Vec<TunableParameter> {
        let dropped = self
            .capabilities
            .unsupported_parameters(&request.requested_parameters());
        if !dropped.is_empty() {
            self.logger.log(
                LogLevel::Debug,
                "ai",
                "Dropped parameters the adapter family does not honour",
                &[
                    ("family", json!(self.capabilities.family)),
                    ("dropped", json!(dropped)),
                ],
            );
        }
        dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::ai::ports::ChatMessage;

    #[test]
    fn the_flattened_prompt_keeps_roles_when_a_turn_carries_history_or_a_system_message() {
        let single = GenerateRequest::new("qwen3.7-plus", vec![ChatMessage::user("Привет")]);
        assert_eq!(flatten_prompt(&single), "Привет");
        let history = GenerateRequest {
            system: Some("Отвечай кратко".to_owned()),
            ..GenerateRequest::new(
                "qwen3.7-plus",
                vec![
                    ChatMessage::user("Привет"),
                    ChatMessage::assistant("Здравствуйте"),
                    ChatMessage::user("Как дела?"),
                ],
            )
        };
        assert_eq!(
            flatten_prompt(&history),
            "System: Отвечай кратко\n\nUser: Привет\n\nAssistant: Здравствуйте\n\nUser: Как дела?"
        );
        let blank_system = GenerateRequest {
            system: Some("   ".to_owned()),
            ..single.clone()
        };
        assert_eq!(flatten_prompt(&blank_system), "Привет");
        let only_assistant =
            GenerateRequest::new("m", vec![ChatMessage::assistant("Здравствуйте")]);
        assert_eq!(flatten_prompt(&only_assistant), "Assistant: Здравствуйте");
    }
}
