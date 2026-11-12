use serde::{Deserialize, Serialize};

use crate::browser_sites::BrowserSitesDto;
use crate::computer::AppGrantDto;
use crate::errors::AppError;
use crate::exec_policy::{ExecPolicyDto, UserRulesDto};
use crate::folders::FolderGrantDto;
use crate::primitives::Timestamp;
use crate::tools::{TOOL_KEY_MAX, ToolTier};
use crate::validate::{Validate, check, nested};

pub const ACCESS_GRANT_ID_MAX: usize = 64;
pub const ACCESS_SCOPE_MAX: usize = 512;
pub const ACCESS_GRANTED_BY_MAX: usize = 32;
pub const TOOL_DISPLAY_NAME_MAX: usize = 256;
pub const ACCESS_LIST_MAX: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolGrantDto {
    pub id: String,
    pub tool: String,
    pub display_name: String,
    pub scope: String,
    pub tier: ToolTier,
    pub granted_by: String,
    pub granted_at: Timestamp,
}

impl Validate for ToolGrantDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("id", &self.id)?;
        check::max_len("id", &self.id, ACCESS_GRANT_ID_MAX)?;
        check::non_empty("tool", &self.tool)?;
        check::max_len("tool", &self.tool, TOOL_KEY_MAX)?;
        check::max_len("displayName", &self.display_name, TOOL_DISPLAY_NAME_MAX)?;
        check::non_empty("scope", &self.scope)?;
        check::max_len("scope", &self.scope, ACCESS_SCOPE_MAX)?;
        check::non_empty("grantedBy", &self.granted_by)?;
        check::max_len("grantedBy", &self.granted_by, ACCESS_GRANTED_BY_MAX)?;
        check::safe_integer("grantedAt", self.granted_at.unsigned_abs())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessListDto {
    pub policy: ExecPolicyDto,
    pub folders: Vec<FolderGrantDto>,
    pub sites: BrowserSitesDto,
    pub apps: Vec<AppGrantDto>,
    pub tools: Vec<ToolGrantDto>,
    pub rules: UserRulesDto,
}

fn each<T: Validate>(field: &str, items: &[T]) -> Result<(), AppError> {
    if items.len() > ACCESS_LIST_MAX {
        return Err(crate::validate::invalid(field, "maxItems"));
    }
    items
        .iter()
        .enumerate()
        .try_for_each(|(index, item)| nested(&format!("{field}.{index}"), item.validate()))
}

impl Validate for AccessListDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("policy", self.policy.validate())?;
        each("folders", &self.folders)?;
        nested("sites", self.sites.validate())?;
        each("apps", &self.apps)?;
        each("tools", &self.tools)?;
        nested("rules", self.rules.validate())
    }
}
