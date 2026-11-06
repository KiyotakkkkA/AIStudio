use rusqlite::{Connection, OptionalExtension, params};

use crate::data::schema::BrowserSiteRuleRow;

pub struct BrowserSiteRuleRepository<'c> {
    connection: &'c Connection,
}

impl<'c> BrowserSiteRuleRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self) -> rusqlite::Result<Vec<BrowserSiteRuleRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM browser_site_rule ORDER BY origin, action",
                BrowserSiteRuleRow::COLUMNS
            ))?
            .query_map([], BrowserSiteRuleRow::from_row)?
            .collect()
    }

    pub fn find(&self, origin: &str, action: &str) -> rusqlite::Result<Option<BrowserSiteRuleRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM browser_site_rule WHERE origin = ?1 AND action = ?2",
                BrowserSiteRuleRow::COLUMNS
            ))?
            .query_row([origin, action], BrowserSiteRuleRow::from_row)
            .optional()
    }

    pub fn save(&self, row: &BrowserSiteRuleRow) -> rusqlite::Result<BrowserSiteRuleRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO browser_site_rule ({columns}) VALUES (?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT (origin, action) DO UPDATE SET decision = excluded.decision, \
                 updated_at = excluded.updated_at \
                 RETURNING {columns}",
                columns = BrowserSiteRuleRow::COLUMNS
            ))?
            .query_row(
                params![row.id, row.origin, row.action, row.decision, row.updated_at],
                BrowserSiteRuleRow::from_row,
            )
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("DELETE FROM browser_site_rule WHERE id = ?1")?
            .execute([id])?
            > 0)
    }

    pub fn remove_rule(&self, origin: &str, action: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("DELETE FROM browser_site_rule WHERE origin = ?1 AND action = ?2")?
            .execute([origin, action])?
            > 0)
    }

    pub fn remove_origin(&self, origin: &str) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached("DELETE FROM browser_site_rule WHERE origin = ?1")?
            .execute([origin])
    }
}
