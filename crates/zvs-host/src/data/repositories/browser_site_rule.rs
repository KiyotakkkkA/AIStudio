use rusqlite::{Connection, OptionalExtension, params};

use crate::data::schema::{BrowserSiteRuleRow, GRANTED_BY_USER};

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
                "SELECT {} FROM access_grant WHERE resource_kind = 'site' ORDER BY resource, action",
                BrowserSiteRuleRow::COLUMNS
            ))?
            .query_map([], BrowserSiteRuleRow::from_row)?
            .collect()
    }

    pub fn find(&self, origin: &str, action: &str) -> rusqlite::Result<Option<BrowserSiteRuleRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM access_grant WHERE resource_kind = 'site' AND resource = ?1 \
                 AND action = ?2 AND owner IS NULL",
                BrowserSiteRuleRow::COLUMNS
            ))?
            .query_row([origin, action], BrowserSiteRuleRow::from_row)
            .optional()
    }

    pub fn save(&self, row: &BrowserSiteRuleRow) -> rusqlite::Result<BrowserSiteRuleRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO access_grant (id, resource_kind, resource, action, decision, scope, \
                 owner, granted_by, granted_at) \
                 VALUES (?1, 'site', ?2, ?3, ?4, 'always', NULL, ?6, ?5) \
                 ON CONFLICT (resource_kind, resource, action) WHERE owner IS NULL \
                 DO UPDATE SET decision = excluded.decision, granted_at = excluded.granted_at \
                 RETURNING {}",
                BrowserSiteRuleRow::COLUMNS
            ))?
            .query_row(
                params![
                    row.id,
                    row.origin,
                    row.action,
                    row.decision,
                    row.updated_at,
                    GRANTED_BY_USER
                ],
                BrowserSiteRuleRow::from_row,
            )
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("DELETE FROM access_grant WHERE id = ?1 AND resource_kind = 'site'")?
            .execute([id])?
            > 0)
    }

    pub fn remove_rule(&self, origin: &str, action: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached(
                "DELETE FROM access_grant WHERE resource_kind = 'site' AND resource = ?1 \
                 AND action = ?2",
            )?
            .execute([origin, action])?
            > 0)
    }

    pub fn remove_origin(&self, origin: &str) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached(
                "DELETE FROM access_grant WHERE resource_kind = 'site' AND resource = ?1",
            )?
            .execute([origin])
    }
}
