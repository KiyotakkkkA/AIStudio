use rusqlite::Row;
use zvs_contract::{FolderGrantScope, Timestamp};

use super::columns::EnumColumn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderGrantRow {
    pub id: String,
    pub path: String,
    pub scope: FolderGrantScope,
    pub conversation_id: Option<String>,
    pub granted_at: Timestamp,
    pub last_used_at: Option<Timestamp>,
}

impl FolderGrantRow {
    pub const COLUMNS: &str = "id, path, scope, conversation_id, granted_at, last_used_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            path: row.get("path")?,
            scope: row.get::<_, EnumColumn<FolderGrantScope>>("scope")?.0,
            conversation_id: row.get("conversation_id")?,
            granted_at: row.get("granted_at")?,
            last_used_at: row.get("last_used_at")?,
        })
    }
}
