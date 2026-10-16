use rusqlite::{Connection, OptionalExtension, params, params_from_iter};

use crate::data::schema::columns::EnumColumn;
use crate::data::schema::{
    ApprovalDecision, PendingApprovalRow, PermissionUseRow, ToolCeilingRow, ToolPermissionRow,
};

pub struct PermissionRepository<'c> {
    connection: &'c Connection,
}

impl<'c> PermissionRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self, subject: &str) -> rusqlite::Result<Vec<ToolPermissionRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM tool_permission WHERE subject = ?1",
                ToolPermissionRow::COLUMNS
            ))?
            .query_map([subject], ToolPermissionRow::from_row)?
            .collect()
    }

    pub fn grant(&self, row: &ToolPermissionRow) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached(
                "INSERT INTO tool_permission (id, subject, scope, tier, granted_at, granted_by) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT (subject, scope) DO UPDATE SET id = excluded.id, \
                 subject = excluded.subject, scope = excluded.scope, tier = excluded.tier, \
                 granted_at = excluded.granted_at, granted_by = excluded.granted_by",
            )?
            .execute(params![
                row.id,
                row.subject,
                row.scope,
                EnumColumn(row.tier),
                row.granted_at,
                row.granted_by,
            ])?;
        Ok(())
    }

    pub fn revoke(&self, id: &str) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached("DELETE FROM tool_permission WHERE id = ?1")?
            .execute([id])
    }

    pub fn log(&self, row: &PermissionUseRow) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached(
                "INSERT INTO permission_use (id, grant_id, run_id, used_at, subject, scope, tier, \
                 granted_by, granted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?
            .execute(params![
                row.id,
                row.grant_id,
                row.run_id,
                row.used_at,
                row.subject,
                row.scope,
                EnumColumn(row.tier),
                row.granted_by,
                row.granted_at,
            ])?;
        Ok(())
    }

    pub fn uses(&self, grant_id: &str) -> rusqlite::Result<Vec<PermissionUseRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM permission_use WHERE grant_id = ?1",
                PermissionUseRow::COLUMNS
            ))?
            .query_map([grant_id], PermissionUseRow::from_row)?
            .collect()
    }

    pub fn create_approval(&self, row: &PendingApprovalRow) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached(
                "INSERT INTO pending_approval (id, run_id, subject, scope, expires_at, decision) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?
            .execute(params![
                row.id,
                row.run_id,
                row.subject,
                row.scope,
                row.expires_at,
                row.decision.map(ApprovalDecision::as_str),
            ])?;
        Ok(())
    }

    pub fn approval(&self, id: &str) -> rusqlite::Result<Option<PendingApprovalRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM pending_approval WHERE id = ?1",
                PendingApprovalRow::COLUMNS
            ))?
            .query_row([id], PendingApprovalRow::from_row)
            .optional()
    }

    pub fn decide(&self, id: &str, decision: ApprovalDecision) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached("UPDATE pending_approval SET decision = ?2 WHERE id = ?1")?
            .execute(params![id, decision.as_str()])
    }

    pub fn approvals_for_run(&self, run_id: &str) -> rusqlite::Result<Vec<PendingApprovalRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM pending_approval WHERE run_id = ?1",
                PendingApprovalRow::COLUMNS
            ))?
            .query_map([run_id], PendingApprovalRow::from_row)?
            .collect()
    }

    pub fn pending_for_runs(
        &self,
        run_ids: &[String],
    ) -> rusqlite::Result<Vec<PendingApprovalRow>> {
        if run_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.connection
            .prepare(&format!(
                "SELECT {} FROM pending_approval WHERE run_id IN ({}) AND decision IS NULL \
                 ORDER BY expires_at",
                PendingApprovalRow::COLUMNS,
                vec!["?"; run_ids.len()].join(", ")
            ))?
            .query_map(params_from_iter(run_ids), PendingApprovalRow::from_row)?
            .collect()
    }

    pub fn deny_unfinished(&self, run_id: &str) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached(
                "UPDATE pending_approval SET decision = 'denied' \
                 WHERE run_id = ?1 AND decision IS NULL",
            )?
            .execute([run_id])
    }

    pub fn tool_ceiling(&self, key: &str) -> rusqlite::Result<Option<ToolCeilingRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM tool WHERE key = ?1",
                ToolCeilingRow::COLUMNS
            ))?
            .query_row([key], ToolCeilingRow::from_row)
            .optional()
    }
}

#[cfg(test)]
mod tests {
    use zvs_contract::ToolTier;

    use super::*;
    use crate::data::repositories::Repositories;
    use crate::testing::temporary_database;

    fn grant(id: &str, scope: &str, tier: ToolTier) -> ToolPermissionRow {
        ToolPermissionRow {
            id: id.to_owned(),
            subject: "mail.send".to_owned(),
            scope: scope.to_owned(),
            tier,
            granted_at: 1,
            granted_by: "user".to_owned(),
        }
    }

    #[test]
    fn a_grant_replaces_the_previous_one_at_the_same_scope() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let permissions = Repositories::new(connection).permissions;
                permissions.grant(&grant("g1", "global", ToolTier::Ask))?;
                permissions.grant(&grant("g2", "global", ToolTier::Auto))?;
                permissions.grant(&grant("g3", "skill:a", ToolTier::Off))?;
                let rows = permissions.list("mail.send")?;
                assert_eq!(rows.len(), 2);
                assert!(rows.contains(&grant("g2", "global", ToolTier::Auto)));
                assert_eq!(permissions.revoke("g3")?, 1);
                assert_eq!(permissions.list("mail.send")?.len(), 1);
                assert_eq!(permissions.tool_ceiling("mail.send")?, None);
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn approvals_are_decided_once_and_listed_pending_first_by_expiry() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let permissions = Repositories::new(connection).permissions;
                for (id, expires_at) in [("a2", 20), ("a1", 10), ("a3", 30)] {
                    permissions.create_approval(&PendingApprovalRow {
                        id: id.to_owned(),
                        run_id: "r1".to_owned(),
                        subject: "mail.send".to_owned(),
                        scope: "global".to_owned(),
                        expires_at,
                        decision: None,
                    })?;
                }
                permissions.decide("a3", ApprovalDecision::Approved)?;
                assert_eq!(
                    permissions
                        .pending_for_runs(&["r1".to_owned()])?
                        .into_iter()
                        .map(|row| row.id)
                        .collect::<Vec<_>>(),
                    ["a1", "a2"]
                );
                assert_eq!(permissions.deny_unfinished("r1")?, 2);
                assert_eq!(
                    permissions.approval("a1")?.and_then(|row| row.decision),
                    Some(ApprovalDecision::Denied)
                );
                assert_eq!(
                    permissions.approval("a3")?.and_then(|row| row.decision),
                    Some(ApprovalDecision::Approved)
                );
                assert_eq!(permissions.approvals_for_run("r1")?.len(), 3);
                Ok(())
            })
            .expect("the repository works");
    }
}
