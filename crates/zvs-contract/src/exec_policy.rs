use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::validate::{Validate, check};

pub const EXEC_JUDGE_ID_MAX: usize = 200;
pub const TOOL_AUTHORISATION_SOURCE_MAX: usize = 200;
pub const TOOL_AUTHORISATION_REASON_MAX: usize = 2000;

wire_enum!(ExecMode {
    AlwaysAsk => "always-ask",
    SafetyAsk => "safety-ask",
    AutoBypass => "auto-bypass",
    NoRestrictions => "no-restrictions",
});

wire_enum!(ToolAuthorisationKind {
    Auto => "auto",
    Granted => "granted",
    Approved => "approved",
    Policy => "policy",
    Judge => "judge",
});

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecJudgeDto {
    pub provider_id: String,
    pub model_id: String,
}

impl Validate for ExecJudgeDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("providerId", &self.provider_id)?;
        check::max_len("providerId", &self.provider_id, EXEC_JUDGE_ID_MAX)?;
        check::non_empty("modelId", &self.model_id)?;
        check::max_len("modelId", &self.model_id, EXEC_JUDGE_ID_MAX)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecPolicyDto {
    pub mode: ExecMode,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub judge: Option<ExecJudgeDto>,
}

impl Default for ExecPolicyDto {
    fn default() -> Self {
        Self {
            mode: ExecMode::AlwaysAsk,
            judge: None,
        }
    }
}

impl Validate for ExecPolicyDto {
    fn validate(&self) -> Result<(), AppError> {
        self.judge.as_ref().map_or(Ok(()), Validate::validate)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAuthorisationDto {
    pub kind: ToolAuthorisationKind,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub source: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub reason: Option<String>,
}

impl ToolAuthorisationDto {
    pub const fn of(kind: ToolAuthorisationKind) -> Self {
        Self {
            kind,
            source: None,
            reason: None,
        }
    }

    #[must_use]
    pub fn from_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    #[must_use]
    pub fn because(mut self, reason: Option<String>) -> Self {
        self.reason = reason;
        self
    }

    pub fn label(&self) -> String {
        self.source.as_ref().map_or_else(
            || self.kind.as_str().to_owned(),
            |source| format!("{}:{source}", self.kind.as_str()),
        )
    }
}

impl Validate for ToolAuthorisationDto {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(source) = &self.source {
            check::non_empty("source", source)?;
            check::max_len("source", source, TOOL_AUTHORISATION_SOURCE_MAX)?;
        }
        self.reason.as_ref().map_or(Ok(()), |reason| {
            check::max_len("reason", reason, TOOL_AUTHORISATION_REASON_MAX)
        })
    }
}
