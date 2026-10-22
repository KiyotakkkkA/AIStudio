use rusqlite::{Connection, params};
use zvs_contract::Timestamp;

use crate::data::schema::FolderGrantRow;
use crate::data::schema::columns::EnumColumn;

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
                "SELECT {} FROM folder_grant ORDER BY path COLLATE NOCASE, granted_at, id",
                FolderGrantRow::COLUMNS
            ))?
            .query_map([], FolderGrantRow::from_row)?
            .collect()
    }

    pub fn applicable(&self, conversation_id: &str) -> rusqlite::Result<Vec<FolderGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM folder_grant WHERE scope = 'always' OR conversation_id = ?1 \
                 ORDER BY granted_at, id",
                FolderGrantRow::COLUMNS
            ))?
            .query_map([conversation_id], FolderGrantRow::from_row)?
            .collect()
    }

    pub fn create(&self, row: &FolderGrantRow) -> rusqlite::Result<FolderGrantRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO folder_grant ({columns}) VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 RETURNING {columns}",
                columns = FolderGrantRow::COLUMNS
            ))?
            .query_row(
                params![
                    row.id,
                    row.path,
                    EnumColumn(row.scope),
                    row.conversation_id,
                    row.granted_at,
                    row.last_used_at,
                ],
                FolderGrantRow::from_row,
            )
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("DELETE FROM folder_grant WHERE id = ?1")?
            .execute([id])?
            > 0)
    }

    pub fn touch(&self, ids: &[String], used_at: Timestamp) -> rusqlite::Result<()> {
        let mut statement = self
            .connection
            .prepare_cached("UPDATE folder_grant SET last_used_at = ?2 WHERE id = ?1")?;
        for id in ids {
            statement.execute(params![id, used_at])?;
        }
        Ok(())
    }
}
