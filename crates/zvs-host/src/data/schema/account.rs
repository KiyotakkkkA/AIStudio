use rusqlite::Row;
use zvs_contract::{AccountFamily, AccountStatus, Timestamp};

pub const SESSION_EXPIRED_DETAIL: &str = "Session expired — sign in again";

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

    pub fn session_lapsed(&self) -> bool {
        self.status == AccountStatus::NeedsRelink.as_str()
            && self.status_detail.as_deref() == Some(SESSION_EXPIRED_DETAIL)
    }

    pub fn family(&self) -> Option<AccountFamily> {
        AccountFamily::parse(&self.adapter)
    }

    pub fn account_status(&self) -> Option<AccountStatus> {
        AccountStatus::parse(&self.status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(status: AccountStatus, detail: Option<&str>) -> AccountRow {
        AccountRow {
            id: "a".to_owned(),
            adapter: "qwen-web".to_owned(),
            partition: "persist:browser-work".to_owned(),
            external_id: "user-1".to_owned(),
            email_masked: None,
            display_name: None,
            avatar_url: None,
            token_secret_id: None,
            token_expires_at: None,
            status: status.as_str().to_owned(),
            status_detail: detail.map(str::to_owned),
            last_checked_at: None,
            linked_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn only_a_session_the_app_found_expired_counts_as_lapsed() {
        assert!(row(AccountStatus::NeedsRelink, Some(SESSION_EXPIRED_DETAIL)).session_lapsed());
        assert!(!row(AccountStatus::NeedsRelink, None).session_lapsed());
        assert!(
            !row(
                AccountStatus::NeedsRelink,
                Some("Browser is signed in to a different account")
            )
            .session_lapsed()
        );
        assert!(!row(AccountStatus::Linked, Some(SESSION_EXPIRED_DETAIL)).session_lapsed());
    }
}
