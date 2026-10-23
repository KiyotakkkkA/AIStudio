use rusqlite::Row;
use zvs_contract::{
    ChatCitationDto, ChatSettings, ChatToolCallDto, DocumentSelectionDto, MessageRole,
    RetrievalMode, Timestamp,
};

use super::columns::{EnumColumn, json_column};

#[derive(Debug, Clone, PartialEq)]
pub struct ConversationRow {
    pub id: String,
    pub title: String,
    pub provider_id: String,
    pub model_id: String,
    pub settings: ChatSettings,
    pub system_prompt: String,
    pub attached_store_ids: Vec<String>,
    pub use_relations: bool,
    pub enabled_tools: Vec<String>,
    pub retrieval_mode: RetrievalMode,
    pub working_folder: Option<String>,
    pub turn_folder: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl ConversationRow {
    pub const COLUMNS: &str = "id, title, provider_id, model_id, settings, system_prompt, \
        attached_store_ids, use_relations, enabled_tools, retrieval_mode, working_folder, turn_folder, \
        created_at, updated_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            title: row.get("title")?,
            provider_id: row.get("provider_id")?,
            model_id: row.get("model_id")?,
            settings: json_column(row, "settings")?,
            system_prompt: row.get("system_prompt")?,
            attached_store_ids: json_column(row, "attached_store_ids")?,
            use_relations: row.get("use_relations")?,
            enabled_tools: json_column(row, "enabled_tools")?,
            retrieval_mode: row.get::<_, EnumColumn<RetrievalMode>>("retrieval_mode")?.0,
            working_folder: row.get("working_folder")?,
            turn_folder: row.get("turn_folder")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MessageRow {
    pub id: String,
    pub conversation_id: String,
    pub role: MessageRole,
    pub content: String,
    pub reasoning: String,
    pub citations: Vec<ChatCitationDto>,
    pub tool_calls: Vec<ChatToolCallDto>,
    pub selections: Vec<DocumentSelectionDto>,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub usage_estimated: bool,
    pub duration_ms: i64,
    pub run_id: Option<String>,
    pub partial: bool,
    pub created_at: Timestamp,
}

impl MessageRow {
    pub const COLUMNS: &str = "id, conversation_id, role, content, reasoning, citations, \
        tool_calls, selections, tokens_in, tokens_out, usage_estimated, duration_ms, run_id, partial, \
        created_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            conversation_id: row.get("conversation_id")?,
            role: row.get::<_, EnumColumn<MessageRole>>("role")?.0,
            content: row.get("content")?,
            reasoning: row.get("reasoning")?,
            citations: json_column(row, "citations")?,
            tool_calls: json_column(row, "tool_calls")?,
            selections: json_column(row, "selections")?,
            tokens_in: row.get("tokens_in")?,
            tokens_out: row.get("tokens_out")?,
            usage_estimated: row.get("usage_estimated")?,
            duration_ms: row.get("duration_ms")?,
            run_id: row.get("run_id")?,
            partial: row.get("partial")?,
            created_at: row.get("created_at")?,
        })
    }
}
