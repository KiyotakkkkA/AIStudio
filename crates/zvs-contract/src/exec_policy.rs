use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::{RunId, is_wire_uuid};
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, nested};

pub const EXEC_JUDGE_ID_MAX: usize = 200;
pub const TOOL_AUTHORISATION_SOURCE_MAX: usize = 200;
pub const TOOL_AUTHORISATION_REASON_MAX: usize = 2000;
pub const POLICY_RULE_ID_MAX: usize = 200;
pub const POLICY_RULE_TEXT_MAX: usize = 2000;
pub const POLICY_PACK_NAME_MAX: usize = 255;
pub const USER_RULES_MAX: usize = 4096;

wire_enum!(ExecMode {
    AlwaysAsk => "always-ask",
    SafetyAsk => "safety-ask",
    AutoBypass => "auto-bypass",
    NoRestrictions => "no-restrictions",
});

wire_enum!(AllowRuleScope {
    Folder => "folder",
    Anywhere => "anywhere",
});

wire_enum!(PolicyRuleEffect {
    Allow => "allow",
    Review => "review",
    Ask => "ask",
    Deny => "deny",
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
pub struct AllowRuleInput {
    pub run_id: RunId,
    pub approval_id: String,
    pub scope: AllowRuleScope,
}

impl Validate for AllowRuleInput {
    fn validate(&self) -> Result<(), AppError> {
        if is_wire_uuid(&self.approval_id) {
            Ok(())
        } else {
            Err(invalid("approvalId", "uuid"))
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

fn check_pack_name(field: &str, pack: &str) -> Result<(), AppError> {
    check::non_empty(field, pack)?;
    check::max_len(field, pack, POLICY_PACK_NAME_MAX)?;
    if pack.contains(['/', '\\', ':']) || pack.starts_with('.') {
        return Err(invalid(field, "fileName"));
    }
    Ok(())
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserRuleDto {
    pub id: String,
    pub text: String,
    pub effect: PolicyRuleEffect,
    pub pack: String,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub created_at: Option<Timestamp>,
    pub active: bool,
}

impl Validate for UserRuleDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("id", &self.id)?;
        check::max_len("id", &self.id, POLICY_RULE_ID_MAX)?;
        check::non_empty("text", &self.text)?;
        check::max_len("text", &self.text, POLICY_RULE_TEXT_MAX)?;
        check_pack_name("pack", &self.pack)?;
        if let Some(created) = self.created_at {
            check::timestamp("createdAt", created)?;
        }
        Ok(())
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserRulesDto {
    pub rules: Vec<UserRuleDto>,
    pub unconfirmed: Vec<String>,
}

impl Validate for UserRulesDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.rules.len() > USER_RULES_MAX {
            return Err(invalid("rules", "maxItems"));
        }
        self.rules
            .iter()
            .enumerate()
            .try_for_each(|(index, rule)| nested(&format!("rules.{index}"), rule.validate()))?;
        self.unconfirmed
            .iter()
            .try_for_each(|pack| check_pack_name("unconfirmed", pack))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveRuleInput {
    pub id: String,
}

impl Validate for RemoveRuleInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("id", &self.id)?;
        check::max_len("id", &self.id, POLICY_RULE_ID_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmRulePackInput {
    pub pack: String,
}

impl Validate for ConfirmRulePackInput {
    fn validate(&self) -> Result<(), AppError> {
        check_pack_name("pack", &self.pack)
    }
}
