use rusqlite::Row;
use zvs_contract::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppGrantRow {
    pub id: String,
    pub app: String,
    pub path: String,
    pub icon: Option<String>,
    pub scope: String,
    pub conversation_id: Option<String>,
    pub conversation_title: Option<String>,
    pub granted_at: Timestamp,
    pub last_used_at: Option<Timestamp>,
}

impl AppGrantRow {
    pub const COLUMNS: &str = "g.id, g.app, g.path, g.icon, g.scope, g.conversation_id, \
         c.title AS conversation_title, g.granted_at, g.last_used_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            app: row.get("app")?,
            path: row.get("path")?,
            icon: row.get("icon")?,
            scope: row.get("scope")?,
            conversation_id: row.get("conversation_id")?,
            conversation_title: row.get("conversation_title")?,
            granted_at: row.get("granted_at")?,
            last_used_at: row.get("last_used_at")?,
        })
    }
}
