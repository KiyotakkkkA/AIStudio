use rusqlite::Row;
use zvs_contract::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserSiteRuleRow {
    pub id: String,
    pub origin: String,
    pub action: String,
    pub decision: String,
    pub updated_at: Timestamp,
}

impl BrowserSiteRuleRow {
    pub const COLUMNS: &str = "id, origin, action, decision, updated_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            origin: row.get("origin")?,
            action: row.get("action")?,
            decision: row.get("decision")?,
            updated_at: row.get("updated_at")?,
        })
    }
}
