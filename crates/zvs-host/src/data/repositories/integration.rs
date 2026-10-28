use rusqlite::{Connection, OptionalExtension, params};
use zvs_contract::{IntegrationConfig, Timestamp};

use crate::data::schema::columns::{EnumColumn, json_text};
use crate::data::schema::{IntegrationAccountRow, IntegrationActivityRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrationCounter {
    Events,
    Duplicates,
    Runs,
    ToolCalls,
    Errors,
}

impl IntegrationCounter {
    const fn column(self) -> &'static str {
        match self {
            Self::Events => "event_count",
            Self::Duplicates => "duplicate_count",
            Self::Runs => "run_count",
            Self::ToolCalls => "tool_call_count",
            Self::Errors => "error_count",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntegrationPatch {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub config: Option<IntegrationConfig>,
    pub last_error: Option<Option<String>>,
    pub updated_at: Option<Timestamp>,
}

pub struct IntegrationRepository<'c> {
    connection: &'c Connection,
}

fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

impl<'c> IntegrationRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self) -> rusqlite::Result<Vec<IntegrationAccountRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM integration_account ORDER BY kind, created_at, id",
                IntegrationAccountRow::COLUMNS
            ))?
            .query_map([], IntegrationAccountRow::from_row)?
            .collect()
    }

    pub fn list_enabled(&self) -> rusqlite::Result<Vec<IntegrationAccountRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM integration_account WHERE enabled = 1 ORDER BY kind, created_at, id",
                IntegrationAccountRow::COLUMNS
            ))?
            .query_map([], IntegrationAccountRow::from_row)?
            .collect()
    }

    pub fn find(&self, id: &str) -> rusqlite::Result<Option<IntegrationAccountRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM integration_account WHERE id = ?1",
                IntegrationAccountRow::COLUMNS
            ))?
            .query_row([id], IntegrationAccountRow::from_row)
            .optional()
    }

    pub fn insert(&self, row: &IntegrationAccountRow) -> rusqlite::Result<IntegrationAccountRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO integration_account ({columns}) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14) \
                 RETURNING {columns}",
                columns = IntegrationAccountRow::COLUMNS
            ))?
            .query_row(
                params![
                    row.id,
                    EnumColumn(row.kind),
                    row.name,
                    row.enabled,
                    json_text(&row.config)?,
                    row.last_error,
                    row.last_activity_at,
                    to_i64(row.counters.events),
                    to_i64(row.counters.duplicates),
                    to_i64(row.counters.runs),
                    to_i64(row.counters.tool_calls),
                    to_i64(row.counters.errors),
                    row.created_at,
                    row.updated_at,
                ],
                IntegrationAccountRow::from_row,
            )
    }

    pub fn update(
        &self,
        id: &str,
        patch: &IntegrationPatch,
    ) -> rusqlite::Result<Option<IntegrationAccountRow>> {
        let config = patch.config.as_ref().map(json_text).transpose()?;
        self.connection
            .prepare_cached(&format!(
                "UPDATE integration_account SET \
                 name = COALESCE(?2, name), \
                 enabled = COALESCE(?3, enabled), \
                 config = COALESCE(?4, config), \
                 last_error = CASE WHEN ?5 THEN ?6 ELSE last_error END, \
                 updated_at = COALESCE(?7, updated_at) \
                 WHERE id = ?1 RETURNING {}",
                IntegrationAccountRow::COLUMNS
            ))?
            .query_row(
                params![
                    id,
                    patch.name,
                    patch.enabled,
                    config,
                    patch.last_error.is_some(),
                    patch.last_error.clone().flatten(),
                    patch.updated_at,
                ],
                IntegrationAccountRow::from_row,
            )
            .optional()
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("DELETE FROM integration_account WHERE id = ?1")?
            .execute([id])?
            > 0)
    }

    pub fn record_event(
        &self,
        account_id: &str,
        trigger: &str,
        event_id: &str,
        received_at: Timestamp,
    ) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached(
                "INSERT INTO integration_event (account_id, trigger, event_id, received_at) \
                 VALUES (?1, ?2, ?3, ?4) ON CONFLICT DO NOTHING",
            )?
            .execute(params![account_id, trigger, event_id, received_at])?
            > 0)
    }

    pub fn prune_events(&self, before: Timestamp) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached("DELETE FROM integration_event WHERE received_at < ?1")?
            .execute([before])
    }

    pub fn bump(
        &self,
        id: &str,
        counter: IntegrationCounter,
        by: u64,
        at: Timestamp,
    ) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached(&format!(
                "UPDATE integration_account SET {column} = {column} + ?2, \
                 last_activity_at = ?3 WHERE id = ?1",
                column = counter.column()
            ))?
            .execute(params![id, to_i64(by), at])?;
        Ok(())
    }

    pub fn add_activity(&self, row: &IntegrationActivityRow, keep: usize) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached(
                "INSERT INTO integration_activity (account_id, at, level, message) \
                 VALUES (?1, ?2, ?3, ?4)",
            )?
            .execute(params![
                row.account_id,
                row.at,
                EnumColumn(row.level),
                row.message
            ])?;
        self.connection
            .prepare_cached(
                "DELETE FROM integration_activity WHERE account_id = ?1 AND id NOT IN \
                 (SELECT id FROM integration_activity WHERE account_id = ?1 \
                 ORDER BY id DESC LIMIT ?2)",
            )?
            .execute(params![row.account_id, to_i64(keep as u64)])?;
        Ok(())
    }

    pub fn activity(
        &self,
        account_id: &str,
        limit: u64,
    ) -> rusqlite::Result<Vec<IntegrationActivityRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM integration_activity WHERE account_id = ?1 \
                 ORDER BY id DESC LIMIT ?2",
                IntegrationActivityRow::COLUMNS
            ))?
            .query_map(
                params![account_id, to_i64(limit)],
                IntegrationActivityRow::from_row,
            )?
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use zvs_contract::{IntegrationActivityLevel, IntegrationCountersDto, IntegrationKind};

    use super::*;
    use crate::data::Repositories;
    use crate::testing::temporary_database;

    fn account(id: &str) -> IntegrationAccountRow {
        IntegrationAccountRow {
            id: id.to_owned(),
            kind: IntegrationKind::Telegram,
            name: "Бот".to_owned(),
            enabled: true,
            config: IntegrationConfig::new(),
            last_error: None,
            last_activity_at: None,
            counters: IntegrationCountersDto::default(),
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn an_event_id_is_recorded_once_per_account_and_trigger() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let integrations = Repositories::new(connection).integrations;
                integrations.insert(&account("a"))?;
                integrations.insert(&account("b"))?;
                assert!(integrations.record_event("a", "telegram.message", "1", 10)?);
                assert!(!integrations.record_event("a", "telegram.message", "1", 11)?);
                assert!(integrations.record_event("a", "telegram.edited", "1", 11)?);
                assert!(integrations.record_event("b", "telegram.message", "1", 11)?);
                assert_eq!(integrations.prune_events(11)?, 1);
                assert!(integrations.record_event("a", "telegram.message", "1", 12)?);
                Ok(())
            })
            .expect("records");
    }

    #[test]
    fn counters_patches_and_activity_round_trip() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let integrations = Repositories::new(connection).integrations;
                integrations.insert(&account("a"))?;
                integrations.bump("a", IntegrationCounter::Events, 2, 50)?;
                integrations.bump("a", IntegrationCounter::Runs, 1, 60)?;
                let patched = integrations
                    .update(
                        "a",
                        &IntegrationPatch {
                            name: Some("Новый".to_owned()),
                            last_error: Some(Some("Сбой".to_owned())),
                            ..IntegrationPatch::default()
                        },
                    )?
                    .expect("exists");
                assert_eq!(patched.name, "Новый");
                assert_eq!(patched.counters.events, 2);
                assert_eq!(patched.counters.runs, 1);
                assert_eq!(patched.last_activity_at, Some(60));
                assert_eq!(patched.last_error.as_deref(), Some("Сбой"));
                let cleared = integrations
                    .update(
                        "a",
                        &IntegrationPatch {
                            last_error: Some(None),
                            ..IntegrationPatch::default()
                        },
                    )?
                    .expect("exists");
                assert_eq!(cleared.last_error, None);
                assert_eq!(cleared.name, "Новый");
                for at in 0..5 {
                    integrations.add_activity(
                        &IntegrationActivityRow {
                            account_id: "a".to_owned(),
                            at,
                            level: IntegrationActivityLevel::Info,
                            message: format!("Событие {at}"),
                        },
                        3,
                    )?;
                }
                let recent = integrations.activity("a", 10)?;
                assert_eq!(
                    recent.iter().map(|row| row.at).collect::<Vec<_>>(),
                    [4, 3, 2]
                );
                assert!(integrations.remove("a")?);
                assert!(integrations.activity("a", 10)?.is_empty());
                Ok(())
            })
            .expect("round trips");
    }
}
