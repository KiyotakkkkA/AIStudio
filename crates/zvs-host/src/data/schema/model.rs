use rusqlite::Row;
use zvs_contract::{ModelCapability, Timestamp};

use super::columns::{json_column, qualified};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRow {
    pub id: String,
    pub provider_id: String,
    pub external_id: String,
    pub is_free: Option<bool>,
    pub no_training: Option<bool>,
    pub display_name: String,
    pub family: Option<String>,
    pub context_window: Option<i64>,
    pub max_output: Option<i64>,
    pub size_bytes: Option<i64>,
    pub capabilities: Vec<ModelCapability>,
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub discovered_at: Timestamp,
}

impl ModelRow {
    pub const COLUMNS: &str = "id, provider_id, external_id, is_free, no_training, display_name, family, context_window, max_output, size_bytes, capabilities, available, unavailable_reason, discovered_at";

    pub fn qualified_columns() -> String {
        qualified("model", Self::COLUMNS)
    }

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            provider_id: row.get("provider_id")?,
            external_id: row.get("external_id")?,
            is_free: row.get("is_free")?,
            no_training: row.get("no_training")?,
            display_name: row.get("display_name")?,
            family: row.get("family")?,
            context_window: row.get("context_window")?,
            max_output: row.get("max_output")?,
            size_bytes: row.get("size_bytes")?,
            capabilities: json_column(row, "capabilities")?,
            available: row.get("available")?,
            unavailable_reason: row.get("unavailable_reason")?,
            discovered_at: row.get("discovered_at")?,
        })
    }
}
