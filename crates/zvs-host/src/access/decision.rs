use serde::Serialize;
use zvs_contract::{AppError, AppErrorCode, ToolAuthorisationDto, ToolAuthorisationKind};

use super::stages::{
    FLOOR_DISABLED, FLOOR_UNLISTED, OUTSIDE_ALLOWED_TOOLS, PERMISSION_DENIED, TIER_GRANTED,
    TOOL_DISABLED,
};
use crate::policy::{ReviewVerdict, rule_denied};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StageName {
    Floor,
    Tier,
    Mode,
    Rules,
    Judge,
    Person,
}

impl StageName {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Floor => "floor",
            Self::Tier => "tier",
            Self::Mode => "mode",
            Self::Rules => "rules",
            Self::Judge => "judge",
            Self::Person => "person",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub outcome: Outcome,
    pub stage: StageName,
    pub source: Option<String>,
    pub reason: Option<String>,
    pub trail: Vec<StageName>,
}

impl Decision {
    pub const fn new(outcome: Outcome, stage: StageName) -> Self {
        Self {
            outcome,
            stage,
            source: None,
            reason: None,
            trail: Vec::new(),
        }
    }

    pub const fn allow(stage: StageName) -> Self {
        Self::new(Outcome::Allow, stage)
    }

    pub const fn ask(stage: StageName) -> Self {
        Self::new(Outcome::Ask, stage)
    }

    pub const fn deny(stage: StageName) -> Self {
        Self::new(Outcome::Deny, stage)
    }

    pub fn from_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn because(mut self, reason: Option<String>) -> Self {
        self.reason = reason;
        self
    }

    pub(crate) fn with_trail(mut self, trail: Vec<StageName>) -> Self {
        self.trail = trail;
        self
    }

    pub fn authorisation(&self) -> Option<ToolAuthorisationDto> {
        if self.outcome != Outcome::Allow {
            return None;
        }
        let sourced = |kind| {
            let authorisation = ToolAuthorisationDto::of(kind);
            match &self.source {
                Some(source) => authorisation.from_source(source.clone()),
                None => authorisation,
            }
        };
        match self.stage {
            StageName::Floor => None,
            StageName::Tier if self.source.as_deref() == Some(TIER_GRANTED) => {
                Some(ToolAuthorisationDto::of(ToolAuthorisationKind::Granted))
            }
            StageName::Tier => Some(ToolAuthorisationDto::of(ToolAuthorisationKind::Auto)),
            StageName::Mode => Some(sourced(ToolAuthorisationKind::Policy)),
            StageName::Rules | StageName::Judge => {
                Some(sourced(ToolAuthorisationKind::Judge).because(self.reason.clone()))
            }
            StageName::Person => Some(
                ToolAuthorisationDto::of(ToolAuthorisationKind::Approved)
                    .because(self.reason.clone()),
            ),
        }
    }

    pub fn verdict(&self) -> ReviewVerdict {
        match self.outcome {
            Outcome::Allow => ReviewVerdict::Run(
                self.authorisation()
                    .unwrap_or_else(|| ToolAuthorisationDto::of(ToolAuthorisationKind::Auto)),
            ),
            Outcome::Ask => ReviewVerdict::Ask {
                reason: self.reason.clone(),
            },
            Outcome::Deny => ReviewVerdict::Deny {
                rule: self.source.clone().unwrap_or_default(),
            },
        }
    }

    pub fn refusal(&self, tool: &str) -> AppError {
        match (self.stage, self.source.as_deref()) {
            (StageName::Floor, Some(FLOOR_UNLISTED)) => {
                AppError::new(AppErrorCode::PermissionDenied, OUTSIDE_ALLOWED_TOOLS)
                    .with_detail("key", tool)
            }
            (StageName::Floor, Some(FLOOR_DISABLED)) => {
                AppError::new(AppErrorCode::PermissionDenied, TOOL_DISABLED)
            }
            (StageName::Rules, Some(rule)) => rule_denied(rule),
            _ => AppError::new(AppErrorCode::PermissionDenied, PERMISSION_DENIED),
        }
    }
}
