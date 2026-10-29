use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::files::FileChangeKind;
use crate::folders::check_folder_path;
use crate::process::{PROCESS_ARG_MAX, PROCESS_ARGV_MAX, PROCESS_PROGRAM_MAX};
use crate::validate::{Validate, check, invalid, nested};

pub const EXEC_JUDGE_ID_MAX: usize = 200;
pub const TOOL_AUTHORISATION_SOURCE_MAX: usize = 200;
pub const TOOL_AUTHORISATION_REASON_MAX: usize = 2000;
pub const ALLOW_RULE_CHANGES_MAX: usize = 64;
pub const POLICY_RULE_ID_MAX: usize = 200;
pub const POLICY_RULE_TEXT_MAX: usize = 2000;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowRuleChangeDto {
    pub path: String,
    pub kind: FileChangeKind,
}

impl Validate for AllowRuleChangeDto {
    fn validate(&self) -> Result<(), AppError> {
        check_folder_path("path", &self.path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AllowRuleInput {
    Files {
        changes: Vec<AllowRuleChangeDto>,
        #[serde(deserialize_with = "crate::wire::nullable")]
        folder: Option<String>,
    },
    Process {
        argv: Vec<String>,
        cwd: String,
        #[serde(deserialize_with = "crate::wire::nullable")]
        folder: Option<String>,
    },
}

impl AllowRuleInput {
    pub fn folder(&self) -> Option<&str> {
        match self {
            Self::Files { folder, .. } | Self::Process { folder, .. } => folder.as_deref(),
        }
    }
}

impl Validate for AllowRuleInput {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(folder) = self.folder() {
            check_folder_path("folder", folder)?;
        }
        match self {
            Self::Files { changes, .. } => {
                if changes.is_empty() {
                    return Err(invalid("changes", "minItems"));
                }
                if changes.len() > ALLOW_RULE_CHANGES_MAX {
                    return Err(invalid("changes", "maxItems"));
                }
                changes.iter().enumerate().try_for_each(|(index, change)| {
                    nested(&format!("changes.{index}"), change.validate())
                })
            }
            Self::Process { argv, cwd, .. } => {
                let Some(program) = argv.first() else {
                    return Err(invalid("argv", "minItems"));
                };
                check::non_empty("argv", program)?;
                check::max_len("argv", program, PROCESS_PROGRAM_MAX)?;
                if argv.len() > PROCESS_ARGV_MAX {
                    return Err(invalid("argv", "maxItems"));
                }
                argv.iter()
                    .try_for_each(|arg| check::max_len("argv", arg, PROCESS_ARG_MAX))?;
                check_folder_path("cwd", cwd)
            }
        }
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRuleDto {
    pub id: String,
    pub text: String,
}

impl Validate for PolicyRuleDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("id", &self.id)?;
        check::max_len("id", &self.id, POLICY_RULE_ID_MAX)?;
        check::non_empty("text", &self.text)?;
        check::max_len("text", &self.text, POLICY_RULE_TEXT_MAX)
    }
}
