use rusqlite::{Connection, OptionalExtension, params};
use zvs_contract::Timestamp;

use crate::data::schema::{AccessGrantRow, GrantDecision, GrantKind};

pub struct AccessGrantRepository<'c> {
    connection: &'c Connection,
}

impl<'c> AccessGrantRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self, kind: Option<GrantKind>) -> rusqlite::Result<Vec<AccessGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM access_grant WHERE ?1 IS NULL OR resource_kind = ?1 \
                 ORDER BY resource_kind, resource COLLATE NOCASE, action, granted_at, id",
                AccessGrantRow::COLUMNS
            ))?
            .query_map([kind], AccessGrantRow::from_row)?
            .collect()
    }

    pub fn applicable(
        &self,
        kind: GrantKind,
        owner: &str,
    ) -> rusqlite::Result<Vec<AccessGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM access_grant WHERE resource_kind = ?1 \
                 AND (owner IS NULL OR owner = ?2) ORDER BY granted_at, id",
                AccessGrantRow::COLUMNS
            ))?
            .query_map(params![kind, owner], AccessGrantRow::from_row)?
            .collect()
    }

    pub fn find(
        &self,
        kind: GrantKind,
        resource: &str,
        action: &str,
        owner: Option<&str>,
    ) -> rusqlite::Result<Option<AccessGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM access_grant WHERE resource_kind = ?1 AND resource = ?2 \
                 AND action = ?3 AND owner IS ?4",
                AccessGrantRow::COLUMNS
            ))?
            .query_row(
                params![kind, resource, action, owner],
                AccessGrantRow::from_row,
            )
            .optional()
    }

    pub fn get(&self, id: &str) -> rusqlite::Result<Option<AccessGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM access_grant WHERE id = ?1",
                AccessGrantRow::COLUMNS
            ))?
            .query_row([id], AccessGrantRow::from_row)
            .optional()
    }

    pub fn insert(&self, row: &AccessGrantRow) -> rusqlite::Result<AccessGrantRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO access_grant ({columns}) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) RETURNING {columns}",
                columns = AccessGrantRow::COLUMNS
            ))?
            .query_row(
                params![
                    row.id,
                    row.resource_kind,
                    row.resource,
                    row.action,
                    row.decision,
                    row.scope,
                    row.owner,
                    row.granted_by,
                    row.granted_at,
                    row.used_at,
                    row.detail,
                ],
                AccessGrantRow::from_row,
            )
    }

    pub fn replace(&self, row: &AccessGrantRow) -> rusqlite::Result<AccessGrantRow> {
        if let Some(existing) = self.find(
            row.resource_kind,
            &row.resource,
            &row.action,
            row.owner.as_deref(),
        )? {
            self.remove(&existing.id)?;
        }
        self.insert(row)
    }

    pub fn decide(
        &self,
        id: &str,
        decision: GrantDecision,
        granted_by: &str,
        granted_at: Timestamp,
        detail: Option<&str>,
    ) -> rusqlite::Result<Option<AccessGrantRow>> {
        self.connection
            .prepare_cached(&format!(
                "UPDATE access_grant SET decision = ?2, granted_by = ?3, granted_at = ?4, \
                 detail = coalesce(?5, detail) WHERE id = ?1 RETURNING {}",
                AccessGrantRow::COLUMNS
            ))?
            .query_row(
                params![id, decision, granted_by, granted_at, detail],
                AccessGrantRow::from_row,
            )
            .optional()
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("DELETE FROM access_grant WHERE id = ?1")?
            .execute([id])?
            > 0)
    }

    pub fn remove_resource(
        &self,
        kind: GrantKind,
        resource: &str,
        action: Option<&str>,
    ) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached(
                "DELETE FROM access_grant WHERE resource_kind = ?1 AND resource = ?2 \
                 AND (?3 IS NULL OR action = ?3)",
            )?
            .execute(params![kind, resource, action])
    }

    pub fn touch(&self, ids: &[String], used_at: Timestamp) -> rusqlite::Result<()> {
        let mut statement = self
            .connection
            .prepare_cached("UPDATE access_grant SET used_at = ?2 WHERE id = ?1")?;
        for id in ids {
            statement.execute(params![id, used_at])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::repositories::Repositories;
    use crate::data::schema::GrantScope;
    use crate::testing::temporary_database;

    fn site(id: &str, action: &str, decision: GrantDecision) -> AccessGrantRow {
        AccessGrantRow {
            id: id.to_owned(),
            resource_kind: GrantKind::Site,
            resource: "https://shop.example".to_owned(),
            action: action.to_owned(),
            decision,
            scope: GrantScope::Always,
            owner: None,
            granted_by: "user".to_owned(),
            granted_at: 1,
            used_at: None,
            detail: None,
        }
    }

    #[test]
    fn one_grant_per_resource_action_and_owner() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let grants = Repositories::new(connection).access_grants;
                grants.insert(&site("g1", "click", GrantDecision::Allow))?;
                assert!(
                    grants
                        .insert(&site("g2", "click", GrantDecision::Block))
                        .is_err()
                );
                grants.replace(&site("g3", "click", GrantDecision::Block))?;
                grants.insert(&site("g4", "type", GrantDecision::Ask))?;
                let rows = grants.list(Some(GrantKind::Site))?;
                assert_eq!(
                    rows.iter()
                        .filter(|row| row.resource == "https://shop.example")
                        .map(|row| (row.id.as_str(), row.decision))
                        .collect::<Vec<_>>(),
                    [("g3", GrantDecision::Block), ("g4", GrantDecision::Ask)]
                );
                assert_eq!(
                    grants.remove_resource(GrantKind::Site, "https://shop.example", None)?,
                    2
                );
                assert!(grants.list(Some(GrantKind::Folder))?.is_empty());
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn a_conversation_grant_needs_its_owner() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let grants = Repositories::new(connection).access_grants;
                let mut row = site("g1", "click", GrantDecision::Allow);
                row.scope = GrantScope::Conversation;
                assert!(grants.insert(&row).is_err());
                row.owner = Some("missing".to_owned());
                assert!(grants.insert(&row).is_err());
                Ok(())
            })
            .expect("the repository works");
    }
}
