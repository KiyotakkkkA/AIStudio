use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::folders::check_folder_path;
use crate::ids::{ConversationId, RunId, is_wire_uuid};
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, nested};

pub const PROCESS_APPROVAL_KIND: &str = "process";
pub const PROCESS_PROGRAM_MAX: usize = 32_767;
pub const PROCESS_ARGV_MAX: usize = 64;
pub const PROCESS_ARG_MAX: usize = 4096;
pub const PROCESS_SOURCE_MAX: usize = 262_144;
pub const PROCESS_PACKAGES_MAX: usize = 32;
pub const PROCESS_PACKAGE_MAX: usize = 214;
pub const PROCESS_ACTION_MAX: usize = 2000;

wire_enum!(CodeLanguage {
    Python => "python",
    Javascript => "javascript",
    Powershell => "powershell",
});

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessPlanDto {
    pub program: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub language: Option<CodeLanguage>,
    pub argv: Vec<String>,
    pub cwd: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub source: Option<String>,
    pub packages: Vec<String>,
}

impl Validate for ProcessPlanDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("program", &self.program)?;
        check::max_len("program", &self.program, PROCESS_PROGRAM_MAX)?;
        if self.argv.len() > PROCESS_ARGV_MAX {
            return Err(invalid("argv", "maxItems"));
        }
        for (index, argument) in self.argv.iter().enumerate() {
            check::max_len(&format!("argv.{index}"), argument, PROCESS_ARG_MAX)?;
        }
        check_folder_path("cwd", &self.cwd)?;
        match (&self.source, self.language) {
            (Some(source), Some(_)) => check::max_len("source", source, PROCESS_SOURCE_MAX)?,
            (None, None) => {}
            _ => return Err(invalid("source", "language")),
        }
        if self.packages.len() > PROCESS_PACKAGES_MAX {
            return Err(invalid("packages", "maxItems"));
        }
        if !self.packages.is_empty() && self.language != Some(CodeLanguage::Javascript) {
            return Err(invalid("packages", "language"));
        }
        for (index, package) in self.packages.iter().enumerate() {
            let field = format!("packages.{index}");
            check::non_empty(&field, package)?;
            check::max_len(&field, package, PROCESS_PACKAGE_MAX)?;
        }
        Ok(())
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessResultDto {
    pub process: ProcessPlanDto,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub timed_out: bool,
    pub truncated: bool,
}

impl Validate for ProcessResultDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("process", self.process.validate())?;
        check::safe_integer("durationMs", self.duration_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessApprovalDto {
    pub id: String,
    pub run_id: RunId,
    pub kind: String,
    pub subject: String,
    pub scope: String,
    pub expires_at: Timestamp,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub conversation_id: Option<ConversationId>,
    pub tool: String,
    pub action: String,
    pub process: ProcessPlanDto,
}

impl Validate for ProcessApprovalDto {
    fn validate(&self) -> Result<(), AppError> {
        if !is_wire_uuid(&self.id) {
            return Err(invalid("id", "uuid"));
        }
        if self.kind != PROCESS_APPROVAL_KIND {
            return Err(invalid("kind", "literal"));
        }
        check::non_empty("subject", &self.subject)?;
        check::non_empty("tool", &self.tool)?;
        check::max_len("action", &self.action, PROCESS_ACTION_MAX)?;
        check::safe_integer("expiresAt", self.expires_at.unsigned_abs())?;
        nested("process", self.process.validate())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::parse;
    use serde_json::json;

    #[test]
    fn only_javascript_takes_packages_and_source_needs_a_language() {
        let plan = json!({
            "program": "Node.js",
            "language": "javascript",
            "argv": [],
            "cwd": r"D:\scratch\run",
            "source": "console.log(1)",
            "packages": ["lodash@4.17.21"]
        });
        assert!(parse::<ProcessPlanDto>(plan.clone()).is_ok());
        let mut python = plan.clone();
        python["language"] = json!("python");
        assert!(parse::<ProcessPlanDto>(python).is_err());
        let mut bare = plan;
        bare.as_object_mut().expect("an object").remove("language");
        assert!(parse::<ProcessPlanDto>(bare).is_err());
    }
}
