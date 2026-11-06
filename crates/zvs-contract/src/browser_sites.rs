use serde::{Deserialize, Serialize};

use crate::browser::URL_MAX;
use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, nested};

pub const SITE_INPUT_MAX: usize = 2048;
pub const BROWSER_SITES_MAX: usize = 1000;

wire_enum!(BrowserSiteAction {
    Browse => "browse",
    Click => "click",
    Type => "type",
    Submit => "submit",
    Download => "download",
});

wire_enum!(BrowserSiteDecision {
    Ask => "ask",
    Allow => "allow",
    Block => "block",
});

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSiteDefaultsDto {
    pub browse: BrowserSiteDecision,
    pub click: BrowserSiteDecision,
    #[serde(rename = "type")]
    pub type_text: BrowserSiteDecision,
    pub submit: BrowserSiteDecision,
    pub download: BrowserSiteDecision,
}

impl Default for BrowserSiteDefaultsDto {
    fn default() -> Self {
        Self {
            browse: BrowserSiteDecision::Allow,
            click: BrowserSiteDecision::Ask,
            type_text: BrowserSiteDecision::Ask,
            submit: BrowserSiteDecision::Ask,
            download: BrowserSiteDecision::Ask,
        }
    }
}

impl BrowserSiteDefaultsDto {
    pub const fn get(&self, action: BrowserSiteAction) -> BrowserSiteDecision {
        match action {
            BrowserSiteAction::Browse => self.browse,
            BrowserSiteAction::Click => self.click,
            BrowserSiteAction::Type => self.type_text,
            BrowserSiteAction::Submit => self.submit,
            BrowserSiteAction::Download => self.download,
        }
    }

    pub fn set(&mut self, action: BrowserSiteAction, decision: BrowserSiteDecision) {
        match action {
            BrowserSiteAction::Browse => self.browse = decision,
            BrowserSiteAction::Click => self.click = decision,
            BrowserSiteAction::Type => self.type_text = decision,
            BrowserSiteAction::Submit => self.submit = decision,
            BrowserSiteAction::Download => self.download = decision,
        }
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSiteRulesDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browse: Option<BrowserSiteDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub click: Option<BrowserSiteDecision>,
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_text: Option<BrowserSiteDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submit: Option<BrowserSiteDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download: Option<BrowserSiteDecision>,
}

impl BrowserSiteRulesDto {
    pub const fn get(&self, action: BrowserSiteAction) -> Option<BrowserSiteDecision> {
        match action {
            BrowserSiteAction::Browse => self.browse,
            BrowserSiteAction::Click => self.click,
            BrowserSiteAction::Type => self.type_text,
            BrowserSiteAction::Submit => self.submit,
            BrowserSiteAction::Download => self.download,
        }
    }

    pub fn set(&mut self, action: BrowserSiteAction, decision: Option<BrowserSiteDecision>) {
        match action {
            BrowserSiteAction::Browse => self.browse = decision,
            BrowserSiteAction::Click => self.click = decision,
            BrowserSiteAction::Type => self.type_text = decision,
            BrowserSiteAction::Submit => self.submit = decision,
            BrowserSiteAction::Download => self.download = decision,
        }
    }

    pub fn is_empty(&self) -> bool {
        BrowserSiteAction::ALL
            .iter()
            .all(|action| self.get(*action).is_none())
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSiteDto {
    pub origin: String,
    pub rules: BrowserSiteRulesDto,
    pub updated_at: Timestamp,
}

impl Validate for BrowserSiteDto {
    fn validate(&self) -> Result<(), AppError> {
        check::url("origin", &self.origin, URL_MAX)?;
        check::timestamp("updatedAt", self.updated_at)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSitesDto {
    pub defaults: BrowserSiteDefaultsDto,
    pub sites: Vec<BrowserSiteDto>,
}

impl Validate for BrowserSitesDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.sites.len() > BROWSER_SITES_MAX {
            return Err(invalid("sites", "max"));
        }
        nested("sites", self.sites.validate())
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSiteDefaultInput {
    pub action: BrowserSiteAction,
    pub decision: BrowserSiteDecision,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSitePutInput {
    pub site: String,
    pub rules: BrowserSiteRulesDto,
}

impl Validate for BrowserSitePutInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("site", &self.site)?;
        check::max_len("site", &self.site, SITE_INPUT_MAX)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSiteRef {
    pub origin: String,
}

impl Validate for BrowserSiteRef {
    fn validate(&self) -> Result<(), AppError> {
        check::url("origin", &self.origin, URL_MAX)
    }
}

crate::validate::no_refinements!(
    BrowserSiteDefaultsDto,
    BrowserSiteRulesDto,
    BrowserSiteDefaultInput
);
