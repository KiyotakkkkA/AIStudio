use rusqlite::{Connection, OptionalExtension, params};

use crate::data::schema::{BrowserBookmarkRow, BrowserHistoryRow};

fn count(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

pub struct BrowserHistoryRepository<'c> {
    connection: &'c Connection,
}

impl<'c> BrowserHistoryRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn newest(&self, limit: usize) -> rusqlite::Result<Vec<BrowserHistoryRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM browser_history ORDER BY visited_at DESC, id LIMIT ?1",
                BrowserHistoryRow::COLUMNS
            ))?
            .query_map([count(limit)], BrowserHistoryRow::from_row)?
            .collect()
    }

    pub fn total(&self) -> rusqlite::Result<u64> {
        self.connection
            .prepare_cached("SELECT count(*) FROM browser_history")?
            .query_row([], |row| row.get::<_, i64>(0))
            .map(|total| u64::try_from(total).unwrap_or(0))
    }

    pub fn latest_for(&self, url: &str) -> rusqlite::Result<Option<BrowserHistoryRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM browser_history WHERE url = ?1 AND imported_from IS NULL \
                 ORDER BY visited_at DESC LIMIT 1",
                BrowserHistoryRow::COLUMNS
            ))?
            .query_row([url], BrowserHistoryRow::from_row)
            .optional()
    }

    pub fn insert(&self, row: &BrowserHistoryRow) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached(&format!(
                "INSERT OR IGNORE INTO browser_history ({}) VALUES (?1, ?2, ?3, ?4, ?5)",
                BrowserHistoryRow::COLUMNS
            ))?
            .execute(params![
                row.id,
                row.url,
                row.title,
                row.visited_at,
                row.imported_from
            ])?
            > 0)
    }

    pub fn touch(&self, id: &str, title: &str, visited_at: i64) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached(
                "UPDATE OR IGNORE browser_history SET title = ?2, visited_at = ?3 WHERE id = ?1",
            )?
            .execute(params![id, title, visited_at])?
            > 0)
    }

    pub fn keep_newest(&self, limit: usize) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached(
                "DELETE FROM browser_history WHERE id NOT IN \
                 (SELECT id FROM browser_history ORDER BY visited_at DESC, id LIMIT ?1)",
            )?
            .execute([count(limit)])
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("DELETE FROM browser_history WHERE id = ?1")?
            .execute([id])?
            > 0)
    }

    pub fn clear(&self) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached("DELETE FROM browser_history")?
            .execute([])
    }
}

pub struct BrowserBookmarkRepository<'c> {
    connection: &'c Connection,
}

impl<'c> BrowserBookmarkRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self, limit: usize) -> rusqlite::Result<Vec<BrowserBookmarkRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM browser_bookmark ORDER BY folder, created_at, id LIMIT ?1",
                BrowserBookmarkRow::COLUMNS
            ))?
            .query_map([count(limit)], BrowserBookmarkRow::from_row)?
            .collect()
    }

    pub fn recent(&self, limit: usize) -> rusqlite::Result<Vec<BrowserBookmarkRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM browser_bookmark ORDER BY created_at DESC, id LIMIT ?1",
                BrowserBookmarkRow::COLUMNS
            ))?
            .query_map([count(limit)], BrowserBookmarkRow::from_row)?
            .collect()
    }

    pub fn find_url(&self, url: &str) -> rusqlite::Result<Option<BrowserBookmarkRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM browser_bookmark WHERE url = ?1",
                BrowserBookmarkRow::COLUMNS
            ))?
            .query_row([url], BrowserBookmarkRow::from_row)
            .optional()
    }

    pub fn insert(&self, row: &BrowserBookmarkRow) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached(&format!(
                "INSERT OR IGNORE INTO browser_bookmark ({}) VALUES (?1, ?2, ?3, ?4, ?5)",
                BrowserBookmarkRow::COLUMNS
            ))?
            .execute(params![
                row.id,
                row.url,
                row.title,
                row.folder,
                row.created_at
            ])?
            > 0)
    }

    pub fn rename(&self, id: &str, title: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("UPDATE browser_bookmark SET title = ?2 WHERE id = ?1")?
            .execute([id, title])?
            > 0)
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("DELETE FROM browser_bookmark WHERE id = ?1")?
            .execute([id])?
            > 0)
    }

    pub fn count(&self) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached("SELECT count(*) FROM browser_bookmark")?
            .query_row([], |row| row.get::<_, i64>(0))
            .map(|total| usize::try_from(total).unwrap_or(0))
    }
}
