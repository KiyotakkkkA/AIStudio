use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::BoxStream;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, DeltaKind, ModelCapability, TunableParameter, VendorTurnOptions, WebSourceDto,
};

use super::capabilities::AdapterCapabilities;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

impl ChatRole {
    pub const ALL: &[Self] = &[Self::System, Self::User, Self::Assistant, Self::Tool];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl ToolCall {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments: arguments.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    pub tool_call_id: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

impl ChatMessage {
    pub fn new(role: ChatRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::new(ChatRole::System, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new(ChatRole::User, content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(ChatRole::Assistant, content)
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: Some(tool_call_id.into()),
            ..Self::new(ChatRole::Tool, content)
        }
    }

    #[must_use]
    pub fn with_tool_calls(mut self, tool_calls: Vec<ToolCall>) -> Self {
        self.tool_calls = tool_calls;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct GenerateRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub system: Option<String>,
    pub tools: Vec<ToolSpec>,
    pub temperature: Option<f64>,
    pub top_k: Option<u32>,
    pub top_p: Option<f64>,
    pub max_output_tokens: Option<u64>,
    pub vendor: VendorTurnOptions,
}

impl GenerateRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            ..Self::default()
        }
    }

    pub fn requested_parameters(&self) -> Vec<TunableParameter> {
        [
            (self.temperature.is_some(), TunableParameter::Temperature),
            (self.top_k.is_some(), TunableParameter::TopK),
            (self.top_p.is_some(), TunableParameter::TopP),
            (
                self.max_output_tokens.is_some(),
                TunableParameter::MaxOutputTokens,
            ),
        ]
        .into_iter()
        .filter_map(|(present, parameter)| present.then_some(parameter))
        .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    Cancelled,
    Unknown,
}

impl FinishReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Length => "length",
            Self::ToolCalls => "tool_calls",
            Self::Cancelled => "cancelled",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TokenUsage {
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenerateResult {
    pub model: String,
    pub text: String,
    pub reasoning: Option<String>,
    pub finish_reason: FinishReason,
    pub usage: Option<TokenUsage>,
    pub tool_calls: Vec<ToolCall>,
}

/// Usage a vendor reported for one model call. Counts add up across calls in a turn; the
/// context window describes the latest call.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TurnUsage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub context_window: Option<i64>,
}

impl TurnUsage {
    pub fn merge(&mut self, next: Self) {
        let add = |total: Option<i64>, more: Option<i64>| match (total, more) {
            (Some(total), Some(more)) => Some(total.saturating_add(more)),
            (total, more) => total.or(more),
        };
        self.input_tokens = add(self.input_tokens, next.input_tokens);
        self.output_tokens = add(self.output_tokens, next.output_tokens);
        if next.context_window.is_some() {
            self.context_window = next.context_window;
        }
    }

    pub const fn counted(&self) -> bool {
        self.input_tokens.is_some() || self.output_tokens.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextDelta {
    Content { text: String, kind: DeltaKind },
    ToolCall(ToolCall),
    WebSources(Vec<WebSourceDto>),
    Usage(TurnUsage),
}

impl TextDelta {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Content {
            text: text.into(),
            kind: DeltaKind::Text,
        }
    }

    pub fn reasoning(text: impl Into<String>) -> Self {
        Self::Content {
            text: text.into(),
            kind: DeltaKind::Reasoning,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredModel {
    pub is_free: Option<bool>,
    pub no_training: Option<bool>,
    pub external_id: String,
    pub display_name: String,
    pub family: Option<String>,
    pub context_window: Option<i64>,
    pub max_output: Option<i64>,
    pub size_bytes: Option<i64>,
    pub capabilities: Vec<ModelCapability>,
}

impl DiscoveredModel {
    pub fn named(external_id: impl Into<String>) -> Self {
        let external_id = external_id.into();
        Self {
            is_free: None,
            no_training: None,
            display_name: external_id.clone(),
            external_id,
            family: None,
            context_window: None,
            max_output: None,
            size_bytes: None,
            capabilities: Vec::new(),
        }
    }
}

pub type DeltaStream = BoxStream<'static, Result<TextDelta, AppError>>;

#[async_trait]
pub trait TextGenerationDriver: Send + Sync {
    fn capabilities(&self) -> AdapterCapabilities;

    async fn usage(
        &self,
        _cancel: &CancellationToken,
    ) -> Result<Option<zvs_contract::ProviderUsageDto>, AppError> {
        Ok(None)
    }

    async fn generate(
        &self,
        request: &GenerateRequest,
        cancel: &CancellationToken,
    ) -> Result<GenerateResult, AppError>;

    fn stream(&self, request: &GenerateRequest, cancel: &CancellationToken) -> DeltaStream;

    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError>;

    fn excluded_policy_check_tools(&self) -> &'static [&'static str] {
        &[]
    }
}

#[async_trait]
pub trait EmbeddingDriver: Send + Sync {
    fn capabilities(&self) -> AdapterCapabilities;

    async fn embed(
        &self,
        texts: &[String],
        model: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<Vec<f32>>, AppError>;

    fn dimensions(&self) -> Option<usize>;

    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRequest {
    pub model: String,
    pub prompt: String,
    pub count: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedImage {
    pub mime_type: String,
    pub data: Vec<u8>,
}

#[async_trait]
pub trait ImageDriver: Send + Sync {
    fn capabilities(&self) -> AdapterCapabilities;

    async fn generate_images(
        &self,
        request: &ImageRequest,
        cancel: &CancellationToken,
    ) -> Result<Vec<GeneratedImage>, AppError>;

    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError>;
}

pub type SharedTextDriver = Arc<dyn TextGenerationDriver>;
pub type SharedEmbeddingDriver = Arc<dyn EmbeddingDriver>;
pub type SharedImageDriver = Arc<dyn ImageDriver>;

#[derive(Clone, Default)]
pub struct AiDriver {
    pub text: Option<SharedTextDriver>,
    pub embedding: Option<SharedEmbeddingDriver>,
    pub image: Option<SharedImageDriver>,
}

impl std::fmt::Debug for AiDriver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AiDriver")
            .field("text", &self.text.is_some())
            .field("embedding", &self.embedding.is_some())
            .field("image", &self.image.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requested_parameters_list_only_what_is_set() {
        let request = GenerateRequest {
            temperature: Some(0.4),
            top_k: Some(40),
            ..GenerateRequest::new("m", vec![ChatMessage::user("x")])
        };
        assert_eq!(
            request.requested_parameters(),
            [TunableParameter::Temperature, TunableParameter::TopK]
        );
        assert!(
            GenerateRequest::new("m", Vec::new())
                .requested_parameters()
                .is_empty()
        );
    }

    #[test]
    fn roles_and_finish_reasons_use_the_wire_names() {
        assert_eq!(
            ChatRole::ALL
                .iter()
                .map(|role| role.as_str())
                .collect::<Vec<_>>(),
            ["system", "user", "assistant", "tool"]
        );
        assert_eq!(FinishReason::ToolCalls.as_str(), "tool_calls");
        let tool = ChatMessage::tool("call_1", "done");
        assert_eq!(tool.tool_call_id.as_deref(), Some("call_1"));
        assert_eq!(DiscoveredModel::named("m").display_name, "m");
    }
}
