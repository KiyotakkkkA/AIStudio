use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use serde_json::{Map, Value, json};
use zvs_contract::validate::{js_slice, js_trim};
use zvs_contract::{
    AppError, AppErrorCode, ChatGoalDto, ChatGoalStepDto, ChatToolCallStatus, ConversationId,
    GOAL_SUMMARY_MAX, GoalBudgetDto, GoalReason, GoalStatus, HostEventDraft, OfferedToolSource,
    RunOutcomeStatus, Timestamp,
};

use super::chat_tools::{OfferedTool, wire_tool_name};
use super::tool_loop::{BridgeOutcome, RoundHook, RoundNext, RoundReport};
use crate::agentic::plan::{LoopGuard, signature};
use crate::data::repositories::ConversationPatch;
use crate::data::{Database, Repositories};
use crate::kernel::StepContext;
use crate::platform::clock::SharedClock;

pub const ROUNDS_KEY: &str = "chat.goal.maxRounds";
pub const TOKENS_KEY: &str = "chat.goal.maxTokens";
pub const MINUTES_KEY: &str = "chat.goal.maxMinutes";

pub const GOAL_DESCRIPTION: &str = "Работа над целью в несколько раундов до подтверждённого результата, исчерпания бюджета или остановки";
pub const GOAL_CONTINUE_TEXT: &str = "Продолжить работу над целью";
pub const GOAL_RUNNING: &str = "В диалоге выполняется цель";
pub const GOAL_NOT_PAUSED: &str = "Продолжить можно только приостановленную цель";
pub const GOAL_NOT_STOPPABLE: &str = "Цель не выполняется и не приостановлена";
pub const GOAL_NEEDS_TOOLS: &str =
    "Модель не поддерживает вызов инструментов, необходимый для цели";
pub const GOAL_NOT_ACTIVE: &str = "Цель диалога не активна";
pub const GOAL_EVIDENCE_REQUIRED: &str = "Не указано подтверждение достижения цели";

pub const GOAL_TOOL: &str = "goal.complete";
pub const GOAL_TOOL_NAME: &str = "complete_goal";
pub const GOAL_TOOL_DESCRIPTION: &str = "Declare the goal achieved. Call it only after you have verified the result. summary is a short account of what was done; evidence is the concrete proof: commands run with their results, tests that pass, files changed.";
pub const GOAL_HEADING: &str = "Goal mode";
pub const GOAL_RULES: &str = "Keep working on the goal across rounds without waiting for the user: decide the next step, act with the tools and check what they return. Verify the outcome before declaring it achieved, for example by running the tests or reading the changed files. Ask the user with the ask tool only when you are blocked and cannot continue without their decision. Do not repeat a call that already failed the same way; change the approach instead.";
pub const GOAL_NUDGE: &str = "Goal mode is still on. Continue working on the goal with the tools. If it is achieved and verified, call the completion tool with the summary and the evidence.";
pub const GOAL_RECORDED: &str = "The goal is recorded as achieved.";
pub const GOAL_EVIDENCE_PROBLEM: &str = "Both summary and evidence are required and must not be empty. Verify the result, then call the tool again with the evidence.";

pub fn goal_tool(offered: &[OfferedTool]) -> OfferedTool {
    let taken: HashSet<String> = offered.iter().map(|tool| tool.name.clone()).collect();
    let name = if taken.contains(GOAL_TOOL_NAME) {
        wire_tool_name(GOAL_TOOL, &taken)
    } else {
        GOAL_TOOL_NAME.to_owned()
    };
    OfferedTool {
        key: GOAL_TOOL.to_owned(),
        name,
        description: GOAL_TOOL_DESCRIPTION.to_owned(),
        parameters: json!({
            "type": "object",
            "properties": {
                "summary": { "type": "string", "description": "What was done, in a few sentences" },
                "evidence": { "type": "string", "description": "Commands run and their results, tests, changed files" }
            },
            "required": ["summary", "evidence"]
        }),
        destructive: false,
        read_only: false,
        source: OfferedToolSource::Base,
        server: None,
    }
}

pub fn goal_section(goal: &ChatGoalDto, tool_name: &str) -> String {
    let budget = goal.budget;
    let usage = goal.usage;
    format!(
        "{GOAL_HEADING}\nThe user set this goal for the conversation:\n<goal>\n{}\n</goal>\n{GOAL_RULES} When the goal is achieved and verified, call {tool_name} with the summary and the evidence. Budget: {} rounds, {} tokens, {} minutes; used so far: {} rounds, {} tokens, {} minutes.",
        goal.text,
        budget.rounds,
        budget.tokens,
        budget.minutes,
        usage.rounds,
        usage.tokens,
        usage.elapsed_ms / 60_000,
    )
}

pub fn default_budget(repositories: &Repositories<'_>) -> GoalBudgetDto {
    let fallback = GoalBudgetDto::default();
    let read = |key: &str, fallback: u64| {
        repositories
            .settings
            .get(key)
            .ok()
            .flatten()
            .and_then(|row| serde_json::from_str::<u64>(&row.value).ok())
            .unwrap_or(fallback)
    };
    let budget = GoalBudgetDto {
        rounds: read(ROUNDS_KEY, fallback.rounds),
        tokens: read(TOKENS_KEY, fallback.tokens),
        minutes: read(MINUTES_KEY, fallback.minutes),
    };
    if budget.check_limits().is_ok() {
        budget
    } else {
        fallback
    }
}

fn text_field(arguments: &Value, key: &str) -> String {
    match arguments.get(key) {
        Some(Value::String(text)) => js_trim(text).to_owned(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(js_trim)
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

pub fn completion(arguments: &Value) -> Option<String> {
    let summary = text_field(arguments, "summary");
    let evidence = text_field(arguments, "evidence");
    if summary.is_empty() || evidence.is_empty() {
        return None;
    }
    Some(js_slice(&summary, GOAL_SUMMARY_MAX).to_owned())
}

pub fn ended(
    mut goal: ChatGoalDto,
    status: GoalStatus,
    reason: Option<GoalReason>,
    now: Timestamp,
) -> ChatGoalDto {
    goal.status = status;
    goal.reason = reason;
    if status.is_finished() {
        goal.ended_at = Some(now);
    }
    goal
}

pub fn settled(
    goal: ChatGoalDto,
    outcome: RunOutcomeStatus,
    now: Timestamp,
) -> Option<ChatGoalDto> {
    if !goal.active() {
        return None;
    }
    Some(match outcome {
        RunOutcomeStatus::Cancelled => {
            ended(goal, GoalStatus::Stopped, Some(GoalReason::User), now)
        }
        RunOutcomeStatus::Failed | RunOutcomeStatus::Ok => {
            ended(goal, GoalStatus::Failed, Some(GoalReason::Error), now)
        }
    })
}

pub fn restarted(goal: ChatGoalDto) -> Option<ChatGoalDto> {
    goal.active()
        .then(|| ended(goal, GoalStatus::Paused, Some(GoalReason::Restart), 0))
}

pub fn emit_goal(
    context: &StepContext,
    conversation: ConversationId,
    goal: &ChatGoalDto,
) -> Result<(), AppError> {
    let Ok(Value::Object(step)) = serde_json::to_value(ChatGoalStepDto::of(conversation, goal))
    else {
        return Ok(());
    };
    context.emit(HostEventDraft::Step {
        step: Map::from_iter(step),
    })
}

struct GoalProgress {
    goal: ChatGoalDto,
    base_elapsed: u64,
    started: Timestamp,
    guard: LoopGuard,
    completed: Option<String>,
}

pub(crate) struct GoalHook {
    database: Arc<Database>,
    clock: SharedClock,
    conversation: ConversationId,
    tool_name: String,
    progress: Mutex<GoalProgress>,
}

impl GoalHook {
    pub(crate) fn start(
        database: Arc<Database>,
        clock: SharedClock,
        conversation: &str,
        offered: &[OfferedTool],
        context: &StepContext,
    ) -> Result<Self, AppError> {
        let id = ConversationId::parse(conversation)
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, GOAL_NOT_ACTIVE))?;
        let goal = database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).chat.get(conversation)?)
            })?
            .and_then(|row| row.goal)
            .filter(ChatGoalDto::active)
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, GOAL_NOT_ACTIVE))?;
        let tool_name = offered
            .iter()
            .find(|tool| tool.key == GOAL_TOOL)
            .map_or_else(|| GOAL_TOOL_NAME.to_owned(), |tool| tool.name.clone());
        emit_goal(context, id, &goal)?;
        let started = clock.now();
        Ok(Self {
            database,
            clock,
            conversation: id,
            tool_name,
            progress: Mutex::new(GoalProgress {
                base_elapsed: goal.usage.elapsed_ms,
                goal,
                started,
                guard: LoopGuard::default(),
                completed: None,
            }),
        })
    }

    fn save(&self, goal: &ChatGoalDto) -> Result<bool, AppError> {
        let id = self.conversation.to_string();
        self.database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            let active = repositories
                .chat
                .get(&id)?
                .and_then(|row| row.goal)
                .is_some_and(|goal| goal.active());
            if active {
                repositories.chat.update(
                    &id,
                    &ConversationPatch {
                        goal: Some(Some(goal.clone())),
                        ..ConversationPatch::default()
                    },
                )?;
            }
            Ok(active)
        })
    }
}

#[async_trait]
impl RoundHook for GoalHook {
    fn handles(&self, key: &str) -> bool {
        key == GOAL_TOOL
    }

    fn bridge(&self, _key: &str, arguments: &Value) -> BridgeOutcome {
        match completion(arguments) {
            Some(summary) => {
                self.progress
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .completed = Some(summary);
                BridgeOutcome {
                    status: ChatToolCallStatus::Succeeded,
                    error: None,
                    result: Some(GOAL_RECORDED.to_owned()),
                }
            }
            None => BridgeOutcome {
                status: ChatToolCallStatus::Failed,
                error: Some(GOAL_EVIDENCE_REQUIRED.to_owned()),
                result: Some(GOAL_EVIDENCE_PROBLEM.to_owned()),
            },
        }
    }

    async fn after_round(
        &self,
        context: &StepContext,
        report: &RoundReport<'_>,
    ) -> Result<RoundNext, AppError> {
        let now = self.clock.now();
        let (goal, next) = {
            let mut progress = self.progress.lock().unwrap_or_else(PoisonError::into_inner);
            let elapsed = u64::try_from(now.saturating_sub(progress.started)).unwrap_or(0);
            let mut goal = progress.goal.clone();
            goal.usage.rounds += 1;
            goal.usage.tokens = goal.usage.tokens.saturating_add(report.tokens);
            goal.usage.elapsed_ms = progress.base_elapsed.saturating_add(elapsed);
            let watched: Vec<(String, String)> = report
                .tool_calls
                .iter()
                .filter(|call| call.name != self.tool_name)
                .map(|call| (call.name.clone(), call.arguments.clone()))
                .collect();
            let tripped = !watched.is_empty() && progress.guard.round(signature(&watched));
            let (goal, next) = if let Some(summary) = progress.completed.take() {
                goal.summary = Some(summary);
                (ended(goal, GoalStatus::Done, None, now), RoundNext::Stop)
            } else if tripped {
                (
                    ended(goal, GoalStatus::Stopped, Some(GoalReason::Repetition), now),
                    RoundNext::Stop,
                )
            } else if let Some(reason) = goal.budget.exceeded(&goal.usage) {
                (
                    ended(goal, GoalStatus::Paused, Some(reason), now),
                    RoundNext::Stop,
                )
            } else if report.called {
                (goal, RoundNext::Continue)
            } else {
                (goal, RoundNext::Nudge(GOAL_NUDGE.to_owned()))
            };
            progress.goal = goal.clone();
            (goal, next)
        };
        if !self.save(&goal)? {
            return Ok(RoundNext::Stop);
        }
        emit_goal(context, self.conversation, &goal)?;
        Ok(next)
    }
}
