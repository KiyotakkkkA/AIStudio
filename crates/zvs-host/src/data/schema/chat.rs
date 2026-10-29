use rusqlite::Row;
use zvs_contract::{
    ChatCitationDto, ChatCommand, ChatRoundDto, ChatSettings, ChatToolCallDto,
    DocumentSelectionDto, MessageRole, OfferedToolDto, RetrievalMode, Timestamp, WebSourceDto,
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
    pub retrieval_mode: RetrievalMode,
    pub working_folder: Option<String>,
    pub turn_folder: Option<String>,
    pub profile: String,
    pub pinned: bool,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl ConversationRow {
    pub const COLUMNS: &str = "id, title, provider_id, model_id, settings, system_prompt, \
        attached_store_ids, use_relations, retrieval_mode, working_folder, turn_folder, \
        profile, pinned, created_at, updated_at";

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
            retrieval_mode: row.get::<_, EnumColumn<RetrievalMode>>("retrieval_mode")?.0,
            working_folder: row.get("working_folder")?,
            turn_folder: row.get("turn_folder")?,
            profile: row.get("profile")?,
            pinned: row.get("pinned")?,
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
    pub command: Option<ChatCommand>,
    pub offered_tools: Vec<OfferedToolDto>,
    pub web_sources: Vec<WebSourceDto>,
    pub rounds: Vec<ChatRoundDto>,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub usage_estimated: bool,
    pub duration_ms: i64,
    pub run_id: Option<String>,
    pub partial: bool,
    pub pinned: bool,
    pub created_at: Timestamp,
}

impl MessageRow {
    pub const COLUMNS: &str = "id, conversation_id, role, content, reasoning, citations, \
        (SELECT json_group_array(json(call) ORDER BY idx) FROM message_tool_call \
        WHERE message_id = message.id) AS tool_calls, selections, command, offered_tools, web_sources, rounds, tokens_in, tokens_out, usage_estimated, duration_ms, run_id, partial, \
        pinned, created_at";

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
            command: row
                .get::<_, Option<EnumColumn<ChatCommand>>>("command")?
                .map(|column| column.0),
            offered_tools: json_column(row, "offered_tools")?,
            web_sources: json_column(row, "web_sources")?,
            rounds: json_column(row, "rounds")?,
            tokens_in: row.get("tokens_in")?,
            tokens_out: row.get("tokens_out")?,
            usage_estimated: row.get("usage_estimated")?,
            duration_ms: row.get("duration_ms")?,
            run_id: row.get("run_id")?,
            partial: row.get("partial")?,
            pinned: row.get("pinned")?,
            created_at: row.get("created_at")?,
        })
    }
}
