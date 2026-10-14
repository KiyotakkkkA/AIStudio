use rusqlite::Row;
use zvs_contract::{AccountFamily, AccountStatus, Timestamp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRow {
    pub id: String,
    pub adapter: String,
    pub partition: String,
    pub external_id: String,
    pub email_masked: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub token_secret_id: Option<String>,
    pub token_expires_at: Option<i64>,
    pub status: String,
    pub status_detail: Option<String>,
    pub last_checked_at: Option<Timestamp>,
    pub linked_at: Timestamp,
    pub updated_at: Timestamp,
}

impl AccountRow {
    pub const COLUMNS: &str = "id, adapter, partition, external_id, email_masked, display_name, avatar_url, token_secret_id, token_expires_at, status, status_detail, last_checked_at, linked_at, updated_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            adapter: row.get("adapter")?,
            partition: row.get("partition")?,
            external_id: row.get("external_id")?,
            email_masked: row.get("email_masked")?,
            display_name: row.get("display_name")?,
            avatar_url: row.get("avatar_url")?,
            token_secret_id: row.get("token_secret_id")?,
            token_expires_at: row.get("token_expires_at")?,
            status: row.get("status")?,
            status_detail: row.get("status_detail")?,
            last_checked_at: row.get("last_checked_at")?,
            linked_at: row.get("linked_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    pub fn is_linked(&self) -> bool {
        self.status == AccountStatus::Linked.as_str()
    }

    pub fn family(&self) -> Option<AccountFamily> {
        AccountFamily::parse(&self.adapter)
    }

    pub fn account_status(&self) -> Option<AccountStatus> {
        AccountStatus::parse(&self.status)
    }
}
