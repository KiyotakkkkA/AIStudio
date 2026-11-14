use rusqlite::Row;
use zvs_contract::{
    ChatCommand, ChatEventDto, ChatGoalDto, ChatPluginDto, ChatRoundDto, ChatSettings,
    ChatSourceDto, ChatToolCallDto, CompactionTrigger, DocumentSelectionDto, MessageRole,
    OfferedToolDto, RetrievalMode, Timestamp,
};

use super::columns::{EnumColumn, json_column, optional_json_column};

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
    pub goal: Option<ChatGoalDto>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl ConversationRow {
    pub const COLUMNS: &str = "id, title, provider_id, model_id, settings, system_prompt, \
        attached_store_ids, use_relations, retrieval_mode, working_folder, turn_folder, \
        profile, pinned, goal, created_at, updated_at";

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
            goal: optional_json_column(row, "goal")?,
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
    pub sources: Vec<ChatSourceDto>,
    pub tool_calls: Vec<ChatToolCallDto>,
    pub images: Vec<zvs_contract::ImageAttachmentDto>,
    pub selections: Vec<DocumentSelectionDto>,
    pub commands: Vec<ChatCommand>,
    pub plugins: Vec<ChatPluginDto>,
    pub offered_tools: Vec<OfferedToolDto>,
    pub rounds: Vec<ChatRoundDto>,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub tokens_cached: i64,
    pub tokens_reasoning: i64,
    pub usage_estimated: bool,
    pub duration_ms: i64,
    pub run_id: Option<String>,
    pub partial: bool,
    pub pinned: bool,
    pub event: Option<ChatEventDto>,
    pub created_at: Timestamp,
    pub recap: Option<String>,
}

impl MessageRow {
    pub const COLUMNS: &str = "id, conversation_id, role, content, reasoning, sources, \
        (SELECT json_group_array(json(call) ORDER BY idx) FROM message_tool_call \
        WHERE message_id = message.id) AS tool_calls, images, selections, commands, plugins, offered_tools, rounds, tokens_in, tokens_out, tokens_cached, tokens_reasoning, usage_estimated, duration_ms, run_id, partial, \
        pinned, event, created_at, recap";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            conversation_id: row.get("conversation_id")?,
            role: row.get::<_, EnumColumn<MessageRole>>("role")?.0,
            content: row.get("content")?,
            reasoning: row.get("reasoning")?,
            sources: json_column(row, "sources")?,
            tool_calls: json_column(row, "tool_calls")?,
            images: json_column(row, "images")?,
            selections: json_column(row, "selections")?,
            commands: json_column(row, "commands")?,
            plugins: json_column(row, "plugins")?,
            offered_tools: json_column(row, "offered_tools")?,
            rounds: json_column(row, "rounds")?,
            tokens_in: row.get("tokens_in")?,
            tokens_out: row.get("tokens_out")?,
            tokens_cached: row.get("tokens_cached")?,
            tokens_reasoning: row.get("tokens_reasoning")?,
            usage_estimated: row.get("usage_estimated")?,
            duration_ms: row.get("duration_ms")?,
            run_id: row.get("run_id")?,
            partial: row.get("partial")?,
            pinned: row.get("pinned")?,
            event: optional_json_column(row, "event")?,
            created_at: row.get("created_at")?,
            recap: row.get("recap")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionRow {
    pub id: String,
    pub conversation_id: String,
    pub boundary_message_id: String,
    pub summary: String,
    pub messages: i64,
    pub tokens_before: i64,
    pub tokens_after: i64,
    pub provider_id: String,
    pub model_id: String,
    pub trigger: CompactionTrigger,
    pub created_at: Timestamp,
}

impl CompactionRow {
    pub const COLUMNS: &str = "id, conversation_id, boundary_message_id, summary, messages, \
        tokens_before, tokens_after, provider_id, model_id, trigger, created_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            conversation_id: row.get("conversation_id")?,
            boundary_message_id: row.get("boundary_message_id")?,
            summary: row.get("summary")?,
            messages: row.get("messages")?,
            tokens_before: row.get("tokens_before")?,
            tokens_after: row.get("tokens_after")?,
            provider_id: row.get("provider_id")?,
            model_id: row.get("model_id")?,
            trigger: row.get::<_, EnumColumn<CompactionTrigger>>("trigger")?.0,
            created_at: row.get("created_at")?,
        })
    }
}
