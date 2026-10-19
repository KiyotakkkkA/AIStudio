use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::{ConversationId, MessageId, ProviderId, RunId, VectorStoreId};
use crate::primitives::{Json, Timestamp};
use crate::tools::check_tool_key;
use crate::validate::{Validate, check, invalid, js_trim, nested};

pub const DEFAULT_CONVERSATION_TITLE: &str = "New conversation";
pub const CONVERSATION_TITLE_MAX: usize = 200;
pub const SYSTEM_PROMPT_MAX: usize = 100_000;
pub const CHAT_TEXT_MAX: usize = 100_000;
pub const ATTACHED_STORES_MAX: usize = 64;
pub const ENABLED_TOOLS_MAX: usize = 64;

wire_enum!(ChatToolCallStatus {
    Running => "running",
    AwaitingApproval => "awaiting-approval",
    Succeeded => "succeeded",
    Failed => "failed",
    Refused => "refused",
    Denied => "denied",
    Cancelled => "cancelled",
});

wire_enum!(MessageRole {
    User => "user",
    Assistant => "assistant",
});

fn positive(field: &str, value: u64) -> Result<(), AppError> {
    check::safe_integer(field, value)?;
    check::int_range(field, value, 1, u64::MAX)
}

fn check_int(field: &str, value: i64) -> Result<(), AppError> {
    check::safe_integer(field, value.unsigned_abs())
}

fn check_title(value: &str) -> Result<(), AppError> {
    check::trimmed_name("title", value, CONVERSATION_TITLE_MAX)
}

pub fn check_enabled_tools(field: &str, keys: &[String]) -> Result<(), AppError> {
    if keys.len() > ENABLED_TOOLS_MAX {
        return Err(invalid(field, "maxItems"));
    }
    for (index, key) in keys.iter().enumerate() {
        check_tool_key(&format!("{field}.{index}"), key)?;
    }
    Ok(())
}

pub fn check_attached_stores(field: &str, ids: &[VectorStoreId]) -> Result<(), AppError> {
    if ids.len() > ATTACHED_STORES_MAX {
        Err(invalid(field, "maxItems"))
    } else {
        Ok(())
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSettings {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub temperature: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub top_k: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub top_p: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub max_output_tokens: Option<u64>,
}

impl Validate for ChatSettings {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(temperature) = self.temperature {
            check::number_range("temperature", temperature, 0.0, 2.0)?;
        }
        if let Some(top_k) = self.top_k {
            positive("topK", top_k)?;
        }
        if let Some(top_p) = self.top_p {
            check::number_range("topP", top_p, 0.0, 1.0)?;
        }
        if let Some(tokens) = self.max_output_tokens {
            positive("maxOutputTokens", tokens)?;
        }
        Ok(())
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatCitationDto {
    pub store_id: VectorStoreId,
    pub document_id: String,
    pub source_path: String,
    pub chunk_index: u64,
    pub score: f64,
    #[serde(default)]
    pub section: String,
    #[serde(default)]
    pub page: Option<u64>,
    #[serde(default)]
    pub related: Option<String>,
}

impl Validate for ChatCitationDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("chunkIndex", self.chunk_index)?;
        check::number_range("score", self.score, 0.0, 1.0)?;
        self.page.map_or(Ok(()), |page| positive("page", page))
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatToolCallDto {
    pub id: String,
    pub key: String,
    pub name: String,
    pub round: u64,
    pub arguments: Json,
    pub status: ChatToolCallStatus,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub approval_id: Option<String>,
    #[serde(default)]
    pub step_id: Option<String>,
    #[serde(default)]
    pub duration_ms: u64,
}

impl Validate for ChatToolCallDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("id", &self.id)?;
        check::max_len("id", &self.id, 128)?;
        check::non_empty("key", &self.key)?;
        check::max_len("key", &self.key, 200)?;
        check::non_empty("name", &self.name)?;
        check::max_len("name", &self.name, 200)?;
        check::safe_integer("round", self.round)?;
        check::safe_integer("durationMs", self.duration_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateConversationInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub title: Option<String>,
    pub provider_id: ProviderId,
    pub model_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub settings: Option<ChatSettings>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub system_prompt: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub attached_store_ids: Option<Vec<VectorStoreId>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub use_relations: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub enabled_tools: Option<Vec<String>>,
}

impl CreateConversationInput {
    pub fn new(provider_id: ProviderId, model_id: impl Into<String>) -> Self {
        Self {
            title: None,
            provider_id,
            model_id: model_id.into(),
            settings: None,
            system_prompt: None,
            attached_store_ids: None,
            use_relations: None,
            enabled_tools: None,
        }
    }

    pub fn title(&self) -> String {
        self.title.as_deref().map_or_else(
            || DEFAULT_CONVERSATION_TITLE.to_owned(),
            |title| js_trim(title).to_owned(),
        )
    }

    pub fn settings(&self) -> ChatSettings {
        self.settings.unwrap_or_default()
    }

    pub fn system_prompt(&self) -> String {
        self.system_prompt.clone().unwrap_or_default()
    }

    pub fn attached_store_ids(&self) -> Vec<VectorStoreId> {
        self.attached_store_ids.clone().unwrap_or_default()
    }

    pub fn use_relations(&self) -> bool {
        self.use_relations.unwrap_or(false)
    }

    pub fn enabled_tools(&self) -> Vec<String> {
        self.enabled_tools
            .iter()
            .flatten()
            .map(|key| js_trim(key).to_owned())
            .collect()
    }
}

impl Validate for CreateConversationInput {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(title) = &self.title {
            check_title(title)?;
        }
        check::non_empty("modelId", &self.model_id)?;
        if let Some(settings) = &self.settings {
            nested("settings", settings.validate())?;
        }
        if let Some(prompt) = &self.system_prompt {
            check::max_len("systemPrompt", prompt, SYSTEM_PROMPT_MAX)?;
        }
        if let Some(ids) = &self.attached_store_ids {
            check_attached_stores("attachedStoreIds", ids)?;
        }
        if let Some(keys) = &self.enabled_tools {
            check_enabled_tools("enabledTools", keys)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRef {
    pub id: ConversationId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDto {
    pub id: ConversationId,
    pub title: String,
    pub provider_id: ProviderId,
    pub model_id: String,
    #[serde(default)]
    pub settings: ChatSettings,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default)]
    pub attached_store_ids: Vec<VectorStoreId>,
    #[serde(default)]
    pub use_relations: bool,
    #[serde(default)]
    pub enabled_tools: Vec<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Validate for ConversationDto {
    fn validate(&self) -> Result<(), AppError> {
        check_title(&self.title)?;
        check::non_empty("modelId", &self.model_id)?;
        nested("settings", self.settings.validate())?;
        check::max_len("systemPrompt", &self.system_prompt, SYSTEM_PROMPT_MAX)?;
        check_attached_stores("attachedStoreIds", &self.attached_store_ids)?;
        check_enabled_tools("enabledTools", &self.enabled_tools)?;
        check_int("createdAt", self.created_at)?;
        check_int("updatedAt", self.updated_at)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDto {
    pub id: MessageId,
    pub conversation_id: ConversationId,
    pub role: MessageRole,
    pub content: String,
    #[serde(default)]
    pub reasoning: String,
    pub citations: Vec<ChatCitationDto>,
    #[serde(default)]
    pub tool_calls: Vec<ChatToolCallDto>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub usage_estimated: bool,
    pub duration_ms: u64,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub run_id: Option<RunId>,
    pub partial: bool,
    pub created_at: Timestamp,
}

impl Validate for MessageDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("citations", self.citations.validate())?;
        nested("toolCalls", self.tool_calls.validate())?;
        check::safe_integer("tokensIn", self.tokens_in)?;
        check::safe_integer("tokensOut", self.tokens_out)?;
        check::safe_integer("durationMs", self.duration_ms)?;
        check_int("createdAt", self.created_at)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDetailDto {
    #[serde(flatten)]
    pub conversation: ConversationDto,
    pub messages: Vec<MessageDto>,
}

impl Validate for ConversationDetailDto {
    fn validate(&self) -> Result<(), AppError> {
        self.conversation.validate()?;
        nested("messages", self.messages.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSendInput {
    pub conversation_id: ConversationId,
    pub text: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub provider_id: Option<ProviderId>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub model_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub settings: Option<ChatSettings>,
}

impl ChatSendInput {
    pub fn new(conversation_id: ConversationId, text: impl Into<String>) -> Self {
        Self {
            conversation_id,
            text: text.into(),
            provider_id: None,
            model_id: None,
            settings: None,
        }
    }

    pub fn text(&self) -> &str {
        js_trim(&self.text)
    }
}

impl Validate for ChatSendInput {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("text", &self.text, CHAT_TEXT_MAX)?;
        if let Some(model_id) = &self.model_id {
            check::non_empty("modelId", model_id)?;
        }
        if let Some(settings) = &self.settings {
            nested("settings", settings.validate())?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatContextInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub conversation_id: Option<ConversationId>,
    pub provider_id: ProviderId,
    pub model_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub settings: Option<ChatSettings>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub system_prompt: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub enabled_tools: Option<Vec<String>>,
}

impl Validate for ChatContextInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("modelId", &self.model_id)?;
        if let Some(settings) = &self.settings {
            nested("settings", settings.validate())?;
        }
        if let Some(prompt) = &self.system_prompt {
            check::max_len("systemPrompt", prompt, SYSTEM_PROMPT_MAX)?;
        }
        if let Some(keys) = &self.enabled_tools {
            check_enabled_tools("enabledTools", keys)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatContextToolDto {
    pub key: String,
    pub tokens: u64,
}

impl Validate for ChatContextToolDto {
    fn validate(&self) -> Result<(), AppError> {
        check_tool_key("key", &self.key)?;
        check::safe_integer("tokens", self.tokens)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatContextDto {
    pub context_window: u64,
    pub reserved_output: u64,
    pub system: u64,
    pub tools: u64,
    pub tool_items: Vec<ChatContextToolDto>,
    pub history: u64,
    pub history_messages: u64,
    pub trimmed: u64,
    pub total: u64,
}

impl Validate for ChatContextDto {
    fn validate(&self) -> Result<(), AppError> {
        positive("contextWindow", self.context_window)?;
        for (field, value) in [
            ("reservedOutput", self.reserved_output),
            ("system", self.system),
            ("tools", self.tools),
            ("history", self.history),
            ("historyMessages", self.history_messages),
            ("trimmed", self.trimmed),
            ("total", self.total),
        ] {
            check::safe_integer(field, value)?;
        }
        nested("toolItems", self.tool_items.validate())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TruncateConversationInput {
    pub id: ConversationId,
    pub message_id: MessageId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameConversationInput {
    pub id: ConversationId,
    pub title: String,
}

impl RenameConversationInput {
    pub fn title(&self) -> &str {
        js_trim(&self.title)
    }
}

impl Validate for RenameConversationInput {
    fn validate(&self) -> Result<(), AppError> {
        check_title(&self.title)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConversationStoresInput {
    pub id: ConversationId,
    pub attached_store_ids: Vec<VectorStoreId>,
}

impl Validate for UpdateConversationStoresInput {
    fn validate(&self) -> Result<(), AppError> {
        check_attached_stores("attachedStoreIds", &self.attached_store_ids)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConversationRelationsInput {
    pub id: ConversationId,
    pub use_relations: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConversationToolsInput {
    pub id: ConversationId,
    pub enabled_tools: Vec<String>,
}

impl UpdateConversationToolsInput {
    pub fn enabled_tools(&self) -> Vec<String> {
        self.enabled_tools
            .iter()
            .map(|key| js_trim(key).to_owned())
            .collect()
    }
}

impl Validate for UpdateConversationToolsInput {
    fn validate(&self) -> Result<(), AppError> {
        check_enabled_tools("enabledTools", &self.enabled_tools)
    }
}

pub type ChatConversationsListOutput = Vec<ConversationDto>;

crate::validate::no_refinements!(
    ConversationRef,
    TruncateConversationInput,
    UpdateConversationRelationsInput
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::parse;
    use serde_json::json;

    const PROVIDER: &str = "0199aa11-1111-7111-8111-111111111111";
    const CONVERSATION: &str = "0199aa11-2222-7111-8111-111111111111";

    #[test]
    fn statuses_and_roles_use_the_wire_names() {
        assert_eq!(
            serde_json::to_value(ChatToolCallStatus::ALL).expect("serialises"),
            json!([
                "running",
                "awaiting-approval",
                "succeeded",
                "failed",
                "refused",
                "denied",
                "cancelled"
            ])
        );
        assert_eq!(
            MessageRole::parse("assistant"),
            Some(MessageRole::Assistant)
        );
    }

    #[test]
    fn a_create_input_applies_its_defaults_and_trims() {
        let input = parse::<CreateConversationInput>(json!({
            "providerId": PROVIDER,
            "modelId": "m",
            "title": "  Plans  ",
            "enabledTools": [" mail.send "]
        }))
        .expect("parses");
        assert_eq!(input.title(), "Plans");
        assert_eq!(input.enabled_tools(), ["mail.send"]);
        assert_eq!(input.settings(), ChatSettings::default());
        assert!(!input.use_relations());
        let bare =
            parse::<CreateConversationInput>(json!({ "providerId": PROVIDER, "modelId": "m" }))
                .expect("parses");
        assert_eq!(bare.title(), DEFAULT_CONVERSATION_TITLE);
        assert!(
            parse::<CreateConversationInput>(
                json!({ "providerId": PROVIDER, "modelId": "m", "title": "   " })
            )
            .is_err()
        );
        assert!(
            parse::<CreateConversationInput>(
                json!({ "providerId": PROVIDER, "modelId": "m", "enabledTools": ["mail"] })
            )
            .is_err()
        );
    }

    #[test]
    fn a_send_input_needs_text_and_valid_settings() {
        let input =
            parse::<ChatSendInput>(json!({ "conversationId": CONVERSATION, "text": " hi " }))
                .expect("parses");
        assert_eq!(input.text(), "hi");
        assert!(
            parse::<ChatSendInput>(json!({ "conversationId": CONVERSATION, "text": "  " }))
                .is_err()
        );
        assert!(
            parse::<ChatSendInput>(json!({
                "conversationId": CONVERSATION,
                "text": "x",
                "settings": { "temperature": 3 }
            }))
            .is_err()
        );
    }

    #[test]
    fn a_message_fills_its_defaults() {
        let message = parse::<MessageDto>(json!({
            "id": CONVERSATION,
            "conversationId": CONVERSATION,
            "role": "user",
            "content": "x",
            "citations": [],
            "tokensIn": 1,
            "tokensOut": 0,
            "usageEstimated": true,
            "durationMs": 0,
            "runId": null,
            "partial": false,
            "createdAt": 1
        }))
        .expect("parses");
        assert_eq!(message.reasoning, "");
        assert!(message.tool_calls.is_empty());
    }
}
