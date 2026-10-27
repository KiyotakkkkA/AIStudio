use rusqlite::{Connection, OptionalExtension, params};
use zvs_contract::Timestamp;

use crate::data::schema::MemoryRow;
use crate::data::schema::columns::EnumColumn;

pub struct MemoryRepository<'c> {
    connection: &'c Connection,
}

impl<'c> MemoryRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self) -> rusqlite::Result<Vec<MemoryRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM memory ORDER BY updated_at DESC, id",
                MemoryRow::COLUMNS
            ))?
            .query_map([], MemoryRow::from_row)?
            .collect()
    }

    pub fn list_account(&self) -> rusqlite::Result<Vec<MemoryRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM memory WHERE scope = 'account' ORDER BY updated_at DESC, id",
                MemoryRow::COLUMNS
            ))?
            .query_map([], MemoryRow::from_row)?
            .collect()
    }

    pub fn list_conversation(&self, conversation_id: &str) -> rusqlite::Result<Vec<MemoryRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM memory WHERE conversation_id = ?1 ORDER BY updated_at DESC, id",
                MemoryRow::COLUMNS
            ))?
            .query_map([conversation_id], MemoryRow::from_row)?
            .collect()
    }

    pub fn find(&self, id: &str) -> rusqlite::Result<Option<MemoryRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM memory WHERE id = ?1",
                MemoryRow::COLUMNS
            ))?
            .query_row([id], MemoryRow::from_row)
            .optional()
    }

    pub fn create(&self, row: &MemoryRow) -> rusqlite::Result<MemoryRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO memory ({columns}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
                 RETURNING {columns}",
                columns = MemoryRow::COLUMNS
            ))?
            .query_row(
                params![
                    row.id,
                    EnumColumn(row.scope),
                    row.conversation_id,
                    row.content,
                    EnumColumn(row.source),
                    row.created_at,
                    row.updated_at,
                ],
                MemoryRow::from_row,
            )
    }

    pub fn update_content(
        &self,
        id: &str,
        content: &str,
        updated_at: Timestamp,
    ) -> rusqlite::Result<Option<MemoryRow>> {
        self.connection
            .prepare_cached(&format!(
                "UPDATE memory SET content = ?2, updated_at = ?3 WHERE id = ?1 RETURNING {}",
                MemoryRow::COLUMNS
            ))?
            .query_row(params![id, content, updated_at], MemoryRow::from_row)
            .optional()
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("DELETE FROM memory WHERE id = ?1")?
            .execute([id])?
            > 0)
    }

    pub fn clear_account(&self) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached("DELETE FROM memory WHERE scope = 'account'")?
            .execute([])
    }

    pub fn clear_conversation(&self, conversation_id: &str) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached("DELETE FROM memory WHERE conversation_id = ?1")?
            .execute([conversation_id])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Repositories;
    use crate::data::repositories::ProviderDraft;
    use crate::data::schema::ConversationRow;
    use crate::testing::temporary_database;
    use zvs_contract::{
        ChatSettings, MemoryScope, MemorySource, ProviderCapability, ProviderKind, RetrievalMode,
    };

    fn conversation(id: &str, provider_id: &str) -> ConversationRow {
        ConversationRow {
            id: id.to_owned(),
            title: "Планы".to_owned(),
            provider_id: provider_id.to_owned(),
            model_id: "m".to_owned(),
            settings: ChatSettings::default(),
            system_prompt: String::new(),
            attached_store_ids: Vec::new(),
            use_relations: false,
            retrieval_mode: RetrievalMode::Hybrid,
            working_folder: None,
            turn_folder: None,
            profile: String::new(),
            pinned: false,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn memory(id: &str, conversation_id: Option<&str>, updated_at: Timestamp) -> MemoryRow {
        MemoryRow {
            id: id.to_owned(),
            scope: if conversation_id.is_some() {
                MemoryScope::Conversation
            } else {
                MemoryScope::Account
            },
            conversation_id: conversation_id.map(str::to_owned),
            content: format!("fact {id}"),
            source: MemorySource::Model,
            created_at: 1,
            updated_at,
        }
    }

    fn ids(rows: Vec<MemoryRow>) -> Vec<String> {
        rows.into_iter().map(|row| row.id).collect()
    }

    #[test]
    fn facts_list_by_scope_newest_first_and_follow_their_dialog() {
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
                repositories.chat.create(&conversation("c1", &provider))?;
                repositories.chat.create(&conversation("c2", &provider))?;
                let store = repositories.memory;
                store.create(&memory("a", None, 5))?;
                store.create(&memory("b", None, 9))?;
                store.create(&memory("d", Some("c1"), 3))?;
                store.create(&memory("e", Some("c2"), 4))?;
                assert_eq!(ids(store.list_account()?), ["b", "a"]);
                assert_eq!(ids(store.list_conversation("c1")?), ["d"]);
                assert_eq!(ids(store.list()?), ["b", "a", "e", "d"]);
                let updated = store.update_content("a", "edited", 20)?.expect("the row");
                assert_eq!(
                    (updated.content.as_str(), updated.updated_at),
                    ("edited", 20)
                );
                assert_eq!(store.update_content("missing", "x", 1)?, None);
                let mismatched = MemoryRow {
                    scope: MemoryScope::Account,
                    ..memory("bad", Some("c1"), 1)
                };
                assert!(store.create(&mismatched).is_err());
                repositories.chat.remove("c1")?;
                assert_eq!(store.find("d")?, None);
                assert_eq!(store.clear_conversation("c2")?, 1);
                assert_eq!(store.clear_account()?, 2);
                assert!(!store.remove("a")?);
                assert!(store.list()?.is_empty());
                Ok(())
            })
            .expect("memory rows");
    }
}
