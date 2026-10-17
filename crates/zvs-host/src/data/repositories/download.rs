use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use zvs_contract::{DownloadStatus, Timestamp};

use crate::data::schema::DownloadRow;
use crate::data::schema::columns::EnumColumn;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadPatch {
    pub status: Option<DownloadStatus>,
    pub bytes_done: Option<u64>,
    pub size_bytes: Option<u64>,
    pub priority: Option<u64>,
    pub error: Option<Option<String>>,
    pub run_id: Option<Option<String>>,
    pub updated_at: Option<Timestamp>,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Option<Timestamp>>,
}

const ACTIVE: &str = "'queued', 'running', 'paused'";

fn signed(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn integer(value: u64) -> SqlValue {
    SqlValue::Integer(i64::try_from(value).unwrap_or(i64::MAX))
}

pub struct DownloadRepository<'c> {
    connection: &'c Connection,
}

impl<'c> DownloadRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    fn select(&self, clause: &str, values: Vec<SqlValue>) -> rusqlite::Result<Vec<DownloadRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM download {clause}",
                DownloadRow::COLUMNS
            ))?
            .query_map(params_from_iter(values), DownloadRow::from_row)?
            .collect()
    }

    pub fn list(&self) -> rusqlite::Result<Vec<DownloadRow>> {
        self.select("ORDER BY created_at DESC, rowid DESC", Vec::new())
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<DownloadRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM download WHERE id = ?1",
                DownloadRow::COLUMNS
            ))?
            .query_row([id], DownloadRow::from_row)
            .optional()
    }

    pub fn find_active_by_ref(&self, item_ref: &str) -> rusqlite::Result<Option<DownloadRow>> {
        Ok(self
            .select(
                &format!("WHERE item_ref = ?1 AND status IN ({ACTIVE}) LIMIT 1"),
                vec![SqlValue::Text(item_ref.to_owned())],
            )?
            .into_iter()
            .next())
    }

    pub fn find_latest_by_ref(&self, item_ref: &str) -> rusqlite::Result<Option<DownloadRow>> {
        Ok(self
            .select(
                "WHERE item_ref = ?1 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                vec![SqlValue::Text(item_ref.to_owned())],
            )?
            .into_iter()
            .next())
    }

    pub fn by_status(&self, statuses: &[DownloadStatus]) -> rusqlite::Result<Vec<DownloadRow>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; statuses.len()].join(", ");
        self.select(
            &format!("WHERE status IN ({placeholders}) ORDER BY created_at DESC, rowid DESC"),
            statuses
                .iter()
                .map(|status| SqlValue::Text(status.as_str().to_owned()))
                .collect(),
        )
    }

    pub fn next_queued(&self, limit: usize) -> rusqlite::Result<Vec<DownloadRow>> {
        self.select(
            "WHERE status = 'queued' ORDER BY priority DESC, size_bytes ASC, created_at ASC, rowid ASC LIMIT ?1",
            vec![integer(limit as u64)],
        )
    }

    pub fn running(&self) -> rusqlite::Result<Vec<DownloadRow>> {
        self.select("WHERE status = 'running' ORDER BY rowid", Vec::new())
    }

    pub fn installed(&self) -> rusqlite::Result<Vec<DownloadRow>> {
        self.select("WHERE status = 'succeeded' ORDER BY rowid", Vec::new())
    }

    pub fn create(&self, draft: &DownloadRow) -> rusqlite::Result<DownloadRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO download ({columns}) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19) \
                 RETURNING {columns}",
                columns = DownloadRow::COLUMNS
            ))?
            .query_row(
                params![
                    draft.id,
                    EnumColumn(draft.item_kind),
                    draft.item_ref,
                    draft.display_name,
                    draft.version,
                    signed(draft.size_bytes),
                    signed(draft.bytes_done),
                    EnumColumn(draft.status),
                    signed(draft.priority),
                    draft.target_path,
                    draft.url,
                    draft.checksum_algorithm.map(EnumColumn),
                    draft.checksum_value,
                    draft.error,
                    draft.run_id,
                    draft.created_at,
                    draft.updated_at,
                    draft.started_at,
                    draft.finished_at,
                ],
                DownloadRow::from_row,
            )
    }

    pub fn update(&self, id: &str, patch: &DownloadPatch) -> rusqlite::Result<Option<DownloadRow>> {
        let mut assignments: Vec<&str> = Vec::new();
        let mut values: Vec<SqlValue> = Vec::new();
        if let Some(status) = patch.status {
            assignments.push("status = ?");
            values.push(SqlValue::Text(status.as_str().to_owned()));
        }
        if let Some(bytes) = patch.bytes_done {
            assignments.push("bytes_done = ?");
            values.push(integer(bytes));
        }
        if let Some(bytes) = patch.size_bytes {
            assignments.push("size_bytes = ?");
            values.push(integer(bytes));
        }
        if let Some(priority) = patch.priority {
            assignments.push("priority = ?");
            values.push(integer(priority));
        }
        if let Some(error) = &patch.error {
            assignments.push("error = ?");
            values.push(error.clone().map_or(SqlValue::Null, SqlValue::Text));
        }
        if let Some(run_id) = &patch.run_id {
            assignments.push("run_id = ?");
            values.push(run_id.clone().map_or(SqlValue::Null, SqlValue::Text));
        }
        if let Some(at) = patch.updated_at {
            assignments.push("updated_at = ?");
            values.push(SqlValue::Integer(at));
        }
        if let Some(at) = patch.started_at {
            assignments.push("started_at = ?");
            values.push(SqlValue::Integer(at));
        }
        if let Some(at) = patch.finished_at {
            assignments.push("finished_at = ?");
            values.push(at.map_or(SqlValue::Null, SqlValue::Integer));
        }
        if assignments.is_empty() {
            return self.find_by_id(id);
        }
        values.push(SqlValue::Text(id.to_owned()));
        self.connection
            .prepare_cached(&format!(
                "UPDATE download SET {} WHERE id = ? RETURNING {}",
                assignments.join(", "),
                DownloadRow::COLUMNS
            ))?
            .query_row(params_from_iter(values), DownloadRow::from_row)
            .optional()
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached("DELETE FROM download WHERE id = ?1")?
            .execute([id])?;
        Ok(())
    }

    pub fn interrupt(&self, at: Timestamp) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached(
                "UPDATE download SET status = 'paused', run_id = NULL, updated_at = ?1 \
                 WHERE status = 'running'",
            )?
            .execute([at])
    }

    pub fn remove_terminal(&self) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached(&format!(
                "DELETE FROM download WHERE status NOT IN ({ACTIVE})"
            ))?
            .execute([])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Repositories;
    use crate::testing::temporary_database;

    fn row(id: &str, item_ref: &str, created_at: Timestamp) -> DownloadRow {
        DownloadRow {
            id: id.to_owned(),
            item_kind: zvs_contract::DownloadItemKind::Model,
            item_ref: item_ref.to_owned(),
            display_name: item_ref.to_owned(),
            version: None,
            size_bytes: 100,
            bytes_done: 0,
            status: DownloadStatus::Queued,
            priority: 5,
            target_path: format!("C:/downloads/{id}.bin"),
            url: format!("https://example.test/{id}"),
            checksum_algorithm: None,
            checksum_value: None,
            error: None,
            run_id: None,
            created_at,
            updated_at: created_at,
            started_at: None,
            finished_at: None,
        }
    }

    #[test]
    fn the_queue_orders_by_priority_then_size_then_age() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let downloads = Repositories::new(connection).downloads;
                downloads.create(&DownloadRow {
                    size_bytes: 240,
                    ..row("big", "curated:model:big", 1)
                })?;
                downloads.create(&DownloadRow {
                    size_bytes: 10,
                    ..row("small", "curated:model:small", 3)
                })?;
                downloads.create(&DownloadRow {
                    priority: 7,
                    size_bytes: 1000,
                    ..row("mcp", "curated:mcp:a", 4)
                })?;
                downloads.create(&DownloadRow {
                    size_bytes: 10,
                    ..row("older", "curated:model:older", 2)
                })?;
                downloads.create(&DownloadRow {
                    status: DownloadStatus::Running,
                    priority: 9,
                    ..row("busy", "curated:model:busy", 0)
                })?;
                let order: Vec<String> = downloads
                    .next_queued(10)?
                    .into_iter()
                    .map(|row| row.id)
                    .collect();
                assert_eq!(order, ["mcp", "older", "small", "big"]);
                assert_eq!(downloads.next_queued(1)?.len(), 1);
                assert_eq!(downloads.running()?[0].id, "busy");
                let listed: Vec<String> = downloads.list()?.into_iter().map(|row| row.id).collect();
                assert_eq!(listed, ["mcp", "small", "older", "big", "busy"]);
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn a_patch_sets_and_clears_nullable_columns_and_returns_the_row() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let downloads = Repositories::new(connection).downloads;
                let created = downloads.create(&DownloadRow {
                    checksum_algorithm: Some(zvs_contract::ChecksumAlgorithm::Sha256),
                    checksum_value: Some("ab".repeat(32)),
                    version: Some("F16".to_owned()),
                    ..row("a", "curated:model:a", 1)
                })?;
                assert_eq!(
                    created.checksum_algorithm,
                    Some(zvs_contract::ChecksumAlgorithm::Sha256)
                );
                let failed = downloads
                    .update(
                        "a",
                        &DownloadPatch {
                            status: Some(DownloadStatus::Failed),
                            error: Some(Some("boom".to_owned())),
                            run_id: Some(Some("run".to_owned())),
                            finished_at: Some(Some(9)),
                            bytes_done: Some(40),
                            ..DownloadPatch::default()
                        },
                    )?
                    .expect("the row");
                assert_eq!(failed.status, DownloadStatus::Failed);
                assert_eq!(failed.error.as_deref(), Some("boom"));
                assert_eq!(failed.finished_at, Some(9));
                assert_eq!(failed.bytes_done, 40);
                let cleared = downloads
                    .update(
                        "a",
                        &DownloadPatch {
                            status: Some(DownloadStatus::Queued),
                            error: Some(None),
                            run_id: Some(None),
                            finished_at: Some(None),
                            ..DownloadPatch::default()
                        },
                    )?
                    .expect("the row");
                assert_eq!(cleared.error, None);
                assert_eq!(cleared.run_id, None);
                assert_eq!(cleared.finished_at, None);
                assert_eq!(
                    downloads.update("missing", &DownloadPatch::default())?,
                    None
                );
                assert_eq!(
                    downloads.update(
                        "missing",
                        &DownloadPatch {
                            priority: Some(1),
                            ..DownloadPatch::default()
                        }
                    )?,
                    None
                );
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn an_item_has_one_active_download_and_finished_rows_can_be_swept() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let downloads = Repositories::new(connection).downloads;
                downloads.create(&DownloadRow {
                    status: DownloadStatus::Succeeded,
                    ..row("done", "curated:model:a", 1)
                })?;
                assert_eq!(downloads.find_active_by_ref("curated:model:a")?, None);
                downloads.create(&DownloadRow {
                    status: DownloadStatus::Paused,
                    ..row("paused", "curated:model:a", 2)
                })?;
                assert_eq!(
                    downloads
                        .find_active_by_ref("curated:model:a")?
                        .map(|row| row.id),
                    Some("paused".to_owned())
                );
                assert_eq!(
                    downloads
                        .find_latest_by_ref("curated:model:a")?
                        .map(|row| row.id),
                    Some("paused".to_owned())
                );
                downloads.create(&DownloadRow {
                    status: DownloadStatus::Running,
                    run_id: Some("run".to_owned()),
                    ..row("running", "curated:model:b", 3)
                })?;
                assert_eq!(downloads.installed()?.len(), 1);
                assert_eq!(
                    downloads
                        .by_status(&[DownloadStatus::Paused, DownloadStatus::Running])?
                        .len(),
                    2
                );
                assert_eq!(downloads.interrupt(10)?, 1);
                let interrupted = downloads.find_by_id("running")?.expect("the row");
                assert_eq!(interrupted.status, DownloadStatus::Paused);
                assert_eq!(interrupted.run_id, None);
                assert_eq!(downloads.remove_terminal()?, 1);
                downloads.remove("paused")?;
                assert_eq!(downloads.list()?.len(), 1);
                Ok(())
            })
            .expect("the repository works");
    }
}
