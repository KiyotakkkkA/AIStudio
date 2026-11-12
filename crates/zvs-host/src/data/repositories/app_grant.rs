use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

use crate::data::schema::{AppGrantRow, GRANTED_BY_USER};

pub struct AppGrantRepository<'c> {
    connection: &'c Connection,
}

const FROM: &str = "FROM access_grant g LEFT JOIN conversation c ON c.id = g.owner \
     WHERE g.resource_kind = 'app'";

const SHARED: &str = "NOT EXISTS (SELECT 1 FROM access_grant shared \
     WHERE shared.resource_kind = 'app' AND shared.resource = g.resource \
     AND shared.owner IS NULL)";

fn detail(row: &AppGrantRow) -> String {
    json!({ "path": row.path, "icon": row.icon }).to_string()
}

impl<'c> AppGrantRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self) -> rusqlite::Result<Vec<AppGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} {FROM} ORDER BY g.scope, g.resource, g.granted_at DESC",
                AppGrantRow::COLUMNS
            ))?
            .query_map([], AppGrantRow::from_row)?
            .collect()
    }

    pub fn usable(&self, conversation_id: &str) -> rusqlite::Result<Vec<AppGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} {FROM} AND (g.owner IS NULL OR (g.owner = ?1 AND {SHARED}))",
                AppGrantRow::COLUMNS
            ))?
            .query_map([conversation_id], AppGrantRow::from_row)?
            .collect()
    }

    pub fn find(
        &self,
        app: &str,
        conversation_id: Option<&str>,
    ) -> rusqlite::Result<Option<AppGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} {FROM} AND g.resource = ?1 AND g.owner IS ?2",
                AppGrantRow::COLUMNS
            ))?
            .query_row(params![app, conversation_id], AppGrantRow::from_row)
            .optional()
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<AppGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} {FROM} AND g.id = ?1",
                AppGrantRow::COLUMNS
            ))?
            .query_row([id], AppGrantRow::from_row)
            .optional()
    }

    pub fn save(&self, row: &AppGrantRow) -> rusqlite::Result<()> {
        if let Some(existing) = self.find(&row.app, row.conversation_id.as_deref())? {
            let icon = row.icon.clone().or(existing.icon);
            self.connection
                .prepare_cached(
                    "UPDATE access_grant SET detail = ?2, granted_at = ?3 WHERE id = ?1",
                )?
                .execute(params![
                    existing.id,
                    detail(&AppGrantRow {
                        icon,
                        ..row.clone()
                    }),
                    row.granted_at
                ])?;
            return Ok(());
        }
        self.connection
            .prepare_cached(
                "INSERT INTO access_grant (id, resource_kind, resource, action, decision, scope, \
                 owner, granted_by, granted_at, used_at, detail) \
                 VALUES (?1, 'app', ?2, '', 'allow', ?3, ?4, ?5, ?6, ?7, ?8)",
            )?
            .execute(params![
                row.id,
                row.app,
                row.scope,
                row.conversation_id,
                GRANTED_BY_USER,
                row.granted_at,
                row.last_used_at,
                detail(row),
            ])?;
        Ok(())
    }

    pub fn touch(&self, app: &str, conversation_id: &str, now: i64) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached(
                "UPDATE access_grant SET used_at = ?3 WHERE resource_kind = 'app' \
                 AND resource = ?1 AND (owner IS NULL OR (owner = ?2 AND NOT EXISTS \
                 (SELECT 1 FROM access_grant shared WHERE shared.resource_kind = 'app' \
                 AND shared.resource = ?1 AND shared.owner IS NULL)))",
            )?
            .execute(params![app, conversation_id, now])
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("DELETE FROM access_grant WHERE id = ?1 AND resource_kind = 'app'")?
            .execute([id])?
            > 0)
    }
}
