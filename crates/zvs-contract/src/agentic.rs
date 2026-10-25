use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::folders::check_folder_path;
use crate::ids::{AgenticSessionId, ProviderId, RunId, StepId, StreamId};
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, nested, no_refinements};

pub const AGENTIC_GOAL_MAX: usize = 4000;
pub const AGENTIC_STEP_TITLE_MAX: usize = 200;
pub const AGENTIC_SUMMARY_MAX: usize = 2000;
pub const AGENTIC_INSTRUCTION_MAX: usize = 4000;
pub const AGENTIC_PLAN_MAX: usize = 50;
pub const AGENTIC_TOOLS_MAX: usize = 256;
pub const AGENTIC_STEPS_MAX: u64 = 1000;
pub const AGENTIC_TOKENS_MIN: u64 = 1000;
pub const AGENTIC_TOKENS_MAX: u64 = 100_000_000;
pub const AGENTIC_MINUTES_MAX: u64 = 1440;
pub const AGENTIC_DEFAULT_STEPS: u64 = 25;
pub const AGENTIC_DEFAULT_TOKENS: u64 = 200_000;
pub const AGENTIC_DEFAULT_MINUTES: u64 = 15;

wire_enum!(AgenticPermissionMode {
    ReadOnly => "read-only",
    Ask => "ask",
    Auto => "auto",
});

wire_enum!(AgenticSessionStatus {
    Running => "running",
    Paused => "paused",
    Completed => "completed",
    Stopped => "stopped",
    Failed => "failed",
    Cancelled => "cancelled",
});

wire_enum!(AgenticStopReason {
    Goal => "goal",
    Steps => "steps",
    Tokens => "tokens",
    Time => "time",
    Loop => "loop",
    Cancelled => "cancelled",
    Error => "error",
});

wire_enum!(AgenticPlanStepStatus {
    Queued => "queued",
    Running => "running",
    Done => "done",
    Failed => "failed",
});

impl AgenticSessionStatus {
    pub const fn is_finished(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Stopped | Self::Failed | Self::Cancelled
        )
    }
}

fn check_title(field: &str, value: &str) -> Result<(), AppError> {
    check::trimmed_name(field, value, AGENTIC_STEP_TITLE_MAX)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticBudgetDto {
    pub max_steps: u64,
    pub max_tokens: u64,
    pub max_minutes: u64,
}

impl Default for AgenticBudgetDto {
    fn default() -> Self {
        Self {
            max_steps: AGENTIC_DEFAULT_STEPS,
            max_tokens: AGENTIC_DEFAULT_TOKENS,
            max_minutes: AGENTIC_DEFAULT_MINUTES,
        }
    }
}

impl Validate for AgenticBudgetDto {
    fn validate(&self) -> Result<(), AppError> {
        check::int_range("maxSteps", self.max_steps, 1, AGENTIC_STEPS_MAX)?;
        check::int_range(
            "maxTokens",
            self.max_tokens,
            AGENTIC_TOKENS_MIN,
            AGENTIC_TOKENS_MAX,
        )?;
        check::int_range("maxMinutes", self.max_minutes, 1, AGENTIC_MINUTES_MAX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticUsageDto {
    pub steps: u64,
    pub tokens: u64,
    pub elapsed_ms: u64,
    pub tool_calls: u64,
}

impl Validate for AgenticUsageDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("steps", self.steps)?;
        check::safe_integer("tokens", self.tokens)?;
        check::safe_integer("elapsedMs", self.elapsed_ms)?;
        check::safe_integer("toolCalls", self.tool_calls)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticPlanStepDto {
    pub id: String,
    pub title: String,
    pub status: AgenticPlanStepStatus,
    pub step_ids: Vec<StepId>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub summary: Option<String>,
}

impl Validate for AgenticPlanStepDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("id", &self.id)?;
        check::max_len("id", &self.id, 16)?;
        check_title("title", &self.title)?;
        if let Some(summary) = &self.summary {
            check::max_len("summary", summary, AGENTIC_SUMMARY_MAX)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticPlanEditDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub id: Option<String>,
    pub title: String,
}

impl Validate for AgenticPlanEditDto {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(id) = &self.id {
            check::non_empty("id", id)?;
            check::max_len("id", id, 16)?;
        }
        check_title("title", &self.title)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticChangedFileDto {
    pub path: String,
    pub added: u64,
    pub removed: u64,
}

impl Validate for AgenticChangedFileDto {
    fn validate(&self) -> Result<(), AppError> {
        check_folder_path("path", &self.path)?;
        check::safe_integer("added", self.added)?;
        check::safe_integer("removed", self.removed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticSessionDto {
    pub id: AgenticSessionId,
    pub goal: String,
    pub status: AgenticSessionStatus,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub stop_reason: Option<AgenticStopReason>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub error: Option<String>,
    pub mode: AgenticPermissionMode,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub pending_mode: Option<AgenticPermissionMode>,
    pub workspace: String,
    pub provider_id: ProviderId,
    pub model_id: String,
    pub tools: Vec<String>,
    pub budget: AgenticBudgetDto,
    pub usage: AgenticUsageDto,
    pub plan: Vec<AgenticPlanStepDto>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub pending_plan: Option<Vec<AgenticPlanEditDto>>,
    pub pending_instructions: Vec<String>,
    pub changed_files: Vec<AgenticChangedFileDto>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub run_id: Option<RunId>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub stream_id: Option<StreamId>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub summary: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Validate for AgenticSessionDto {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("goal", &self.goal, AGENTIC_GOAL_MAX)?;
        if (self.status == AgenticSessionStatus::Running) == self.stop_reason.is_some() {
            return Err(invalid("stopReason", "status"));
        }
        check_folder_path("workspace", &self.workspace)?;
        check::non_empty("modelId", &self.model_id)?;
        nested("budget", self.budget.validate())?;
        nested("usage", self.usage.validate())?;
        nested("plan", self.plan.validate())?;
        if let Some(pending) = &self.pending_plan {
            nested("pendingPlan", pending.validate())?;
        }
        if self.run_id.is_some() != self.stream_id.is_some() {
            return Err(invalid("streamId", "runId"));
        }
        check::timestamp("createdAt", self.created_at)?;
        check::timestamp("updatedAt", self.updated_at)
    }
}

pub type AgenticListOutput = Vec<AgenticSessionDto>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAgenticSessionInput {
    pub goal: String,
    pub workspace: String,
    pub provider_id: ProviderId,
    pub model_id: String,
    pub mode: AgenticPermissionMode,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub budget: Option<AgenticBudgetDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub tools: Option<Vec<String>>,
}

impl Validate for CreateAgenticSessionInput {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("goal", &self.goal, AGENTIC_GOAL_MAX)?;
        check_folder_path("workspace", &self.workspace)?;
        check::non_empty("modelId", &self.model_id)?;
        if let Some(budget) = &self.budget {
            nested("budget", budget.validate())?;
        }
        if let Some(tools) = &self.tools {
            if tools.len() > AGENTIC_TOOLS_MAX {
                return Err(invalid("tools", "maxLength"));
            }
            for (index, key) in tools.iter().enumerate() {
                if !crate::tools::is_tool_key(key) {
                    return Err(invalid(&format!("tools.{index}"), "pattern"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticSessionRef {
    pub id: AgenticSessionId,
}

no_refinements!(AgenticSessionRef);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAgenticPlanInput {
    pub id: AgenticSessionId,
    pub steps: Vec<AgenticPlanEditDto>,
}

impl Validate for UpdateAgenticPlanInput {
    fn validate(&self) -> Result<(), AppError> {
        if self.steps.len() > AGENTIC_PLAN_MAX {
            return Err(invalid("steps", "maxLength"));
        }
        nested("steps", self.steps.validate())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAgenticModeInput {
    pub id: AgenticSessionId,
    pub mode: AgenticPermissionMode,
}

no_refinements!(SetAgenticModeInput);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgenticInstructInput {
    pub id: AgenticSessionId,
    pub text: String,
}

impl Validate for AgenticInstructInput {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("text", &self.text, AGENTIC_INSTRUCTION_MAX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtendAgenticSessionInput {
    pub id: AgenticSessionId,
    pub budget: AgenticBudgetDto,
}

impl Validate for ExtendAgenticSessionInput {
    fn validate(&self) -> Result<(), AppError> {
        nested("budget", self.budget.validate())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_budget_is_the_conservative_one_and_validates() {
        let budget = AgenticBudgetDto::default();
        assert_eq!(
            (budget.max_steps, budget.max_tokens, budget.max_minutes),
            (25, 200_000, 15)
        );
        assert!(budget.validate().is_ok());
        assert!(
            AgenticBudgetDto {
                max_steps: 0,
                ..budget
            }
            .validate()
            .is_err()
        );
        assert!(
            AgenticBudgetDto {
                max_minutes: AGENTIC_MINUTES_MAX + 1,
                ..budget
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn plan_edits_need_a_title_and_a_bounded_list() {
        let edit = |title: &str| AgenticPlanEditDto {
            id: None,
            title: title.to_owned(),
        };
        let id =
            AgenticSessionId::parse("0199aa11-8888-7111-8111-111111111111").expect("a session id");
        let input = |steps| UpdateAgenticPlanInput { id, steps };
        assert!(input(vec![edit("Прочитать файлы")]).validate().is_ok());
        assert!(input(vec![edit("   ")]).validate().is_err());
        assert!(
            input(vec![edit("Шаг"); AGENTIC_PLAN_MAX + 1])
                .validate()
                .is_err()
        );
    }
}
