use rusqlite::Row;
use zvs_contract::{Timestamp, ToolTier};

use super::columns::EnumColumn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPermissionRow {
    pub id: String,
    pub subject: String,
    pub scope: String,
    pub tier: ToolTier,
    pub granted_at: Timestamp,
    pub granted_by: String,
}

impl ToolPermissionRow {
    pub const COLUMNS: &str = "id, subject, scope, tier, granted_at, granted_by";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            subject: row.get("subject")?,
            scope: row.get("scope")?,
            tier: row.get::<_, EnumColumn<ToolTier>>("tier")?.0,
            granted_at: row.get("granted_at")?,
            granted_by: row.get("granted_by")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionUseRow {
    pub id: String,
    pub grant_id: String,
    pub run_id: String,
    pub used_at: Timestamp,
    pub subject: String,
    pub scope: String,
    pub tier: ToolTier,
    pub granted_by: String,
    pub granted_at: Timestamp,
}

impl PermissionUseRow {
    pub const COLUMNS: &str =
        "id, grant_id, run_id, used_at, subject, scope, tier, granted_by, granted_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            grant_id: row.get("grant_id")?,
            run_id: row.get("run_id")?,
            used_at: row.get("used_at")?,
            subject: row.get("subject")?,
            scope: row.get("scope")?,
            tier: row.get::<_, EnumColumn<ToolTier>>("tier")?.0,
            granted_by: row.get("granted_by")?,
            granted_at: row.get("granted_at")?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApprovalDecision {
    Approved,
    Denied,
}

impl ApprovalDecision {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Denied => "denied",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "approved" => Some(Self::Approved),
            "denied" => Some(Self::Denied),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingApprovalRow {
    pub id: String,
    pub run_id: String,
    pub subject: String,
    pub scope: String,
    pub expires_at: Timestamp,
    pub decision: Option<ApprovalDecision>,
}

impl PendingApprovalRow {
    pub const COLUMNS: &str = "id, run_id, subject, scope, expires_at, decision";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        let decision = row
            .get::<_, Option<String>>("decision")?
            .map(|text| {
                ApprovalDecision::parse(&text).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        row.as_ref().column_index("decision").unwrap_or(0),
                        rusqlite::types::Type::Text,
                        format!("unknown decision «{text}»").into(),
                    )
                })
            })
            .transpose()?;
        Ok(Self {
            id: row.get("id")?,
            run_id: row.get("run_id")?,
            subject: row.get("subject")?,
            scope: row.get("scope")?,
            expires_at: row.get("expires_at")?,
            decision,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolCeilingRow {
    pub tier: ToolTier,
    pub enabled: bool,
    pub available: bool,
}

impl ToolCeilingRow {
    pub const COLUMNS: &str = "tier, enabled, available";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            tier: row.get::<_, EnumColumn<ToolTier>>("tier")?.0,
            enabled: row.get("enabled")?,
            available: row.get("available")?,
        })
    }
}
