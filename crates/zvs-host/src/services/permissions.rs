use std::sync::Arc;

use zvs_contract::{AppError, ToolTier, is_permission_scope, validate::invalid};

use crate::data::schema::{PermissionUseRow, ToolPermissionRow};
use crate::data::{Database, Repositories};
use crate::kernel::{NodeRegistry, PermissionRequirement};
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;

pub const GLOBAL_SCOPE: &str = "global";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PermissionContext {
    pub run_id: String,
    pub scopes: Vec<String>,
}

impl PermissionContext {
    pub fn new(run_id: impl Into<String>, scopes: &[&str]) -> Self {
        Self {
            run_id: run_id.into(),
            scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
        }
    }
}

const fn rank(tier: ToolTier) -> u8 {
    match tier {
        ToolTier::Auto => 0,
        ToolTier::Ask => 1,
        ToolTier::Off => 2,
    }
}

fn id_text() -> String {
    new_id().hyphenated().to_string()
}

pub struct PermissionService {
    database: Arc<Database>,
    registry: Arc<NodeRegistry>,
    clock: SharedClock,
}

impl PermissionService {
    pub fn new(database: Arc<Database>, registry: Arc<NodeRegistry>, clock: SharedClock) -> Self {
        Self {
            database,
            registry,
            clock,
        }
    }

    pub fn grant(
        &self,
        subject: &str,
        scope: &str,
        tier: ToolTier,
        granted_by: &str,
    ) -> Result<String, AppError> {
        self.database.with_connection(|connection| {
            self.grant_in(
                &Repositories::new(connection),
                subject,
                scope,
                tier,
                granted_by,
            )
        })
    }

    pub(crate) fn grant_in(
        &self,
        repositories: &Repositories<'_>,
        subject: &str,
        scope: &str,
        tier: ToolTier,
        granted_by: &str,
    ) -> Result<String, crate::data::DataError> {
        if subject.is_empty() {
            return Err(invalid("subject", "nonEmpty").into());
        }
        if scope != GLOBAL_SCOPE && !is_permission_scope(scope) {
            return Err(invalid("scope", "pattern").into());
        }
        if granted_by.is_empty() {
            return Err(invalid("grantedBy", "nonEmpty").into());
        }
        let id = id_text();
        repositories.permissions.grant(&ToolPermissionRow {
            id: id.clone(),
            subject: subject.to_owned(),
            scope: scope.to_owned(),
            tier,
            granted_by: granted_by.to_owned(),
            granted_at: self.clock.now(),
        })?;
        Ok(id)
    }

    pub fn revoke(&self, id: &str) -> Result<(), AppError> {
        self.database.with_connection(|connection| {
            Repositories::new(connection).permissions.revoke(id)?;
            Ok(())
        })
    }

    pub fn admit(
        &self,
        node_type: &str,
        context: &PermissionContext,
    ) -> Result<Admission, AppError> {
        match self.registry.resolve(node_type)?.permission() {
            PermissionRequirement::None => Ok(Admission::Allow),
            PermissionRequirement::Tool { tool, tier } => {
                self.admit_subject(tool, context, tier.unwrap_or(ToolTier::Auto))
            }
            PermissionRequirement::AppFolder { tool } => self.admit_app_folder(tool, context),
        }
    }

    pub fn admit_app_folder(
        &self,
        subject: &str,
        context: &PermissionContext,
    ) -> Result<Admission, AppError> {
        self.database.with_connection(|connection| {
            let repositories = Repositories::new(connection);
            let closed = repositories
                .permissions
                .tool_ceiling(subject)?
                .is_some_and(|tool| !tool.enabled || !tool.available || tool.tier == ToolTier::Off);
            let refused = repositories.permissions.list(subject)?.iter().any(|row| {
                row.tier == ToolTier::Off
                    && (row.scope == GLOBAL_SCOPE || context.scopes.contains(&row.scope))
            });
            Ok(if closed || refused {
                Admission::Deny
            } else {
                Admission::Allow
            })
        })
    }

    pub fn admit_subject(
        &self,
        subject: &str,
        context: &PermissionContext,
        minimum: ToolTier,
    ) -> Result<Admission, AppError> {
        self.database.with_connection(|connection| {
            let repositories = Repositories::new(connection);
            let registered = repositories.permissions.tool_ceiling(subject)?;
            if registered.is_some_and(|tool| !tool.enabled || !tool.available) {
                return Ok(Admission::Deny);
            }
            let rows = repositories.permissions.list(subject)?;
            let mut tier = rows
                .iter()
                .find(|row| row.scope == GLOBAL_SCOPE)
                .map_or(ToolTier::Ask, |row| row.tier);
            if let Some(tool) = registered
                && rank(tool.tier) > rank(tier)
            {
                tier = tool.tier;
            }
            if rank(minimum) > rank(tier) {
                tier = minimum;
            }
            let applicable: Vec<&ToolPermissionRow> = rows
                .iter()
                .filter(|row| row.scope == GLOBAL_SCOPE || context.scopes.contains(&row.scope))
                .collect();
            for row in &applicable {
                if rank(row.tier) > rank(tier) {
                    tier = row.tier;
                }
            }
            if tier == ToolTier::Auto {
                let used_at = self.clock.now();
                for row in &applicable {
                    repositories.permissions.log(&PermissionUseRow {
                        id: id_text(),
                        grant_id: row.id.clone(),
                        run_id: context.run_id.clone(),
                        used_at,
                        subject: row.subject.clone(),
                        scope: row.scope.clone(),
                        tier: row.tier,
                        granted_by: row.granted_by.clone(),
                        granted_at: row.granted_at,
                    })?;
                }
            }
            Ok(match tier {
                ToolTier::Auto => Admission::Allow,
                ToolTier::Ask => Admission::Ask,
                ToolTier::Off => Admission::Deny,
            })
        })
    }
}
