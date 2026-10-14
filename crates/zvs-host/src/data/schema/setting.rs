use rusqlite::Row;
use zvs_contract::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingRow {
    pub key: String,
    pub value: String,
    pub updated_at: Timestamp,
}

impl SettingRow {
    pub const COLUMNS: &str = "key, value, updated_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            key: row.get("key")?,
            value: row.get("value")?,
            updated_at: row.get("updated_at")?,
        })
    }
}
