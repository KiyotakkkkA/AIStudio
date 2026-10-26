use std::sync::Arc;

use zvs_contract::{AppError, AppErrorCode, Timestamp, ToolTier};

use super::capability::{BrowserCapability, resolve_tier, site_origin};
use crate::data::schema::BrowserGrantRow;
use crate::data::{Database, Repositories};
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;

pub const NOT_GRANTABLE: &str = "Это действие подтверждается каждый раз и не выдаётся сайту";
pub const NOT_A_SITE: &str = "Разрешение выдаётся только сайту с адресом HTTP или HTTPS";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserGrant {
    pub id: String,
    pub origin: String,
    pub capability: BrowserCapability,
    pub granted_at: Timestamp,
}

fn grant_of(row: BrowserGrantRow) -> Option<BrowserGrant> {
    Some(BrowserGrant {
        capability: BrowserCapability::from_key(&row.capability)?,
        id: row.id,
        origin: row.origin,
        granted_at: row.granted_at,
    })
}

#[derive(Clone)]
pub struct BrowserGrants {
    database: Arc<Database>,
    clock: SharedClock,
}

impl BrowserGrants {
    pub fn new(database: Arc<Database>, clock: SharedClock) -> Self {
        Self { database, clock }
    }

    pub fn list(&self) -> Result<Vec<BrowserGrant>, AppError> {
        self.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .browser_grants
                .list()?
                .into_iter()
                .filter_map(grant_of)
                .collect())
        })
    }

    pub fn grant(
        &self,
        origin: &str,
        capability: BrowserCapability,
    ) -> Result<BrowserGrant, AppError> {
        if !capability.grantable() {
            return Err(AppError::new(AppErrorCode::PermissionDenied, NOT_GRANTABLE));
        }
        let origin = site_origin(origin)
            .filter(|canonical| canonical == origin)
            .ok_or_else(|| AppError::new(AppErrorCode::ValidationFailed, NOT_A_SITE))?;
        let row = BrowserGrantRow {
            id: new_id().hyphenated().to_string(),
            origin,
            capability: capability.key().to_owned(),
            granted_at: self.clock.now(),
        };
        self.database.with_connection(|connection| {
            let stored = Repositories::new(connection).browser_grants.grant(&row)?;
            Ok(grant_of(stored).expect("a stored grant has a known capability"))
        })
    }

    pub fn revoke(&self, id: &str) -> Result<bool, AppError> {
        self.database.with_connection(|connection| {
            Ok(Repositories::new(connection).browser_grants.remove(id)?)
        })
    }

    pub fn revoke_origin(&self, origin: &str) -> Result<usize, AppError> {
        self.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .browser_grants
                .remove_origin(origin)?)
        })
    }

    pub fn find(
        &self,
        origin: &str,
        capability: BrowserCapability,
    ) -> Result<Option<BrowserGrant>, AppError> {
        self.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .browser_grants
                .find(origin, capability.key())?
                .and_then(grant_of))
        })
    }

    pub fn tier(&self, capability: BrowserCapability, origin: Option<&str>) -> ToolTier {
        resolve_tier(capability, origin, |origin, capability| {
            self.find(origin, capability).ok().flatten().is_some()
        })
    }
}
