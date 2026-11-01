use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use serde_json::{Map, Value, json};
use zvs_contract::validate::{js_slice, js_trim};
use zvs_contract::{
    AGENTIC_PLAN_MAX, AppError, AppErrorCode, ChatGoalDto, ChatGoalStepDto, ChatToolCallStatus,
    ConversationId, GOAL_SUMMARY_MAX, GoalBudgetDto, GoalReason, GoalStatus, HostEventDraft,
    OfferedToolSource, RunOutcomeStatus, Timestamp,
};

use super::chat_tools::{OfferedTool, wire_tool_name};
use super::tool_loop::{BridgeOutcome, RoundHook, RoundNext, RoundReport};
use crate::agentic::node::{COMPLETE_DESCRIPTION, NO_STEP_RUNNING, PLAN_DESCRIPTION, PLAN_EMPTY};
use crate::agentic::plan::{LoopGuard, complete, describe, finish, propose, signature};
use crate::data::repositories::ConversationPatch;
use crate::data::{Database, Repositories};
use crate::kernel::StepContext;
use crate::platform::clock::SharedClock;

pub const ROUNDS_KEY: &str = "chat.goal.maxRounds";
pub const TOKENS_KEY: &str = "chat.goal.maxTokens";
pub const MINUTES_KEY: &str = "chat.goal.maxMinutes";

pub const GOAL_DESCRIPTION: &str =
    "Работа над целью до подтверждённого результата, исчерпания бюджета или остановки";
pub const GOAL_CONTINUE_TEXT: &str = "Продолжить работу над целью";
pub const GOAL_RUNNING: &str = "В диалоге выполняется цель";
pub const GOAL_NOT_PAUSED: &str = "Продолжить можно только приостановленную цель";
pub const GOAL_NOT_STOPPABLE: &str = "Цель не выполняется и не приостановлена";
pub const GOAL_NEEDS_TOOLS: &str =
    "Модель не поддерживает вызов инструментов, необходимый для цели";
pub const GOAL_NOT_ACTIVE: &str = "Цель диалога не активна";
pub const GOAL_EVIDENCE_REQUIRED: &str = "Не указано подтверждение достижения цели";

pub const GOAL_PLAN_EMPTY: &str = "План должен содержать хотя бы один невыполненный шаг";
pub const GOAL_NO_STEP: &str = "Нет текущего шага плана";

pub const GOAL_PLAN_TOOL: &str = "goal.plan";
pub const GOAL_PLAN_NAME: &str = "update_plan";
pub const GOAL_STEP_TOOL: &str = "goal.step";
pub const GOAL_STEP_NAME: &str = "complete_step";
pub const GOAL_TOOLS: [&str; 3] = [GOAL_PLAN_TOOL, GOAL_STEP_TOOL, GOAL_TOOL];
pub const GOAL_TOOL: &str = "goal.complete";
pub const GOAL_TOOL_NAME: &str = "complete_goal";
pub const GOAL_TOOL_DESCRIPTION: &str = "Declare the goal achieved. Call it only after you have verified the result. summary is a short account of what was done; evidence is the concrete proof: commands run with their results, tests that pass, files changed.";
pub const GOAL_HEADING: &str = "Goal mode";
pub const GOAL_RULES: &str = "Keep working on the goal across rounds without waiting for the user: decide the next step, act with the tools and check what they return. Verify the outcome before declaring it achieved, for example by running the tests or reading the changed files. Ask the user with the ask tool only when you are blocked and cannot continue without their decision. Do not repeat a call that already failed the same way; change the approach instead.";
pub const GOAL_NUDGE: &str = "Goal mode is still on. Continue working on the goal with the tools. If it is achieved and verified, call the completion tool with the summary and the evidence.";
pub const GOAL_RECORDED: &str = "The goal is recorded as achieved.";
pub const GOAL_EVIDENCE_PROBLEM: &str = "Both summary and evidence are required and must not be empty. Verify the result, then call the tool again with the evidence.";

fn bridge_tool(
    taken: &mut HashSet<String>,
    key: &str,
    preferred: &str,
    description: &str,
    parameters: Value,
) -> OfferedTool {
    let name = if taken.contains(preferred) {
        wire_tool_name(key, taken)
    } else {
        preferred.to_owned()
    };
    taken.insert(name.clone());
    OfferedTool {
        key: key.to_owned(),
        name,
        description: description.to_owned(),
        parameters,
        destructive: false,
        read_only: false,
        source: OfferedToolSource::Command,
        server: None,
    }
}

pub fn goal_tools(offered: &[OfferedTool]) -> Vec<OfferedTool> {
    let mut taken: HashSet<String> = offered.iter().map(|tool| tool.name.clone()).collect();
    vec![
        bridge_tool(
            &mut taken,
            GOAL_PLAN_TOOL,
            GOAL_PLAN_NAME,
            PLAN_DESCRIPTION,
            json!({
                "type": "object",
                "properties": {
                    "steps": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 1,
                        "maxItems": AGENTIC_PLAN_MAX
                    }
                },
                "required": ["steps"]
            }),
        ),
        bridge_tool(
            &mut taken,
            GOAL_STEP_TOOL,
            GOAL_STEP_NAME,
            COMPLETE_DESCRIPTION,
            json!({
                "type": "object",
                "properties": { "summary": { "type": "string" } },
                "required": ["summary"]
            }),
        ),
        bridge_tool(
            &mut taken,
            GOAL_TOOL,
            GOAL_TOOL_NAME,
            GOAL_TOOL_DESCRIPTION,
            json!({
                "type": "object",
                "properties": {
                    "summary": { "type": "string", "description": "What was done, in a few sentences" },
                    "evidence": { "type": "string", "description": "Commands run and their results, tests, changed files" }
                },
                "required": ["summary", "evidence"]
            }),
        ),
    ]
}

fn tool_name<'a>(offered: &'a [OfferedTool], key: &str, fallback: &'a str) -> &'a str {
    offered
        .iter()
        .find(|tool| tool.key == key)
        .map_or(fallback, |tool| tool.name.as_str())
}

pub fn goal_section(goal: &ChatGoalDto, offered: &[OfferedTool]) -> String {
    let budget = goal.budget;
    let usage = goal.usage;
    let plan = tool_name(offered, GOAL_PLAN_TOOL, GOAL_PLAN_NAME);
    let step = tool_name(offered, GOAL_STEP_TOOL, GOAL_STEP_NAME);
    let complete = tool_name(offered, GOAL_TOOL, GOAL_TOOL_NAME);
    let current = if goal.plan.is_empty() {
        String::new()
    } else {
        format!("\nCurrent plan:\n{}", describe(&goal.plan))
    };
    format!(
        "{GOAL_HEADING}\nThe user set this goal for the conversation:\n<goal>\n{}\n</goal>\n{GOAL_RULES} Start by calling {plan} with a short ordered plan, then carry the steps out one at a time. When a step is done, call {step} with a one-line summary, and revise the remaining steps with {plan} when what you learn changes them. When the goal is achieved and verified, call {complete} with the summary and the evidence. Budget: {} rounds, {} tokens, {} minutes; used so far: {} rounds, {} tokens, {} minutes.{current}",
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
    revised_too_often: bool,
}

fn plan_titles(arguments: &Value) -> Vec<String> {
    match arguments.get("steps") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Some(Value::String(text)) => text.lines().map(str::to_owned).collect(),
        _ => Vec::new(),
    }
}

const fn outcome(
    status: ChatToolCallStatus,
    error: Option<String>,
    result: String,
) -> BridgeOutcome {
    BridgeOutcome {
        status,
        error,
        result: Some(result),
    }
}

pub(crate) struct GoalHook {
    database: Arc<Database>,
    clock: SharedClock,
    conversation: ConversationId,
    bridge_names: Vec<String>,
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
        let bridge_names = offered
            .iter()
            .filter(|tool| GOAL_TOOLS.contains(&tool.key.as_str()))
            .map(|tool| tool.name.clone())
            .collect();
        emit_goal(context, id, &goal)?;
        let started = clock.now();
        Ok(Self {
            database,
            clock,
            conversation: id,
            bridge_names,
            progress: Mutex::new(GoalProgress {
                base_elapsed: goal.usage.elapsed_ms,
                goal,
                started,
                guard: LoopGuard::default(),
                completed: None,
                revised_too_often: false,
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
        GOAL_TOOLS.contains(&key)
    }

    fn bridge(&self, key: &str, arguments: &Value) -> BridgeOutcome {
        if key == GOAL_PLAN_TOOL {
            let mut progress = self.progress.lock().unwrap_or_else(PoisonError::into_inner);
            let mut proposed = progress.goal.plan.clone();
            if propose(&mut proposed, &plan_titles(arguments)) == 0 {
                return outcome(
                    ChatToolCallStatus::Failed,
                    Some(GOAL_PLAN_EMPTY.to_owned()),
                    PLAN_EMPTY.to_owned(),
                );
            }
            progress.goal.plan = proposed;
            if progress.guard.revised() {
                progress.revised_too_often = true;
            }
            return outcome(
                ChatToolCallStatus::Succeeded,
                None,
                format!("Plan updated:\n{}", describe(&progress.goal.plan)),
            );
        }
        if key == GOAL_STEP_TOOL {
            let mut progress = self.progress.lock().unwrap_or_else(PoisonError::into_inner);
            let summary = arguments
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            return match complete(&mut progress.goal.plan, &summary) {
                Some(done) => {
                    progress.guard.progressed();
                    outcome(
                        ChatToolCallStatus::Succeeded,
                        None,
                        format!(
                            "Step done: {done}\nCurrent plan:\n{}",
                            describe(&progress.goal.plan)
                        ),
                    )
                }
                None => outcome(
                    ChatToolCallStatus::Failed,
                    Some(GOAL_NO_STEP.to_owned()),
                    NO_STEP_RUNNING.to_owned(),
                ),
            };
        }
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
                .filter(|call| !self.bridge_names.contains(&call.name))
                .map(|call| (call.name.clone(), call.arguments.clone()))
                .collect();
            let repeated = !watched.is_empty() && progress.guard.round(signature(&watched));
            let tripped = repeated || std::mem::take(&mut progress.revised_too_often);
            let (goal, next) = if let Some(summary) = progress.completed.take() {
                goal.summary = Some(summary);
                finish(&mut goal.plan);
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
