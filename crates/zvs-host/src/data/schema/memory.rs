use rusqlite::Row;
use zvs_contract::{MemoryScope, MemorySource, Timestamp};

use super::columns::EnumColumn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRow {
    pub id: String,
    pub scope: MemoryScope,
    pub conversation_id: Option<String>,
    pub content: String,
    pub source: MemorySource,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl MemoryRow {
    pub const COLUMNS: &str = "id, scope, conversation_id, content, source, created_at, updated_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            scope: row.get::<_, EnumColumn<MemoryScope>>("scope")?.0,
            conversation_id: row.get("conversation_id")?,
            content: row.get("content")?,
            source: row.get::<_, EnumColumn<MemorySource>>("source")?.0,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}
