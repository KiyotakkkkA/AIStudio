use rusqlite::Row;
use zvs_contract::{AccountStatus, Timestamp};

#[derive(Debug, Clone, PartialEq)]
pub struct AccountRow {
    pub id: String,
    pub adapter: String,
    pub partition: String,
    pub external_id: String,
    pub email_masked: Option<String>,
    pub display_name: Option<String>,
    pub token_expires_at: Option<f64>,
    pub status: String,
    pub linked_at: Timestamp,
    pub updated_at: Timestamp,
}

impl AccountRow {
    pub const COLUMNS: &str = "id, adapter, partition, external_id, email_masked, display_name, token_expires_at, status, linked_at, updated_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            adapter: row.get("adapter")?,
            partition: row.get("partition")?,
            external_id: row.get("external_id")?,
            email_masked: row.get("email_masked")?,
            display_name: row.get("display_name")?,
            token_expires_at: row.get("token_expires_at")?,
            status: row.get("status")?,
            linked_at: row.get("linked_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    pub fn is_linked(&self) -> bool {
        self.status == AccountStatus::Linked.as_str()
    }
}
