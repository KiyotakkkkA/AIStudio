use std::collections::HashMap;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use zvs_contract::validate::check;
use zvs_contract::{
    AgenticBudgetDto, AgenticChangedFileDto, AgenticPermissionMode, AgenticSessionStatus,
    AgenticStopReason, AgenticUsageDto, AppError, AppErrorCode, HostEventDraft, ToolListFilter,
    ToolMode, ToolTier, Validate, log_line, resolve_tool_mode,
};

use super::plan::{
    LoopGuard, Plan, PlanChange, complete, describe, edit, finish, link, propose, record_changes,
    signature, summary,
};
use super::store::SessionStore;
use crate::commands::PROCESS_TOOLS;
use crate::data::schema::{AgenticSessionRow, TranscriptCall, TranscriptEntry};
use crate::drivers::ai::{
    ChatMessage, ChatRole, GenerateRequest, ToolCall, ToolSpec, with_injected_tools,
};
use crate::files::FILE_TOOLS;
use crate::kernel::nodes::PROVIDERS_UNAVAILABLE;
use crate::kernel::{NodeDef, PermissionRequirement, StepContext, ToolStepRequest};
use crate::services::chat::{FILE_TOOLS_RULES, GenerateChatNode, RoundOutput};
use crate::services::chat_tools::{
    OfferedTool, TOOL_RESULT_MAX_CHARS, TOOL_RESULTS_INSTRUCTION, cap_text, fence_tool_result,
    offered_tools, parse_tool_arguments, tool_result_text, tool_specs,
};
use crate::services::token_estimate::estimate_tokens;
use crate::tools::ToolRegistry;

pub const AGENTIC_SESSION: &str = "agentic.session";
pub const AGENTIC_DOMAIN: &str = "agentic";
pub const SESSION_NODE_ID: &str = "session";
pub const PLAN_TOOL: &str = "update_plan";
pub const COMPLETE_TOOL: &str = "complete_step";
pub const FINISH_TOOL: &str = "finish";
pub const NO_TOOL_SUPPORT: &str =
    "Модель не поддерживает вызов инструментов: агентная сессия невозможна";
pub const TOOL_NOT_OFFERED: &str = "Инструмент не предоставлен сессии";
pub const NOT_THIS_RUN: &str = "Сессия не выполняется в этом запуске";
pub const PLAN_DESCRIPTION: &str = "Задать или пересмотреть план: упорядоченный список ещё не выполненных шагов. Выполненные шаги сохраняются, остальные заменяются этим списком, первый шаг становится текущим.";
pub const COMPLETE_DESCRIPTION: &str = "Отметить текущий шаг плана выполненным и перейти к следующему. В summary — одна строка о результате шага.";
pub const FINISH_DESCRIPTION: &str =
    "Завершить сессию, когда цель достигнута. В summary — итог для пользователя.";
pub const AGENT_RULES: &str = "You are an agent working toward the user's goal in a workspace folder. Work in steps. First call update_plan with a short ordered plan. Then carry out the steps one at a time with the tools offered. When a step is done, call complete_step with a one-line summary of what it produced. When what you learn changes the remaining work, revise it with update_plan. When the goal is reached, call finish with a summary for the user. Messages that start with \"Plan changed by the user\" or \"Instruction from the user\" come from the user: follow them, and never add back a step the user removed. Every tool call is checked by the host against the permission model; a refused or denied call is final, so choose another way or finish.";
pub const READ_ONLY_RULES: &str = "This session is read-only: only tools that read are offered. Do not attempt to change files or run programs.";
pub const ASK_RULES: &str = "The user approves every action that changes something before it runs.";
pub const NO_STEP_RUNNING: &str = "No plan step is in progress. Call update_plan first.";
pub const PLAN_EMPTY: &str = "The plan must list at least one step that is not yet done.";
pub const REMOVED_DROPPED: &str = "Steps the user removed were left out";

fn bridge_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: PLAN_TOOL.to_owned(),
            description: PLAN_DESCRIPTION.to_owned(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "steps": {
                        "type": "array",
                        "items": { "type": "string" },
                        "minItems": 1,
                        "maxItems": 50
                    }
                },
                "required": ["steps"]
            }),
        },
        ToolSpec {
            name: COMPLETE_TOOL.to_owned(),
            description: COMPLETE_DESCRIPTION.to_owned(),
            parameters: json!({
                "type": "object",
                "properties": { "summary": { "type": "string" } },
                "required": ["summary"]
            }),
        },
        ToolSpec {
            name: FINISH_TOOL.to_owned(),
            description: FINISH_DESCRIPTION.to_owned(),
            parameters: json!({
                "type": "object",
                "properties": { "summary": { "type": "string" } },
                "required": ["summary"]
            }),
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionInput {
    pub session_id: String,
}

impl Validate for SessionInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("sessionId", &self.session_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionOutput {
    pub status: String,
    pub stop_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

impl Validate for SessionOutput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

struct Progress {
    plan: Plan,
    usage: AgenticUsageDto,
    changed: Vec<AgenticChangedFileDto>,
    messages: Vec<ChatMessage>,
    mode: AgenticPermissionMode,
}

struct Ending {
    status: AgenticSessionStatus,
    reason: AgenticStopReason,
    summary: Option<String>,
}

impl Ending {
    const fn new(
        status: AgenticSessionStatus,
        reason: AgenticStopReason,
        summary: Option<String>,
    ) -> Self {
        Self {
            status,
            reason,
            summary,
        }
    }
}

pub fn budget_exceeded(
    budget: &AgenticBudgetDto,
    usage: &AgenticUsageDto,
) -> Option<AgenticStopReason> {
    if usage.steps >= budget.max_steps {
        Some(AgenticStopReason::Steps)
    } else if usage.tokens >= budget.max_tokens {
        Some(AgenticStopReason::Tokens)
    } else if usage.elapsed_ms >= budget.max_minutes.saturating_mul(60_000) {
        Some(AgenticStopReason::Time)
    } else {
        None
    }
}

fn role_name(role: ChatRole) -> &'static str {
    match role {
        ChatRole::System => "system",
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
        ChatRole::Tool => "tool",
    }
}

fn to_entry(message: &ChatMessage) -> TranscriptEntry {
    TranscriptEntry {
        role: role_name(message.role).to_owned(),
        content: message.content.clone(),
        tool_call_id: message.tool_call_id.clone(),
        tool_calls: message
            .tool_calls
            .iter()
            .map(|call| TranscriptCall {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments: call.arguments.clone(),
            })
            .collect(),
    }
}

fn from_entry(entry: &TranscriptEntry) -> Option<ChatMessage> {
    let message = match entry.role.as_str() {
        "user" => ChatMessage::user(entry.content.clone()),
        "assistant" => ChatMessage::assistant(entry.content.clone()).with_tool_calls(
            entry
                .tool_calls
                .iter()
                .map(|call| ToolCall::new(&call.id, &call.name, &call.arguments))
                .collect(),
        ),
        "tool" => ChatMessage::tool(entry.tool_call_id.clone()?, entry.content.clone()),
        _ => return None,
    };
    Some(message)
}

fn goal_message(row: &AgenticSessionRow) -> String {
    format!("Goal:\n{}\n\nWorkspace folder: {}", row.goal, row.workspace)
}

fn plan_note(plan: &Plan, change: &PlanChange) -> String {
    let mut note = format!(
        "Plan changed by the user. Current plan:\n{}",
        describe(plan)
    );
    if !change.removed.is_empty() {
        note.push_str(&format!(
            "\nRemoved by the user, do not add back: {}",
            change.removed.join("; ")
        ));
    }
    note
}

fn mode_note(mode: AgenticPermissionMode) -> String {
    format!(
        "Instruction from the user: the permission mode is now {}. The tools offered reflect it.",
        mode.as_str()
    )
}

fn system_prompt(offered: &[OfferedTool], mode: AgenticPermissionMode) -> String {
    let mut parts = vec![AGENT_RULES, TOOL_RESULTS_INSTRUCTION];
    match mode {
        AgenticPermissionMode::ReadOnly => parts.push(READ_ONLY_RULES),
        AgenticPermissionMode::Ask => parts.push(ASK_RULES),
        AgenticPermissionMode::Auto => {}
    }
    if offered
        .iter()
        .any(|tool| FILE_TOOLS.contains(&tool.key.as_str()))
    {
        parts.push(FILE_TOOLS_RULES);
    }
    parts.join("\n\n")
}

fn request_tokens(request: &GenerateRequest) -> u64 {
    let system = request.system.as_deref().map_or(0, estimate_tokens);
    let messages: u64 = request
        .messages
        .iter()
        .map(|message| {
            estimate_tokens(&message.content)
                + message
                    .tool_calls
                    .iter()
                    .map(|call| estimate_tokens(&call.name) + estimate_tokens(&call.arguments))
                    .sum::<u64>()
        })
        .sum();
    let tools: u64 = request
        .tools
        .iter()
        .map(|tool| {
            estimate_tokens(&tool.name)
                + estimate_tokens(&tool.description)
                + estimate_tokens(&tool.parameters.to_string())
        })
        .sum();
    system + messages + tools
}

fn round_tokens(round: &RoundOutput) -> u64 {
    estimate_tokens(&round.text)
        + estimate_tokens(&round.reasoning)
        + round
            .tool_calls
            .iter()
            .map(|call| estimate_tokens(&call.name) + estimate_tokens(&call.arguments))
            .sum::<u64>()
}

fn text_argument(arguments: &Map<String, Value>, key: &str) -> String {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn minimum_tier(tool: &OfferedTool, mode: AgenticPermissionMode) -> Option<ToolTier> {
    match mode {
        AgenticPermissionMode::ReadOnly => None,
        AgenticPermissionMode::Ask => (!tool.read_only).then_some(ToolTier::Ask),
        AgenticPermissionMode::Auto => tool.destructive.then_some(ToolTier::Ask),
    }
}

pub struct SessionNode {
    store: SessionStore,
    tools: ToolRegistry,
}

impl SessionNode {
    pub const fn new(store: SessionStore, tools: ToolRegistry) -> Self {
        Self { store, tools }
    }

    fn offered(
        &self,
        keys: &[String],
        mode: AgenticPermissionMode,
    ) -> Result<Vec<OfferedTool>, AppError> {
        let registered = self.tools.list(&ToolListFilter::default())?;
        let mut offered = offered_tools(keys, &registered);
        offered
            .retain(|tool| ![PLAN_TOOL, COMPLETE_TOOL, FINISH_TOOL].contains(&tool.name.as_str()));
        if mode == AgenticPermissionMode::ReadOnly {
            offered.retain(|tool| tool.read_only);
        }
        Ok(offered)
    }

    fn publish(&self, context: &StepContext, row: &AgenticSessionRow) -> Result<(), AppError> {
        let session = serde_json::to_value(self.store.dto(row)?)
            .map_err(|error| AppError::new(AppErrorCode::Unknown, error.to_string()))?;
        let step = Map::from_iter([
            ("domain".to_owned(), json!(AGENTIC_DOMAIN)),
            ("sessionId".to_owned(), json!(row.id)),
            ("session".to_owned(), session),
        ]);
        context.emit(HostEventDraft::Step { step })
    }

    fn save(
        &self,
        context: &StepContext,
        id: &str,
        progress: &Progress,
        ending: Option<&Ending>,
    ) -> Result<(), AppError> {
        let (row, ()) = self.store.update(id, |row| {
            row.plan = progress.plan.clone();
            row.usage = progress.usage;
            row.changed_files = progress.changed.clone();
            row.transcript = progress.messages.iter().map(to_entry).collect();
            row.mode = progress.mode;
            if let Some(ending) = ending {
                row.status = ending.status;
                row.stop_reason = Some(ending.reason);
                row.summary = ending.summary.clone();
            }
            Ok(())
        })?;
        self.publish(context, &row)
    }

    fn boundary(
        &self,
        context: &StepContext,
        id: &str,
        progress: &mut Progress,
        guard: &mut LoopGuard,
    ) -> Result<(), AppError> {
        let mut changed_mode = None;
        let (row, notes) = self.store.update(id, |row| {
            let mut notes = Vec::new();
            if let Some(mode) = row.pending_mode.take()
                && mode != progress.mode
            {
                progress.mode = mode;
                changed_mode = Some(mode);
                notes.push(mode_note(mode));
            }
            if let Some(edits) = row.pending_plan.take() {
                let change = edit(&mut progress.plan, &edits);
                if change.changed || !change.removed.is_empty() || !change.added.is_empty() {
                    guard.forbid(&change.removed);
                    notes.push(plan_note(&progress.plan, &change));
                }
            }
            for text in std::mem::take(&mut row.pending_instructions) {
                notes.push(format!("Instruction from the user: {text}"));
            }
            row.mode = progress.mode;
            row.plan = progress.plan.clone();
            row.usage = progress.usage;
            Ok(notes)
        })?;
        if let Some(mode) = changed_mode {
            context.emit(HostEventDraft::Log {
                line: log_line(&[
                    ("level", json!("info")),
                    ("message", json!("permission mode changed")),
                    ("mode", json!(mode.as_str())),
                ]),
            })?;
        }
        if !notes.is_empty() {
            progress
                .messages
                .push(ChatMessage::user(notes.join("\n\n")));
            self.publish(context, &row)?;
        }
        Ok(())
    }

    async fn call_tool(
        &self,
        context: &StepContext,
        session_id: &str,
        tool: &OfferedTool,
        call: &ToolCall,
        progress: &mut Progress,
        native: bool,
    ) -> Result<String, AppError> {
        let shape = |status: &str, text: String| {
            if native {
                fence_tool_result(&call.name, status, &text)
            } else {
                text
            }
        };
        let mut arguments = parse_tool_arguments(&call.arguments).unwrap_or_default();
        if FILE_TOOLS.contains(&tool.key.as_str()) || PROCESS_TOOLS.contains(&tool.key.as_str()) {
            arguments.insert("conversationId".to_owned(), json!(session_id));
        }
        progress.usage.tool_calls += 1;
        let Some(invocation) = context.invoke_tool(ToolStepRequest {
            key: tool.key.clone(),
            input: Value::Object(arguments),
            label: format!("call{}", progress.usage.tool_calls),
            minimum_tier: minimum_tier(tool, progress.mode),
            on_approval: None,
        }) else {
            return Ok(shape("refused", TOOL_NOT_OFFERED.to_owned()));
        };
        match invocation.await {
            Ok(result) => {
                link(&mut progress.plan, &result.step_id);
                record_changes(&mut progress.changed, &result.output);
                let rendered = tool_result_text(&result.output);
                let capped = cap_text(&rendered.text, TOOL_RESULT_MAX_CHARS);
                Ok(shape(
                    if rendered.is_error {
                        "failed"
                    } else {
                        "succeeded"
                    },
                    capped.text,
                ))
            }
            Err(error) if error.code == AppErrorCode::RunCancelled => Err(error),
            Err(error) => {
                context.emit(HostEventDraft::Log {
                    line: log_line(&[
                        ("level", json!("warn")),
                        ("message", json!(error.message)),
                        ("tool", json!(tool.key)),
                    ]),
                })?;
                let status = match error.code {
                    AppErrorCode::PermissionDenied => "refused",
                    AppErrorCode::ApprovalDenied => "denied",
                    _ => "failed",
                };
                Ok(shape(status, error.message))
            }
        }
    }

    fn bridge(
        &self,
        name: &str,
        arguments: &Map<String, Value>,
        progress: &mut Progress,
        guard: &mut LoopGuard,
        tripped: &mut bool,
        finished: &mut Option<Option<String>>,
    ) -> String {
        match name {
            PLAN_TOOL => {
                let titles: Vec<String> = arguments
                    .get("steps")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect();
                let allowed = guard.allowed(&titles);
                let mut proposed = progress.plan.clone();
                if propose(&mut proposed, &allowed) == 0 {
                    return PLAN_EMPTY.to_owned();
                }
                progress.plan = proposed;
                *tripped |= guard.revised();
                let mut text = format!("Plan updated:\n{}", describe(&progress.plan));
                if allowed.len() < titles.len() {
                    text.push_str(&format!("\n{REMOVED_DROPPED}."));
                }
                text
            }
            COMPLETE_TOOL => {
                match complete(&mut progress.plan, &text_argument(arguments, "summary")) {
                    Some(done) => {
                        guard.progressed();
                        format!(
                            "Step done: {done}\nCurrent plan:\n{}",
                            describe(&progress.plan)
                        )
                    }
                    None => NO_STEP_RUNNING.to_owned(),
                }
            }
            _ => {
                *finished = Some(summary(&text_argument(arguments, "summary")));
                "Session finished.".to_owned()
            }
        }
    }
}

#[async_trait]
impl NodeDef for SessionNode {
    type Input = SessionInput;
    type Output = SessionOutput;

    fn node_type(&self) -> &str {
        AGENTIC_SESSION
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool("llm.generate")
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(
        &self,
        context: &StepContext,
        input: SessionInput,
    ) -> Result<SessionOutput, AppError> {
        let id = input.session_id;
        let row = self.store.load(&id)?;
        if row.status != AgenticSessionStatus::Running
            || row.run_id.as_deref() != Some(context.run_id.to_string().as_str())
        {
            return Err(AppError::new(AppErrorCode::Conflict, NOT_THIS_RUN));
        }
        let Some(providers) = &context.services.providers else {
            return Err(AppError::new(AppErrorCode::Conflict, PROVIDERS_UNAVAILABLE));
        };
        let raw = providers.text(&row.provider_id)?;
        let tool_mode = resolve_tool_mode(raw.capabilities().tools, None);
        if tool_mode == ToolMode::None {
            return Err(AppError::new(
                AppErrorCode::ValidationFailed,
                NO_TOOL_SUPPORT,
            ));
        }
        let native = tool_mode == ToolMode::Native;
        let driver = if tool_mode == ToolMode::Injected {
            with_injected_tools(raw, None)
        } else {
            raw
        };
        let started = self.store.now();
        let base_elapsed = row.usage.elapsed_ms;
        let mut progress = Progress {
            plan: row.plan.clone(),
            usage: row.usage,
            changed: row.changed_files.clone(),
            messages: row.transcript.iter().filter_map(from_entry).collect(),
            mode: row.mode,
        };
        if progress.messages.is_empty() {
            progress
                .messages
                .push(ChatMessage::user(goal_message(&row)));
        }
        let mut guard = LoopGuard::default();
        loop {
            context.check_cancelled()?;
            self.boundary(context, &id, &mut progress, &mut guard)?;
            let elapsed = self.store.now().saturating_sub(started);
            progress.usage.elapsed_ms = base_elapsed + u64::try_from(elapsed).unwrap_or(0);
            if let Some(reason) = budget_exceeded(&row.budget, &progress.usage) {
                let ending = Ending::new(AgenticSessionStatus::Paused, reason, None);
                self.save(context, &id, &progress, Some(&ending))?;
                return Ok(output(&ending));
            }
            let offered = self.offered(&row.tools, progress.mode)?;
            let by_name: HashMap<&str, &OfferedTool> = offered
                .iter()
                .map(|tool| (tool.name.as_str(), tool))
                .collect();
            let mut specs = tool_specs(&offered);
            specs.extend(bridge_specs());
            let request = GenerateRequest {
                system: Some(system_prompt(&offered, progress.mode)),
                messages: progress.messages.clone(),
                tools: specs,
                ..GenerateRequest::new(&row.model_id, Vec::new())
            };
            let round = GenerateChatNode::generate_round(context, &driver, &request, "").await?;
            progress.usage.steps += 1;
            progress.usage.tokens += request_tokens(&request) + round_tokens(&round);
            progress.messages.push(
                ChatMessage::assistant(round.text.clone())
                    .with_tool_calls(round.tool_calls.clone()),
            );
            if round.tool_calls.is_empty() {
                finish(&mut progress.plan);
                let ending = Ending::new(
                    AgenticSessionStatus::Completed,
                    AgenticStopReason::Goal,
                    summary(&round.text),
                );
                self.save(context, &id, &progress, Some(&ending))?;
                return Ok(output(&ending));
            }
            let watched: Vec<(String, String)> = round
                .tool_calls
                .iter()
                .filter(|call| call.name != COMPLETE_TOOL && call.name != FINISH_TOOL)
                .map(|call| (call.name.clone(), call.arguments.clone()))
                .collect();
            let mut tripped = !watched.is_empty() && guard.round(signature(&watched));
            let mut finished: Option<Option<String>> = None;
            for call in &round.tool_calls {
                let arguments = parse_tool_arguments(&call.arguments).unwrap_or_default();
                let content = match call.name.as_str() {
                    PLAN_TOOL | COMPLETE_TOOL | FINISH_TOOL => {
                        let text = self.bridge(
                            &call.name,
                            &arguments,
                            &mut progress,
                            &mut guard,
                            &mut tripped,
                            &mut finished,
                        );
                        if native {
                            fence_tool_result(&call.name, "succeeded", &text)
                        } else {
                            text
                        }
                    }
                    name => match by_name.get(name) {
                        Some(tool) => {
                            self.call_tool(context, &id, tool, call, &mut progress, native)
                                .await?
                        }
                        None if native => fence_tool_result(name, "refused", TOOL_NOT_OFFERED),
                        None => TOOL_NOT_OFFERED.to_owned(),
                    },
                };
                progress
                    .messages
                    .push(ChatMessage::tool(call.id.clone(), content));
            }
            if let Some(summary) = finished {
                finish(&mut progress.plan);
                let ending = Ending::new(
                    AgenticSessionStatus::Completed,
                    AgenticStopReason::Goal,
                    summary,
                );
                self.save(context, &id, &progress, Some(&ending))?;
                return Ok(output(&ending));
            }
            if tripped {
                let ending =
                    Ending::new(AgenticSessionStatus::Stopped, AgenticStopReason::Loop, None);
                self.save(context, &id, &progress, Some(&ending))?;
                return Ok(output(&ending));
            }
            self.save(context, &id, &progress, None)?;
        }
    }
}

fn output(ending: &Ending) -> SessionOutput {
    SessionOutput {
        status: ending.status.as_str().to_owned(),
        stop_reason: ending.reason.as_str().to_owned(),
        summary: ending.summary.clone(),
    }
}
