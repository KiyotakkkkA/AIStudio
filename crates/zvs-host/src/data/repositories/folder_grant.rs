use rusqlite::{Connection, params};
use zvs_contract::{FolderGrantScope, Timestamp};

use crate::data::schema::columns::EnumColumn;
use crate::data::schema::{FolderGrantRow, GRANTED_BY_USER};

pub struct FolderGrantRepository<'c> {
    connection: &'c Connection,
}

impl<'c> FolderGrantRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self) -> rusqlite::Result<Vec<FolderGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM access_grant WHERE resource_kind = 'folder' \
                 ORDER BY resource COLLATE NOCASE, granted_at, id",
                FolderGrantRow::COLUMNS
            ))?
            .query_map([], FolderGrantRow::from_row)?
            .collect()
    }

    pub fn applicable(&self, conversation_id: &str) -> rusqlite::Result<Vec<FolderGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM access_grant WHERE resource_kind = 'folder' \
                 AND (scope = 'always' OR owner = ?1) ORDER BY granted_at, id",
                FolderGrantRow::COLUMNS
            ))?
            .query_map([conversation_id], FolderGrantRow::from_row)?
            .collect()
    }

    pub fn create(&self, row: &FolderGrantRow) -> rusqlite::Result<FolderGrantRow> {
        self.create_by(row, GRANTED_BY_USER)
    }

    pub fn create_by(
        &self,
        row: &FolderGrantRow,
        granted_by: &str,
    ) -> rusqlite::Result<FolderGrantRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO access_grant (id, resource_kind, resource, action, decision, scope, \
                 owner, granted_by, granted_at, used_at) \
                 VALUES (?1, 'folder', ?2, '', 'allow', ?3, ?4, ?7, ?5, ?6) RETURNING {}",
                FolderGrantRow::COLUMNS
            ))?
            .query_row(
                params![
                    row.id,
                    row.path,
                    EnumColumn::<FolderGrantScope>(row.scope),
                    row.conversation_id,
                    row.granted_at,
                    row.last_used_at,
                    granted_by,
                ],
                FolderGrantRow::from_row,
            )
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("DELETE FROM access_grant WHERE id = ?1 AND resource_kind = 'folder'")?
            .execute([id])?
            > 0)
    }

    pub fn touch(&self, ids: &[String], used_at: Timestamp) -> rusqlite::Result<()> {
        let mut statement = self.connection.prepare_cached(
            "UPDATE access_grant SET used_at = ?2 WHERE id = ?1 AND resource_kind = 'folder'",
        )?;
        for id in ids {
            statement.execute(params![id, used_at])?;
        }
        Ok(())
    }
}
