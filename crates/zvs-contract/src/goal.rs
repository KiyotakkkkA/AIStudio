use serde::{Deserialize, Serialize};

use crate::chat::CHAT_TEXT_MAX;
use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::ConversationId;
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, js_trim, nested, no_refinements};

pub const GOAL_STEP_DOMAIN: &str = "goal";
pub const GOAL_SUMMARY_MAX: usize = 4000;
pub const GOAL_DEFAULT_ROUNDS: u64 = 40;
pub const GOAL_DEFAULT_TOKENS: u64 = 400_000;
pub const GOAL_DEFAULT_MINUTES: u64 = 30;
pub const GOAL_ROUNDS_MAX: u64 = 1000;
pub const GOAL_TOKENS_MIN: u64 = 1000;
pub const GOAL_TOKENS_MAX: u64 = 100_000_000;
pub const GOAL_MINUTES_MAX: u64 = 1440;

wire_enum!(GoalStatus {
    Active => "active",
    Paused => "paused",
    Done => "done",
    Stopped => "stopped",
    Failed => "failed",
});

wire_enum!(GoalReason {
    Rounds => "rounds",
    Tokens => "tokens",
    Minutes => "minutes",
    Repetition => "repetition",
    Restart => "restart",
    User => "user",
    Error => "error",
});

impl GoalStatus {
    pub const fn is_finished(self) -> bool {
        matches!(self, Self::Done | Self::Stopped | Self::Failed)
    }

    const fn has_reason(self) -> bool {
        matches!(self, Self::Paused | Self::Stopped | Self::Failed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalBudgetDto {
    pub rounds: u64,
    pub tokens: u64,
    pub minutes: u64,
}

impl Default for GoalBudgetDto {
    fn default() -> Self {
        Self {
            rounds: GOAL_DEFAULT_ROUNDS,
            tokens: GOAL_DEFAULT_TOKENS,
            minutes: GOAL_DEFAULT_MINUTES,
        }
    }
}

impl GoalBudgetDto {
    pub fn check_limits(&self) -> Result<(), AppError> {
        check::int_range("rounds", self.rounds, 1, GOAL_ROUNDS_MAX)?;
        check::int_range("tokens", self.tokens, GOAL_TOKENS_MIN, GOAL_TOKENS_MAX)?;
        check::int_range("minutes", self.minutes, 1, GOAL_MINUTES_MAX)
    }

    pub const fn after(self, usage: &GoalUsageDto) -> Self {
        Self {
            rounds: usage.rounds.saturating_add(self.rounds),
            tokens: usage.tokens.saturating_add(self.tokens),
            minutes: (usage.elapsed_ms / 60_000).saturating_add(self.minutes),
        }
    }

    pub const fn exceeded(&self, usage: &GoalUsageDto) -> Option<GoalReason> {
        if usage.rounds >= self.rounds {
            Some(GoalReason::Rounds)
        } else if usage.tokens >= self.tokens {
            Some(GoalReason::Tokens)
        } else if usage.elapsed_ms >= self.minutes.saturating_mul(60_000) {
            Some(GoalReason::Minutes)
        } else {
            None
        }
    }
}

impl Validate for GoalBudgetDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("rounds", self.rounds)?;
        check::safe_integer("tokens", self.tokens)?;
        check::safe_integer("minutes", self.minutes)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalUsageDto {
    pub rounds: u64,
    pub tokens: u64,
    pub elapsed_ms: u64,
}

impl Validate for GoalUsageDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("rounds", self.rounds)?;
        check::safe_integer("tokens", self.tokens)?;
        check::safe_integer("elapsedMs", self.elapsed_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGoalDto {
    pub text: String,
    pub status: GoalStatus,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub reason: Option<GoalReason>,
    pub budget: GoalBudgetDto,
    pub usage: GoalUsageDto,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub summary: Option<String>,
    pub started_at: Timestamp,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub ended_at: Option<Timestamp>,
}

impl ChatGoalDto {
    pub fn new(text: &str, budget: GoalBudgetDto, now: Timestamp) -> Self {
        Self {
            text: js_trim(text).to_owned(),
            status: GoalStatus::Active,
            reason: None,
            budget,
            usage: GoalUsageDto::default(),
            summary: None,
            started_at: now,
            ended_at: None,
        }
    }

    pub const fn active(&self) -> bool {
        matches!(self.status, GoalStatus::Active)
    }
}

impl Validate for ChatGoalDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("text", js_trim(&self.text))?;
        check::max_len("text", &self.text, CHAT_TEXT_MAX)?;
        nested("budget", self.budget.validate())?;
        nested("usage", self.usage.validate())?;
        if self.status.has_reason() != self.reason.is_some() {
            return Err(invalid("reason", "status"));
        }
        if let Some(summary) = &self.summary {
            check::max_len("summary", summary, GOAL_SUMMARY_MAX)?;
        }
        if self.summary.is_some() && self.status != GoalStatus::Done {
            return Err(invalid("summary", "status"));
        }
        check::timestamp("startedAt", self.started_at)?;
        if let Some(ended) = self.ended_at {
            check::timestamp("endedAt", ended)?;
        }
        if self.status.is_finished() != self.ended_at.is_some() {
            return Err(invalid("endedAt", "status"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGoalStepDto {
    pub domain: String,
    pub conversation_id: ConversationId,
    pub status: GoalStatus,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub reason: Option<GoalReason>,
    pub round: u64,
    pub usage: GoalUsageDto,
    pub budget: GoalBudgetDto,
}

impl ChatGoalStepDto {
    pub fn of(conversation_id: ConversationId, goal: &ChatGoalDto) -> Self {
        Self {
            domain: GOAL_STEP_DOMAIN.to_owned(),
            conversation_id,
            status: goal.status,
            reason: goal.reason,
            round: goal.usage.rounds,
            usage: goal.usage,
            budget: goal.budget,
        }
    }
}

impl Validate for ChatGoalStepDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.domain != GOAL_STEP_DOMAIN {
            return Err(invalid("domain", "literal"));
        }
        if self.status.has_reason() != self.reason.is_some() {
            return Err(invalid("reason", "status"));
        }
        check::safe_integer("round", self.round)?;
        nested("usage", self.usage.validate())?;
        nested("budget", self.budget.validate())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGoalInput {
    pub conversation_id: ConversationId,
}

no_refinements!(ChatGoalInput);

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::validate::parse;

    fn goal(status: &str, reason: Value, summary: Value, ended: Value) -> Value {
        json!({
            "text": "Добавить тесты",
            "status": status,
            "reason": reason,
            "budget": { "rounds": 40, "tokens": 400000, "minutes": 30 },
            "usage": { "rounds": 3, "tokens": 1200, "elapsedMs": 60000 },
            "summary": summary,
            "startedAt": 1,
            "endedAt": ended,
        })
    }

    #[test]
    fn a_reason_belongs_to_a_pause_a_stop_or_a_failure() {
        assert!(
            parse::<ChatGoalDto>(goal("active", json!(null), json!(null), json!(null))).is_ok()
        );
        assert!(
            parse::<ChatGoalDto>(goal("paused", json!("rounds"), json!(null), json!(null))).is_ok()
        );
        assert!(
            parse::<ChatGoalDto>(goal("active", json!("rounds"), json!(null), json!(null)))
                .is_err()
        );
        assert!(
            parse::<ChatGoalDto>(goal("paused", json!(null), json!(null), json!(null))).is_err()
        );
        assert!(
            parse::<ChatGoalDto>(goal("stopped", json!("user"), json!(null), json!(5))).is_ok()
        );
    }

    #[test]
    fn only_a_finished_goal_has_an_end_and_only_a_done_one_a_summary() {
        assert!(parse::<ChatGoalDto>(goal("done", json!(null), json!("Готово"), json!(5))).is_ok());
        assert!(
            parse::<ChatGoalDto>(goal("done", json!(null), json!("Готово"), json!(null))).is_err()
        );
        assert!(
            parse::<ChatGoalDto>(goal("paused", json!("tokens"), json!("x"), json!(null))).is_err()
        );
        assert!(
            parse::<ChatGoalDto>(goal("failed", json!("error"), json!(null), json!(null))).is_err()
        );
    }

    #[test]
    fn a_budget_trips_on_rounds_then_tokens_then_minutes_and_renews_above_usage() {
        let budget = GoalBudgetDto::default();
        let usage = |rounds, tokens, elapsed_ms| GoalUsageDto {
            rounds,
            tokens,
            elapsed_ms,
        };
        assert_eq!(budget.exceeded(&usage(39, 0, 0)), None);
        assert_eq!(budget.exceeded(&usage(40, 0, 0)), Some(GoalReason::Rounds));
        assert_eq!(
            budget.exceeded(&usage(1, 400_000, 0)),
            Some(GoalReason::Tokens)
        );
        assert_eq!(
            budget.exceeded(&usage(1, 1, 30 * 60_000)),
            Some(GoalReason::Minutes)
        );
        let renewed = budget.after(&usage(40, 10, 125_000));
        assert_eq!(
            renewed,
            GoalBudgetDto {
                rounds: 80,
                tokens: 400_010,
                minutes: 32,
            }
        );
        assert!(renewed.check_limits().is_ok());
    }
}
