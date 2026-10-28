use rusqlite::Row;
use zvs_contract::{
    IntegrationActivityLevel, IntegrationConfig, IntegrationCountersDto, IntegrationKind, Timestamp,
};

use super::columns::{EnumColumn, json_column};

fn count(row: &Row<'_>, name: &str) -> rusqlite::Result<u64> {
    Ok(u64::try_from(row.get::<_, i64>(name)?).unwrap_or(0))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationAccountRow {
    pub id: String,
    pub kind: IntegrationKind,
    pub name: String,
    pub enabled: bool,
    pub config: IntegrationConfig,
    pub last_error: Option<String>,
    pub last_activity_at: Option<Timestamp>,
    pub counters: IntegrationCountersDto,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl IntegrationAccountRow {
    pub const COLUMNS: &str = "id, kind, name, enabled, config, last_error, last_activity_at, \
        event_count, duplicate_count, run_count, tool_call_count, error_count, created_at, \
        updated_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            kind: row.get::<_, EnumColumn<IntegrationKind>>("kind")?.0,
            name: row.get("name")?,
            enabled: row.get("enabled")?,
            config: json_column(row, "config")?,
            last_error: row.get("last_error")?,
            last_activity_at: row.get("last_activity_at")?,
            counters: IntegrationCountersDto {
                events: count(row, "event_count")?,
                duplicates: count(row, "duplicate_count")?,
                runs: count(row, "run_count")?,
                tool_calls: count(row, "tool_call_count")?,
                errors: count(row, "error_count")?,
            },
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationActivityRow {
    pub account_id: String,
    pub at: Timestamp,
    pub level: IntegrationActivityLevel,
    pub message: String,
}

impl IntegrationActivityRow {
    pub const COLUMNS: &str = "account_id, at, level, message";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            account_id: row.get("account_id")?,
            at: row.get("at")?,
            level: row
                .get::<_, EnumColumn<IntegrationActivityLevel>>("level")?
                .0,
            message: row.get("message")?,
        })
    }
}
