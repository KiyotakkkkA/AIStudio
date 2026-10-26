use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use zvs_contract::{ChatCitationDto, ChatSettings, ChatToolCallDto, RetrievalMode, Timestamp};

use crate::data::schema::columns::{EnumColumn, json_text};
use crate::data::schema::{ConversationRow, MessageRow};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConversationPatch {
    pub title: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub settings: Option<ChatSettings>,
    pub attached_store_ids: Option<Vec<String>>,
    pub use_relations: Option<bool>,
    pub enabled_tools: Option<Vec<String>>,
    pub retrieval_mode: Option<RetrievalMode>,
    pub working_folder: Option<Option<String>>,
    pub turn_folder: Option<Option<String>>,
    pub updated_at: Option<Timestamp>,
}

impl ConversationPatch {
    pub fn touched(now: Timestamp) -> Self {
        Self {
            updated_at: Some(now),
            ..Self::default()
        }
    }

    fn assignments(&self) -> rusqlite::Result<Vec<(&'static str, SqlValue)>> {
        let mut assignments = Vec::new();
        if let Some(title) = &self.title {
            assignments.push(("title", SqlValue::Text(title.clone())));
        }
        if let Some(provider_id) = &self.provider_id {
            assignments.push(("provider_id", SqlValue::Text(provider_id.clone())));
        }
        if let Some(model_id) = &self.model_id {
            assignments.push(("model_id", SqlValue::Text(model_id.clone())));
        }
        if let Some(settings) = &self.settings {
            assignments.push(("settings", SqlValue::Text(json_text(settings)?)));
        }
        if let Some(ids) = &self.attached_store_ids {
            assignments.push(("attached_store_ids", SqlValue::Text(json_text(ids)?)));
        }
        if let Some(use_relations) = self.use_relations {
            assignments.push(("use_relations", SqlValue::Integer(i64::from(use_relations))));
        }
        if let Some(keys) = &self.enabled_tools {
            assignments.push(("enabled_tools", SqlValue::Text(json_text(keys)?)));
        }
        if let Some(mode) = self.retrieval_mode {
            assignments.push(("retrieval_mode", SqlValue::Text(mode.as_str().to_owned())));
        }
        if let Some(folder) = &self.working_folder {
            assignments.push((
                "working_folder",
                folder.clone().map_or(SqlValue::Null, SqlValue::Text),
            ));
        }
        if let Some(folder) = &self.turn_folder {
            assignments.push((
                "turn_folder",
                folder.clone().map_or(SqlValue::Null, SqlValue::Text),
            ));
        }
        if let Some(updated_at) = self.updated_at {
            assignments.push(("updated_at", SqlValue::Integer(updated_at)));
        }
        Ok(assignments)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MessagePatch {
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub citations: Option<Vec<ChatCitationDto>>,
    pub tool_calls: Option<Vec<ChatToolCallDto>>,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub usage_estimated: Option<bool>,
    pub duration_ms: Option<i64>,
    pub run_id: Option<Option<String>>,
    pub partial: Option<bool>,
}

impl MessagePatch {
    fn assignments(&self) -> rusqlite::Result<Vec<(&'static str, SqlValue)>> {
        let mut assignments = Vec::new();
        if let Some(content) = &self.content {
            assignments.push(("content", SqlValue::Text(content.clone())));
        }
        if let Some(reasoning) = &self.reasoning {
            assignments.push(("reasoning", SqlValue::Text(reasoning.clone())));
        }
        if let Some(citations) = &self.citations {
            assignments.push(("citations", SqlValue::Text(json_text(citations)?)));
        }
        if let Some(calls) = &self.tool_calls {
            assignments.push(("tool_calls", SqlValue::Text(json_text(calls)?)));
        }
        if let Some(tokens) = self.tokens_in {
            assignments.push(("tokens_in", SqlValue::Integer(tokens)));
        }
        if let Some(tokens) = self.tokens_out {
            assignments.push(("tokens_out", SqlValue::Integer(tokens)));
        }
        if let Some(estimated) = self.usage_estimated {
            assignments.push(("usage_estimated", SqlValue::Integer(i64::from(estimated))));
        }
        if let Some(duration) = self.duration_ms {
            assignments.push(("duration_ms", SqlValue::Integer(duration)));
        }
        if let Some(run_id) = &self.run_id {
            assignments.push((
                "run_id",
                run_id.clone().map_or(SqlValue::Null, SqlValue::Text),
            ));
        }
        if let Some(partial) = self.partial {
            assignments.push(("partial", SqlValue::Integer(i64::from(partial))));
        }
        Ok(assignments)
    }
}

pub struct ChatRepository<'c> {
    connection: &'c Connection,
}

impl<'c> ChatRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self) -> rusqlite::Result<Vec<ConversationRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM conversation ORDER BY updated_at DESC, id DESC",
                ConversationRow::COLUMNS
            ))?
            .query_map([], ConversationRow::from_row)?
            .collect()
    }

    pub fn get(&self, id: &str) -> rusqlite::Result<Option<ConversationRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM conversation WHERE id = ?1",
                ConversationRow::COLUMNS
            ))?
            .query_row([id], ConversationRow::from_row)
            .optional()
    }

    pub fn create(&self, row: &ConversationRow) -> rusqlite::Result<ConversationRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO conversation (id, title, provider_id, model_id, settings, system_prompt, attached_store_ids, use_relations, enabled_tools, retrieval_mode, working_folder, turn_folder, profile, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15) RETURNING {}",
                ConversationRow::COLUMNS
            ))?
            .query_row(
                params![
                    row.id,
                    row.title,
                    row.provider_id,
                    row.model_id,
                    json_text(&row.settings)?,
                    row.system_prompt,
                    json_text(&row.attached_store_ids)?,
                    row.use_relations,
                    json_text(&row.enabled_tools)?,
                    EnumColumn(row.retrieval_mode),
                    row.working_folder,
                    row.turn_folder,
                    row.profile,
                    row.created_at,
                    row.updated_at,
                ],
                ConversationRow::from_row,
            )
    }

    pub fn update(
        &self,
        id: &str,
        patch: &ConversationPatch,
    ) -> rusqlite::Result<Option<ConversationRow>> {
        let assignments = patch.assignments()?;
        if assignments.is_empty() {
            return self.get(id);
        }
        let set = assignments
            .iter()
            .map(|(column, _)| format!("{column} = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        let values = assignments
            .into_iter()
            .map(|(_, value)| value)
            .chain([SqlValue::Text(id.to_owned())]);
        self.connection
            .prepare_cached(&format!(
                "UPDATE conversation SET {set} WHERE id = ? RETURNING {}",
                ConversationRow::COLUMNS
            ))?
            .query_row(params_from_iter(values), ConversationRow::from_row)
            .optional()
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached("DELETE FROM conversation WHERE id = ?1")?
            .execute([id])?;
        Ok(())
    }

    pub fn messages(&self, conversation_id: &str) -> rusqlite::Result<Vec<MessageRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM message WHERE conversation_id = ?1 ORDER BY created_at ASC, id ASC",
                MessageRow::COLUMNS
            ))?
            .query_map([conversation_id], MessageRow::from_row)?
            .collect()
    }

    pub fn add_message(&self, row: &MessageRow) -> rusqlite::Result<MessageRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO message (id, conversation_id, role, content, reasoning, citations, tool_calls, selections, tokens_in, tokens_out, usage_estimated, duration_ms, run_id, partial, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15) RETURNING {}",
                MessageRow::COLUMNS
            ))?
            .query_row(
                params![
                    row.id,
                    row.conversation_id,
                    row.role.as_str(),
                    row.content,
                    row.reasoning,
                    json_text(&row.citations)?,
                    json_text(&row.tool_calls)?,
                    json_text(&row.selections)?,
                    row.tokens_in,
                    row.tokens_out,
                    row.usage_estimated,
                    row.duration_ms,
                    row.run_id,
                    row.partial,
                    row.created_at,
                ],
                MessageRow::from_row,
            )
    }

    pub fn get_message(&self, id: &str) -> rusqlite::Result<Option<MessageRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM message WHERE id = ?1",
                MessageRow::COLUMNS
            ))?
            .query_row([id], MessageRow::from_row)
            .optional()
    }

    pub fn update_message(&self, id: &str, patch: &MessagePatch) -> rusqlite::Result<()> {
        let assignments = patch.assignments()?;
        if assignments.is_empty() {
            return Ok(());
        }
        let set = assignments
            .iter()
            .map(|(column, _)| format!("{column} = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        let values = assignments
            .into_iter()
            .map(|(_, value)| value)
            .chain([SqlValue::Text(id.to_owned())]);
        self.connection
            .prepare_cached(&format!("UPDATE message SET {set} WHERE id = ?"))?
            .execute(params_from_iter(values))?;
        Ok(())
    }

    pub fn append_message(
        &self,
        id: &str,
        content: &str,
        reasoning: &str,
        tokens_out: i64,
        now: Timestamp,
    ) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached(
                "UPDATE message SET content = content || ?2, reasoning = reasoning || ?3, \
                 tokens_out = tokens_out + ?4, duration_ms = MAX(?5 - created_at, 0) WHERE id = ?1",
            )?
            .execute(params![id, content, reasoning, tokens_out, now])?;
        Ok(())
    }

    pub fn truncate_from(&self, conversation_id: &str, message_id: &str) -> rusqlite::Result<()> {
        let Some(target) = self.get_message(message_id)? else {
            return Ok(());
        };
        if target.conversation_id != conversation_id {
            return Ok(());
        }
        self.connection
            .prepare_cached(
                "DELETE FROM message WHERE conversation_id = ?1 \
                 AND (created_at > ?2 OR (created_at = ?2 AND id >= ?3))",
            )?
            .execute(params![conversation_id, target.created_at, target.id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Repositories;
    use crate::data::repositories::ProviderDraft;
    use crate::testing::temporary_database;
    use zvs_contract::{MessageRole, ProviderCapability, ProviderKind};

    fn conversation(id: &str, provider_id: &str, updated_at: Timestamp) -> ConversationRow {
        ConversationRow {
            id: id.to_owned(),
            title: "Новый диалог".to_owned(),
            provider_id: provider_id.to_owned(),
            model_id: "m".to_owned(),
            settings: ChatSettings {
                temperature: Some(0.5),
                ..ChatSettings::default()
            },
            system_prompt: String::new(),
            attached_store_ids: Vec::new(),
            use_relations: false,
            enabled_tools: Vec::new(),
            retrieval_mode: RetrievalMode::Hybrid,
            working_folder: None,
            turn_folder: None,
            profile: String::new(),
            created_at: 1,
            updated_at,
        }
    }

    fn message(id: &str, conversation_id: &str, created_at: Timestamp) -> MessageRow {
        MessageRow {
            id: id.to_owned(),
            conversation_id: conversation_id.to_owned(),
            role: MessageRole::User,
            content: id.to_owned(),
            reasoning: String::new(),
            citations: Vec::new(),
            tool_calls: Vec::new(),
            selections: Vec::new(),
            tokens_in: 1,
            tokens_out: 0,
            usage_estimated: true,
            duration_ms: 0,
            run_id: None,
            partial: false,
            created_at,
        }
    }

    #[test]
    fn conversations_list_newest_first_and_patch_column_by_column() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let provider = repositories
                    .providers
                    .create(&ProviderDraft::new(
                        ProviderKind::Ollama,
                        "Fake",
                        "http://localhost:11434",
                        vec![ProviderCapability::Text],
                        1,
                    ))?
                    .id;
                let chat = repositories.chat;
                chat.create(&conversation("a", &provider, 5))?;
                let created = chat.create(&conversation("b", &provider, 9))?;
                assert_eq!(created.settings.temperature, Some(0.5));
                assert_eq!(
                    chat.list()?
                        .iter()
                        .map(|row| row.id.as_str())
                        .collect::<Vec<_>>(),
                    ["b", "a"]
                );
                let patched = chat
                    .update(
                        "a",
                        &ConversationPatch {
                            title: Some("Plans".to_owned()),
                            enabled_tools: Some(vec!["demo.lookup".to_owned()]),
                            use_relations: Some(true),
                            ..ConversationPatch::touched(20)
                        },
                    )?
                    .expect("the row");
                assert_eq!(patched.title, "Plans");
                assert_eq!(patched.enabled_tools, ["demo.lookup"]);
                assert!(patched.use_relations);
                assert_eq!(patched.updated_at, 20);
                assert_eq!(
                    chat.update("missing", &ConversationPatch::touched(1))?,
                    None
                );
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn messages_order_by_time_truncate_from_a_message_and_cascade_with_the_conversation() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let provider = repositories
                    .providers
                    .create(&ProviderDraft::new(
                        ProviderKind::Ollama,
                        "Fake",
                        "http://localhost:11434",
                        vec![ProviderCapability::Text],
                        1,
                    ))?
                    .id;
                let chat = repositories.chat;
                chat.create(&conversation("c", &provider, 1))?;
                chat.create(&conversation("other", &provider, 1))?;
                for (id, at) in [("m3", 3), ("m1", 1), ("m2b", 2), ("m2a", 2)] {
                    chat.add_message(&message(id, "c", at))?;
                }
                chat.add_message(&message("x", "other", 2))?;
                assert_eq!(
                    chat.messages("c")?
                        .iter()
                        .map(|row| row.id.as_str())
                        .collect::<Vec<_>>(),
                    ["m1", "m2a", "m2b", "m3"]
                );
                chat.update_message(
                    "m1",
                    &MessagePatch {
                        content: Some("edited".to_owned()),
                        partial: Some(true),
                        ..MessagePatch::default()
                    },
                )?;
                let edited = chat.get_message("m1")?.expect("the message");
                assert_eq!(edited.content, "edited");
                assert!(edited.partial);
                chat.truncate_from("other", "m2b")?;
                assert_eq!(chat.messages("c")?.len(), 4);
                chat.truncate_from("c", "m2b")?;
                assert_eq!(
                    chat.messages("c")?
                        .iter()
                        .map(|row| row.id.as_str())
                        .collect::<Vec<_>>(),
                    ["m1", "m2a"]
                );
                chat.remove("c")?;
                assert!(chat.messages("c")?.is_empty());
                assert_eq!(chat.messages("other")?.len(), 1);
                Ok(())
            })
            .expect("the repository works");
    }
}
