use rusqlite::{Connection, OptionalExtension, params};

use crate::data::schema::BrowserGrantRow;

pub struct BrowserGrantRepository<'c> {
    connection: &'c Connection,
}

impl<'c> BrowserGrantRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self) -> rusqlite::Result<Vec<BrowserGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM browser_grant ORDER BY origin, capability",
                BrowserGrantRow::COLUMNS
            ))?
            .query_map([], BrowserGrantRow::from_row)?
            .collect()
    }

    pub fn find(
        &self,
        origin: &str,
        capability: &str,
    ) -> rusqlite::Result<Option<BrowserGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM browser_grant WHERE origin = ?1 AND capability = ?2",
                BrowserGrantRow::COLUMNS
            ))?
            .query_row([origin, capability], BrowserGrantRow::from_row)
            .optional()
    }

    pub fn grant(&self, row: &BrowserGrantRow) -> rusqlite::Result<BrowserGrantRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO browser_grant ({columns}) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT (origin, capability) DO UPDATE SET granted_at = excluded.granted_at \
                 RETURNING {columns}",
                columns = BrowserGrantRow::COLUMNS
            ))?
            .query_row(
                params![row.id, row.origin, row.capability, row.granted_at],
                BrowserGrantRow::from_row,
            )
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("DELETE FROM browser_grant WHERE id = ?1")?
            .execute([id])?
            > 0)
    }

    pub fn remove_origin(&self, origin: &str) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached("DELETE FROM browser_grant WHERE origin = ?1")?
            .execute([origin])
    }
}
