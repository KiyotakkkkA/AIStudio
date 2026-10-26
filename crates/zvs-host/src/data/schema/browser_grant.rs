use rusqlite::Row;
use zvs_contract::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserGrantRow {
    pub id: String,
    pub origin: String,
    pub capability: String,
    pub granted_at: Timestamp,
}

impl BrowserGrantRow {
    pub const COLUMNS: &str = "id, origin, capability, granted_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            origin: row.get("origin")?,
            capability: row.get("capability")?,
            granted_at: row.get("granted_at")?,
        })
    }
}
