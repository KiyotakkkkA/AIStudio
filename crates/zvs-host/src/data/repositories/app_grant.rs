use rusqlite::{Connection, OptionalExtension, params};

use crate::data::schema::AppGrantRow;

pub struct AppGrantRepository<'c> {
    connection: &'c Connection,
}

const FROM: &str = "FROM app_grant g LEFT JOIN conversation c ON c.id = g.conversation_id";

impl<'c> AppGrantRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self) -> rusqlite::Result<Vec<AppGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} {FROM} ORDER BY g.scope, g.app, g.granted_at DESC",
                AppGrantRow::COLUMNS
            ))?
            .query_map([], AppGrantRow::from_row)?
            .collect()
    }

    pub fn usable(&self, conversation_id: &str) -> rusqlite::Result<Vec<AppGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} {FROM} WHERE g.conversation_id IS NULL OR g.conversation_id = ?1",
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
                "SELECT {} {FROM} WHERE g.app = ?1 AND g.conversation_id IS ?2",
                AppGrantRow::COLUMNS
            ))?
            .query_row(params![app, conversation_id], AppGrantRow::from_row)
            .optional()
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<AppGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} {FROM} WHERE g.id = ?1",
                AppGrantRow::COLUMNS
            ))?
            .query_row([id], AppGrantRow::from_row)
            .optional()
    }

    pub fn save(&self, row: &AppGrantRow) -> rusqlite::Result<()> {
        if let Some(existing) = self.find(&row.app, row.conversation_id.as_deref())? {
            self.connection
                .prepare_cached(
                    "UPDATE app_grant SET path = ?2, icon = coalesce(?3, icon), granted_at = ?4 \
                     WHERE id = ?1",
                )?
                .execute(params![existing.id, row.path, row.icon, row.granted_at])?;
            return Ok(());
        }
        self.connection
            .prepare_cached(
                "INSERT INTO app_grant (id, app, path, icon, scope, conversation_id, granted_at, \
                 last_used_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?
            .execute(params![
                row.id,
                row.app,
                row.path,
                row.icon,
                row.scope,
                row.conversation_id,
                row.granted_at,
                row.last_used_at
            ])?;
        Ok(())
    }

    pub fn touch(&self, app: &str, conversation_id: &str, now: i64) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached(
                "UPDATE app_grant SET last_used_at = ?3 WHERE app = ?1 \
                 AND (conversation_id IS NULL OR conversation_id = ?2)",
            )?
            .execute(params![app, conversation_id, now])
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("DELETE FROM app_grant WHERE id = ?1")?
            .execute([id])?
            > 0)
    }
}
