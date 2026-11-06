use rusqlite::{Connection, OptionalExtension, params};

use crate::data::schema::BrowserDownloadRow;

pub struct BrowserDownloadRepository<'c> {
    connection: &'c Connection,
}

impl<'c> BrowserDownloadRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self, limit: usize) -> rusqlite::Result<Vec<BrowserDownloadRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM browser_download ORDER BY finished_at DESC, id LIMIT ?1",
                BrowserDownloadRow::COLUMNS
            ))?
            .query_map(
                [i64::try_from(limit).unwrap_or(i64::MAX)],
                BrowserDownloadRow::from_row,
            )?
            .collect()
    }

    pub fn find(&self, id: &str) -> rusqlite::Result<Option<BrowserDownloadRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM browser_download WHERE id = ?1",
                BrowserDownloadRow::COLUMNS
            ))?
            .query_row([id], BrowserDownloadRow::from_row)
            .optional()
    }

    pub fn insert(&self, row: &BrowserDownloadRow) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO browser_download ({}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                BrowserDownloadRow::COLUMNS
            ))?
            .execute(params![
                row.id,
                row.url,
                row.file_name,
                row.path,
                row.bytes,
                row.state,
                row.agent,
                row.finished_at
            ])?;
        Ok(())
    }

    pub fn keep_newest(&self, count: usize) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached(
                "DELETE FROM browser_download WHERE id NOT IN \
                 (SELECT id FROM browser_download ORDER BY finished_at DESC, id LIMIT ?1)",
            )?
            .execute([i64::try_from(count).unwrap_or(i64::MAX)])
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("DELETE FROM browser_download WHERE id = ?1")?
            .execute([id])?
            > 0)
    }

    pub fn clear(&self) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached("DELETE FROM browser_download")?
            .execute([])
    }
}
