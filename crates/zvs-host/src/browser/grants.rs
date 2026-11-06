use std::collections::BTreeMap;
use std::sync::Arc;

use zvs_contract::{
    AppError, AppErrorCode, BrowserSiteAction, BrowserSiteDecision, BrowserSiteDefaultsDto,
    BrowserSiteDto, BrowserSiteRulesDto, BrowserSitesDto, Timestamp, ToolTier,
};

use super::capability::{BrowserCapability, site_origin};
use super::policy::navigation_url;
use crate::data::schema::BrowserSiteRuleRow;
use crate::data::{Database, Repositories};
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;

pub const NOT_GRANTABLE: &str =
    "Для этого действия требуется отдельное подтверждение при каждом вызове";
pub const NOT_A_SITE: &str = "Разрешение выдаётся только сайту с адресом HTTP или HTTPS";
pub const SITE_BLOCKED: &str = "Это действие запрещено для сайта в параметрах браузера";
pub const DEFAULT_ORIGIN: &str = "*";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserGrant {
    pub id: String,
    pub origin: String,
    pub capability: BrowserCapability,
    pub granted_at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SiteDecision {
    pub decision: BrowserSiteDecision,
    pub site: bool,
    pub updated_at: Option<Timestamp>,
}

pub const fn site_action(capability: BrowserCapability) -> BrowserSiteAction {
    match capability {
        BrowserCapability::Navigate
        | BrowserCapability::ReadPage
        | BrowserCapability::Screenshot
        | BrowserCapability::Find => BrowserSiteAction::Browse,
        BrowserCapability::Click => BrowserSiteAction::Click,
        BrowserCapability::Type => BrowserSiteAction::Type,
        BrowserCapability::Submit => BrowserSiteAction::Submit,
        BrowserCapability::Download => BrowserSiteAction::Download,
    }
}

const fn capability_of(action: BrowserSiteAction) -> Option<BrowserCapability> {
    match action {
        BrowserSiteAction::Click => Some(BrowserCapability::Click),
        BrowserSiteAction::Type => Some(BrowserCapability::Type),
        _ => None,
    }
}

pub const fn tier_of(decision: BrowserSiteDecision) -> ToolTier {
    match decision {
        BrowserSiteDecision::Allow | BrowserSiteDecision::Block => ToolTier::Auto,
        BrowserSiteDecision::Ask => ToolTier::Ask,
    }
}

pub fn canonical_site(input: &str) -> Result<String, AppError> {
    navigation_url(input)
        .ok()
        .and_then(|url| site_origin(&url))
        .ok_or_else(|| AppError::new(AppErrorCode::ValidationFailed, NOT_A_SITE))
}

fn exact_origin(origin: &str) -> Result<String, AppError> {
    site_origin(origin)
        .filter(|canonical| canonical == origin)
        .ok_or_else(|| AppError::new(AppErrorCode::ValidationFailed, NOT_A_SITE))
}

fn parsed(row: &BrowserSiteRuleRow) -> Option<(BrowserSiteAction, BrowserSiteDecision)> {
    Some((
        BrowserSiteAction::parse(&row.action)?,
        BrowserSiteDecision::parse(&row.decision)?,
    ))
}

fn grant_of(row: &BrowserSiteRuleRow) -> Option<BrowserGrant> {
    let (action, decision) = parsed(row)?;
    if row.origin == DEFAULT_ORIGIN || decision != BrowserSiteDecision::Allow {
        return None;
    }
    Some(BrowserGrant {
        id: row.id.clone(),
        origin: row.origin.clone(),
        capability: capability_of(action)?,
        granted_at: row.updated_at,
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

    fn rows(&self) -> Result<Vec<BrowserSiteRuleRow>, AppError> {
        self.database.with_connection(|connection| {
            Ok(Repositories::new(connection).browser_site_rules.list()?)
        })
    }

    fn save(
        &self,
        origin: &str,
        action: BrowserSiteAction,
        decision: BrowserSiteDecision,
    ) -> Result<BrowserSiteRuleRow, AppError> {
        let row = BrowserSiteRuleRow {
            id: new_id().hyphenated().to_string(),
            origin: origin.to_owned(),
            action: action.as_str().to_owned(),
            decision: decision.as_str().to_owned(),
            updated_at: self.clock.now(),
        };
        self.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .browser_site_rules
                .save(&row)?)
        })
    }

    pub fn list(&self) -> Result<Vec<BrowserGrant>, AppError> {
        Ok(self.rows()?.iter().filter_map(grant_of).collect())
    }

    pub fn grant(
        &self,
        origin: &str,
        capability: BrowserCapability,
    ) -> Result<BrowserGrant, AppError> {
        if !capability.grantable() {
            return Err(AppError::new(AppErrorCode::PermissionDenied, NOT_GRANTABLE));
        }
        let origin = exact_origin(origin)?;
        let row = self.save(&origin, site_action(capability), BrowserSiteDecision::Allow)?;
        Ok(grant_of(&row).expect("an allow rule for click or type is a grant"))
    }

    pub fn revoke(&self, id: &str) -> Result<bool, AppError> {
        self.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .browser_site_rules
                .remove(id)?)
        })
    }

    pub fn revoke_origin(&self, origin: &str) -> Result<usize, AppError> {
        self.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .browser_site_rules
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
                .browser_site_rules
                .find(origin, site_action(capability).as_str())?
                .as_ref()
                .and_then(grant_of))
        })
    }

    pub fn decision(&self, origin: Option<&str>, capability: BrowserCapability) -> SiteDecision {
        let action = site_action(capability);
        let rules = self.database.with_connection(|connection| {
            let repository = Repositories::new(connection).browser_site_rules;
            let site = match origin {
                Some(origin) => repository.find(origin, action.as_str())?,
                None => None,
            };
            let fallback = repository.find(DEFAULT_ORIGIN, action.as_str())?;
            Ok((site, fallback))
        });
        let (site, fallback) = rules.unwrap_or((None, None));
        if let Some((row, decision)) = site
            .as_ref()
            .and_then(|row| parsed(row).map(|(_, decision)| (row, decision)))
        {
            return SiteDecision {
                decision,
                site: true,
                updated_at: Some(row.updated_at),
            };
        }
        SiteDecision {
            decision: fallback.as_ref().and_then(parsed).map_or_else(
                || BrowserSiteDefaultsDto::default().get(action),
                |(_, decision)| decision,
            ),
            site: false,
            updated_at: None,
        }
    }

    pub fn tier(&self, capability: BrowserCapability, origin: Option<&str>) -> ToolTier {
        tier_of(self.decision(origin, capability).decision)
    }

    pub fn sites(&self) -> Result<BrowserSitesDto, AppError> {
        let mut defaults = BrowserSiteDefaultsDto::default();
        let mut sites: BTreeMap<String, (BrowserSiteRulesDto, Timestamp)> = BTreeMap::new();
        for row in self.rows()? {
            let Some((action, decision)) = parsed(&row) else {
                continue;
            };
            if row.origin == DEFAULT_ORIGIN {
                defaults.set(action, decision);
                continue;
            }
            let entry = sites
                .entry(row.origin.clone())
                .or_insert((BrowserSiteRulesDto::default(), row.updated_at));
            entry.0.set(action, Some(decision));
            entry.1 = entry.1.max(row.updated_at);
        }
        Ok(BrowserSitesDto {
            defaults,
            sites: sites
                .into_iter()
                .map(|(origin, (rules, updated_at))| BrowserSiteDto {
                    origin,
                    rules,
                    updated_at,
                })
                .collect(),
        })
    }

    pub fn set_default(
        &self,
        action: BrowserSiteAction,
        decision: BrowserSiteDecision,
    ) -> Result<BrowserSitesDto, AppError> {
        self.save(DEFAULT_ORIGIN, action, decision)?;
        self.sites()
    }

    pub fn put(
        &self,
        site: &str,
        rules: &BrowserSiteRulesDto,
    ) -> Result<BrowserSitesDto, AppError> {
        let origin = canonical_site(site)?;
        let now = self.clock.now();
        self.database.transaction(|transaction| {
            let repository = Repositories::new(transaction).browser_site_rules;
            for action in BrowserSiteAction::ALL {
                match rules.get(*action) {
                    Some(decision) => {
                        repository.save(&BrowserSiteRuleRow {
                            id: new_id().hyphenated().to_string(),
                            origin: origin.clone(),
                            action: action.as_str().to_owned(),
                            decision: decision.as_str().to_owned(),
                            updated_at: now,
                        })?;
                    }
                    None => {
                        repository.remove_rule(&origin, action.as_str())?;
                    }
                }
            }
            Ok(())
        })?;
        self.sites()
    }

    pub fn remove_site(&self, origin: &str) -> Result<BrowserSitesDto, AppError> {
        self.revoke_origin(origin)?;
        self.sites()
    }
}
