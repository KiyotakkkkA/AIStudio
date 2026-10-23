use std::sync::Arc;

use zvs_contract::{
    AppError, AppErrorCode, ClearMemoryInput, ConversationId, CreateMemoryInput,
    MEMORY_ACCOUNT_MAX, MEMORY_CONVERSATION_MAX, MemoryChatDto, MemoryDto, MemoryId, MemoryScope,
    MemorySource, UpdateMemoryInput, Validate, check_memory_content,
};

use super::chat::CONVERSATION_NOT_FOUND;
use crate::data::schema::{ConversationRow, MemoryRow};
use crate::data::{DataError, Database, Repositories};
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;

pub const MEMORY_ENABLED_KEY: &str = "memory.enabled";
pub const MEMORY_MODEL_KEY: &str = "memory.model";
pub const MEMORY_NOT_FOUND: &str = "Запись памяти не найдена";
pub const ACCOUNT_MEMORY_FULL: &str = "Общая память заполнена: удалите ненужные записи";
pub const CONVERSATION_MEMORY_FULL: &str = "Память диалога заполнена: удалите ненужные записи";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemorySwitches {
    pub enabled: bool,
    pub model: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemorySnapshot {
    pub account: Vec<MemoryRow>,
    pub conversation: Vec<MemoryRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedMemory {
    pub row: MemoryRow,
    pub created: bool,
}

#[derive(Clone)]
pub struct MemoryService {
    database: Arc<Database>,
    clock: SharedClock,
}

pub fn normalise(content: &str) -> String {
    content.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn same_fact(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.to_lowercase()
}

fn not_found() -> AppError {
    AppError::new(AppErrorCode::NotFound, MEMORY_NOT_FOUND)
}

fn chat_dto(row: &ConversationRow) -> Option<MemoryChatDto> {
    ConversationId::parse(&row.id).map(|id| MemoryChatDto {
        id,
        title: row.title.clone(),
    })
}

fn memory_dto(row: MemoryRow, conversations: &[ConversationRow]) -> Result<MemoryDto, AppError> {
    let conversation = row.conversation_id.as_deref().and_then(|owner| {
        conversations
            .iter()
            .find(|conversation| conversation.id == owner)
            .and_then(chat_dto)
    });
    if row.scope == MemoryScope::Conversation && conversation.is_none() {
        return Err(not_found());
    }
    Ok(MemoryDto {
        id: MemoryId::parse(&row.id).ok_or_else(not_found)?,
        scope: row.scope,
        conversation,
        content: row.content,
        source: row.source,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

impl MemoryService {
    pub fn new(database: Arc<Database>, clock: SharedClock) -> Self {
        Self { database, clock }
    }

    fn read<T>(
        &self,
        work: impl FnOnce(&Repositories<'_>) -> Result<T, DataError>,
    ) -> Result<T, AppError> {
        self.database
            .with_connection(|connection| work(&Repositories::new(connection)))
    }

    fn dto(&self, row: MemoryRow) -> Result<MemoryDto, AppError> {
        let conversation = match row.conversation_id.as_deref() {
            Some(id) => self.read(|repositories| Ok(repositories.chat.get(id)?))?,
            None => None,
        };
        memory_dto(row, conversation.as_slice())
    }

    pub fn switches(&self) -> Result<MemorySwitches, AppError> {
        self.read(|repositories| {
            let flag = |key: &str| -> Result<bool, DataError> {
                Ok(repositories
                    .settings
                    .get(key)?
                    .and_then(|row| serde_json::from_str::<bool>(&row.value).ok())
                    .unwrap_or(true))
            };
            Ok(MemorySwitches {
                enabled: flag(MEMORY_ENABLED_KEY)?,
                model: flag(MEMORY_MODEL_KEY)?,
            })
        })
    }

    pub fn list(&self) -> Result<Vec<MemoryDto>, AppError> {
        let (rows, conversations) =
            self.read(|repositories| Ok((repositories.memory.list()?, repositories.chat.list()?)))?;
        rows.into_iter()
            .map(|row| memory_dto(row, &conversations))
            .collect()
    }

    pub fn snapshot(&self, conversation_id: &str) -> Result<MemorySnapshot, AppError> {
        self.read(|repositories| {
            Ok(MemorySnapshot {
                account: repositories.memory.list_account()?,
                conversation: repositories.memory.list_conversation(conversation_id)?,
            })
        })
    }

    pub fn create(&self, input: &CreateMemoryInput) -> Result<MemoryDto, AppError> {
        input.validate()?;
        let conversation = input.conversation_id.map(|id| id.to_string());
        let saved = self.save(
            input.scope,
            conversation.as_deref(),
            &input.content,
            MemorySource::User,
        )?;
        self.dto(saved.row)
    }

    pub fn save(
        &self,
        scope: MemoryScope,
        conversation_id: Option<&str>,
        content: &str,
        source: MemorySource,
    ) -> Result<SavedMemory, AppError> {
        let content = normalise(content);
        check_memory_content("content", &content)?;
        let conversation_id = match scope {
            MemoryScope::Account => None,
            MemoryScope::Conversation => Some(
                conversation_id
                    .ok_or_else(|| AppError::new(AppErrorCode::NotFound, CONVERSATION_NOT_FOUND))?,
            ),
        };
        let now = self.clock.now();
        self.database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            let existing = match conversation_id {
                Some(id) => {
                    if repositories.chat.get(id)?.is_none() {
                        return Err(
                            AppError::new(AppErrorCode::NotFound, CONVERSATION_NOT_FOUND).into(),
                        );
                    }
                    repositories.memory.list_conversation(id)?
                }
                None => repositories.memory.list_account()?,
            };
            if let Some(duplicate) = existing
                .iter()
                .find(|row| same_fact(&row.content, &content))
            {
                let row = repositories
                    .memory
                    .update_content(&duplicate.id, &duplicate.content, now)?
                    .ok_or_else(not_found)?;
                return Ok(SavedMemory {
                    row,
                    created: false,
                });
            }
            let (cap, full) = match scope {
                MemoryScope::Account => (MEMORY_ACCOUNT_MAX, ACCOUNT_MEMORY_FULL),
                MemoryScope::Conversation => (MEMORY_CONVERSATION_MAX, CONVERSATION_MEMORY_FULL),
            };
            if existing.len() >= cap {
                return Err(AppError::new(AppErrorCode::Conflict, full).into());
            }
            let row = repositories.memory.create(&MemoryRow {
                id: new_id().hyphenated().to_string(),
                scope,
                conversation_id: conversation_id.map(str::to_owned),
                content: content.clone(),
                source,
                created_at: now,
                updated_at: now,
            })?;
            Ok(SavedMemory { row, created: true })
        })
    }

    pub fn update(&self, input: &UpdateMemoryInput) -> Result<MemoryDto, AppError> {
        input.validate()?;
        let row = self.rewrite(&input.id.to_string(), &input.content)?;
        self.dto(row)
    }

    pub fn rewrite(&self, id: &str, content: &str) -> Result<MemoryRow, AppError> {
        let content = normalise(content);
        check_memory_content("content", &content)?;
        let now = self.clock.now();
        self.read(|repositories| Ok(repositories.memory.update_content(id, &content, now)?))?
            .ok_or_else(not_found)
    }

    pub fn remove(&self, id: &str) -> Result<(), AppError> {
        if self.read(|repositories| Ok(repositories.memory.remove(id)?))? {
            Ok(())
        } else {
            Err(not_found())
        }
    }

    pub fn clear(&self, input: &ClearMemoryInput) -> Result<(), AppError> {
        input.validate()?;
        self.read(|repositories| {
            match input.conversation_id {
                Some(id) => repositories.memory.clear_conversation(&id.to_string())?,
                None => repositories.memory.clear_account()?,
            };
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests;
