use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use futures::future::join_all;
use serde_json::{Value, json};
use zvs_contract::{
    AppError, ChatRoundDto, ChatToolCallDto, ChatToolCallStatus, HostEventDraft, OfferedToolSource,
    WebSourceDto, log_line,
};

use super::chat::subagent::concurrent_delegate;
use super::chat::{BUDGET_EXHAUSTED, GenerateChatNode, KNOWLEDGE_COMPACTED, RoundOutput};
use super::chat_ask::{ASK_ONCE, ASK_TOOL};
use super::chat_history::{MESSAGE_OVERHEAD, tokens};
use super::chat_tools::{
    OfferedTool, TOOL_BUDGET_NOTE, fence_tool_result, parse_tool_arguments, tool_specs,
    tool_specs_json,
};
use super::token_estimate::estimate_tokens;
use crate::drivers::ai::ports::SharedTextDriver;
use crate::drivers::ai::{ChatMessage, ChatRole, GenerateRequest, ToolCall, TurnUsage};
use crate::kernel::StepContext;

pub const RESULTS_SHED: &str = "Older tool results dropped to fit the context";
pub const SHED_RESULT: &str = "[This older tool result was dropped to fit the context window. Call the tool again if you still need it.]";

pub(crate) struct PendingCall<'a> {
    pub(crate) call: ToolCall,
    pub(crate) tool: Option<&'a OfferedTool>,
    pub(crate) index: usize,
    pub(crate) refuse: Option<&'static str>,
}

pub(crate) struct CallOutcome {
    pub(crate) content: String,
    pub(crate) compact: Option<String>,
    pub(crate) refund: bool,
    pub(crate) usage: Option<TurnUsage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoopBudget {
    Calls(usize),
    Open,
}

impl LoopBudget {
    const fn spent(self, round: usize, executed: usize) -> bool {
        match self {
            Self::Calls(budget) => round >= budget || executed >= budget,
            Self::Open => false,
        }
    }

    const fn refuses(self, executed: usize) -> bool {
        match self {
            Self::Calls(budget) => executed >= budget,
            Self::Open => false,
        }
    }
}

pub(crate) struct RoundReport<'a> {
    pub(crate) called: bool,
    pub(crate) last: bool,
    pub(crate) tool_calls: &'a [ToolCall],
    pub(crate) tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RoundNext {
    Continue,
    Stop,
    Nudge(String),
    Wrap,
}

pub(crate) struct BridgeOutcome {
    pub(crate) status: ChatToolCallStatus,
    pub(crate) error: Option<String>,
    pub(crate) result: Option<String>,
}

#[async_trait]
pub(crate) trait RoundHook: Send + Sync {
    fn handles(&self, _key: &str) -> bool {
        false
    }

    fn bridge(&self, _key: &str, _arguments: &Value) -> BridgeOutcome {
        BridgeOutcome {
            status: ChatToolCallStatus::Refused,
            error: None,
            result: None,
        }
    }

    async fn after_round(
        &self,
        _context: &StepContext,
        report: &RoundReport<'_>,
    ) -> Result<RoundNext, AppError> {
        Ok(if !report.called || report.last {
            RoundNext::Stop
        } else {
            RoundNext::Continue
        })
    }
}

pub(crate) struct ChatTurn;

impl RoundHook for ChatTurn {}

pub(crate) struct LoopCall<'a> {
    pub(crate) index: usize,
    pub(crate) tool: Option<&'a OfferedTool>,
    pub(crate) refuse: Option<&'static str>,
    pub(crate) label: String,
    pub(crate) offered: &'a [OfferedTool],
    pub(crate) deferred: &'a [OfferedTool],
    pub(crate) loading: &'a Mutex<Vec<String>>,
    pub(crate) hook: &'a dyn RoundHook,
}

#[async_trait]
pub(crate) trait LoopHost: Send + Sync {
    async fn call(
        &self,
        context: &StepContext,
        calls: &Arc<Mutex<Vec<ChatToolCallDto>>>,
        call: LoopCall<'_>,
    ) -> Result<CallOutcome, AppError>;

    fn rounds_saved(&self, context: &StepContext, rounds: &[ChatRoundDto], usage: &TurnUsage);

    fn offered_saved(&self, context: &StepContext, offered: &[OfferedTool]);
}

#[derive(Debug, Default)]
pub(crate) struct LoopTranscript {
    pub(crate) text: String,
    pub(crate) reasoning: String,
    pub(crate) rounds: Vec<ChatRoundDto>,
    pub(crate) web_sources: Vec<WebSourceDto>,
    pub(crate) usage: TurnUsage,
}

pub(crate) struct ToolLoop<'a> {
    pub(crate) driver: SharedTextDriver,
    pub(crate) template: GenerateRequest,
    pub(crate) offered: Vec<OfferedTool>,
    pub(crate) deferred: Vec<OfferedTool>,
    pub(crate) budget: LoopBudget,
    pub(crate) context_budget: i64,
    pub(crate) host: &'a dyn LoopHost,
    pub(crate) hook: &'a dyn RoundHook,
}

pub(crate) struct Compactable {
    pub(crate) index: usize,
    pub(crate) round: usize,
    pub(crate) compact: String,
}

fn messages_cost(messages: &[ChatMessage]) -> i64 {
    messages
        .iter()
        .map(|message| tokens(&message.content) + MESSAGE_OVERHEAD)
        .sum()
}

pub(crate) fn compact_knowledge(
    messages: &mut [ChatMessage],
    pending: &mut Vec<Compactable>,
    round: usize,
    budget: i64,
) -> usize {
    let mut compacted = 0;
    while messages_cost(messages) > budget {
        let Some(position) = pending.iter().position(|entry| entry.round < round) else {
            break;
        };
        let entry = pending.remove(position);
        if let Some(message) = messages.get_mut(entry.index) {
            message.content = entry.compact;
            compacted += 1;
        }
    }
    compacted
}

pub(crate) fn shed_tool_results(messages: &mut [ChatMessage], budget: i64) -> usize {
    let mut shed = 0;
    for index in 0..messages.len() {
        if messages_cost(messages) <= budget {
            break;
        }
        let message = &mut messages[index];
        if message.role == ChatRole::Tool && message.content != SHED_RESULT {
            message.content = SHED_RESULT.to_owned();
            shed += 1;
        }
    }
    shed
}

pub(crate) fn request_tokens(request: &GenerateRequest) -> u64 {
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

pub(crate) fn round_tokens(round: &RoundOutput) -> u64 {
    estimate_tokens(&round.text)
        + estimate_tokens(&round.reasoning)
        + round
            .tool_calls
            .iter()
            .map(|call| estimate_tokens(&call.name) + estimate_tokens(&call.arguments))
            .sum::<u64>()
}

fn spent_tokens(request: &GenerateRequest, round: &RoundOutput) -> u64 {
    match &round.usage {
        Some(usage) if usage.counted() => {
            let count = |value: Option<i64>| u64::try_from(value.unwrap_or(0)).unwrap_or(0);
            count(usage.input_tokens) + count(usage.output_tokens)
        }
        _ => request_tokens(request) + round_tokens(round),
    }
}

fn record(call: &ToolCall, tool: Option<&OfferedTool>, round: usize) -> ChatToolCallDto {
    ChatToolCallDto {
        id: call.id.clone(),
        key: tool.map_or_else(|| call.name.clone(), |tool| tool.key.clone()),
        name: call.name.clone(),
        round: round as u64,
        arguments: parse_tool_arguments(&call.arguments)
            .map_or_else(|| Value::String(call.arguments.clone()), Value::Object),
        status: ChatToolCallStatus::Running,
        result: None,
        truncated: false,
        error: None,
        approval_id: None,
        step_id: None,
        duration_ms: 0,
        meta: None,
        authorisation: None,
    }
}

impl ToolLoop<'_> {
    pub(crate) async fn run(
        self,
        context: &StepContext,
        calls: &Arc<Mutex<Vec<ChatToolCallDto>>>,
        out: &mut LoopTranscript,
    ) -> Result<(), AppError> {
        let Self {
            driver,
            mut template,
            mut offered,
            deferred,
            budget,
            context_budget,
            host,
            hook,
        } = self;
        let mut specs = tool_specs(&offered);
        let mut messages = template.messages.clone();
        let mut compactable: Vec<Compactable> = Vec::new();
        let loading: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let mut executed = 0;
        let mut round = 0;
        let mut wrapped = false;
        loop {
            let last = wrapped || budget.spent(round, executed);
            if last && !specs.is_empty() {
                messages.push(ChatMessage::user(TOOL_BUDGET_NOTE));
            }
            let overhead = tokens(template.system.as_deref().unwrap_or_default())
                + tokens(&tool_specs_json(&offered));
            let room = context_budget.saturating_sub(overhead);
            let compacted = compact_knowledge(&mut messages, &mut compactable, round, room);
            if compacted > 0 {
                context.emit(HostEventDraft::Log {
                    line: log_line(&[
                        ("message", json!(KNOWLEDGE_COMPACTED)),
                        ("compacted", json!(compacted)),
                        ("round", json!(round)),
                    ]),
                })?;
            }
            if budget == LoopBudget::Open {
                let shed = shed_tool_results(&mut messages, room);
                if shed > 0 {
                    context.emit(HostEventDraft::Log {
                        line: log_line(&[
                            ("message", json!(RESULTS_SHED)),
                            ("shed", json!(shed)),
                            ("round", json!(round)),
                        ]),
                    })?;
                }
            }
            let request = GenerateRequest {
                messages: messages.clone(),
                ..template.clone()
            };
            let output =
                GenerateChatNode::generate_round(context, &driver, &request, &out.text).await?;
            let spent = spent_tokens(&request, &output);
            out.text.push_str(&output.text);
            out.reasoning.push_str(&output.reasoning);
            out.web_sources.extend(output.web_sources.iter().cloned());
            out.rounds.push(ChatRoundDto {
                text: output.text.clone(),
                reasoning: output.reasoning.clone(),
            });
            if let Some(reported) = output.usage.clone() {
                out.usage.merge(reported);
            }
            host.rounds_saved(context, &out.rounds, &out.usage);
            let called = !output.tool_calls.is_empty() && !specs.is_empty();
            let round_calls = output.tool_calls.clone();
            if called {
                messages.push(
                    ChatMessage::assistant(output.text.clone())
                        .with_tool_calls(output.tool_calls.clone()),
                );
                let mut by_name: HashMap<&str, &OfferedTool> = deferred
                    .iter()
                    .map(|tool| (tool.name.as_str(), tool))
                    .collect();
                by_name.extend(offered.iter().map(|tool| (tool.name.as_str(), tool)));
                let mut pending = Vec::with_capacity(output.tool_calls.len());
                let mut asked = false;
                for call in output.tool_calls {
                    let tool = by_name.get(call.name.as_str()).copied();
                    if let Some(tool) = tool
                        && !offered.iter().any(|known| known.key == tool.key)
                    {
                        loading
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .push(tool.key.clone());
                    }
                    let index = {
                        let mut guard = calls.lock().unwrap_or_else(PoisonError::into_inner);
                        guard.push(record(&call, tool, round));
                        guard.len() - 1
                    };
                    let asking = tool.is_some_and(|tool| tool.key == ASK_TOOL);
                    let refuse = if last || budget.refuses(executed) {
                        Some(BUDGET_EXHAUSTED)
                    } else if asking && asked {
                        Some(ASK_ONCE)
                    } else {
                        None
                    };
                    asked |= asking;
                    if refuse.is_none() && !asking {
                        executed += 1;
                    }
                    pending.push(PendingCall {
                        call,
                        tool,
                        index,
                        refuse,
                    });
                }
                let concurrent = |entry: &PendingCall<'_>| {
                    entry.refuse.is_none()
                        && entry.tool.is_some_and(|tool| {
                            tool.read_only || concurrent_delegate(tool, &entry.call.arguments)
                        })
                };
                let mut position = 0;
                while position < pending.len() {
                    let mut end = position + 1;
                    if concurrent(&pending[position]) {
                        while end < pending.len() && concurrent(&pending[end]) {
                            end += 1;
                        }
                    }
                    let group = &pending[position..end];
                    let outcomes = join_all(group.iter().map(|entry| {
                        host.call(
                            context,
                            calls,
                            LoopCall {
                                index: entry.index,
                                tool: entry.tool,
                                refuse: entry.refuse,
                                label: format!("call{}", entry.index + 1),
                                offered: &offered,
                                deferred: &deferred,
                                loading: &loading,
                                hook,
                            },
                        )
                    }))
                    .await;
                    for (entry, outcome) in group.iter().zip(outcomes) {
                        let outcome = outcome?;
                        if outcome.refund {
                            executed -= 1;
                        }
                        if let Some(usage) = outcome.usage {
                            out.usage.merge(usage);
                        }
                        let status = calls.lock().unwrap_or_else(PoisonError::into_inner)
                            [entry.index]
                            .status;
                        let shape = |content: String| {
                            fence_tool_result(&entry.call.name, status.as_str(), &content)
                        };
                        if let Some(compact) = outcome.compact {
                            compactable.push(Compactable {
                                index: messages.len(),
                                round,
                                compact: shape(compact),
                            });
                        }
                        messages.push(ChatMessage::tool(
                            entry.call.id.clone(),
                            shape(outcome.content),
                        ));
                    }
                    position = end;
                }
                let loaded =
                    std::mem::take(&mut *loading.lock().unwrap_or_else(PoisonError::into_inner));
                let mut grew = false;
                for key in loaded {
                    if offered.iter().any(|tool| tool.key == key) {
                        continue;
                    }
                    if let Some(tool) = deferred.iter().find(|tool| tool.key == key) {
                        offered.push(OfferedTool {
                            source: OfferedToolSource::Loaded,
                            ..tool.clone()
                        });
                        grew = true;
                    }
                }
                if grew {
                    specs = tool_specs(&offered);
                    template.tools = specs.clone();
                    host.offered_saved(context, &offered);
                }
            }
            let report = RoundReport {
                called,
                last,
                tool_calls: &round_calls,
                tokens: spent,
            };
            match hook.after_round(context, &report).await? {
                RoundNext::Stop => break,
                RoundNext::Continue => {}
                RoundNext::Wrap => wrapped = true,
                RoundNext::Nudge(note) => {
                    if !called {
                        messages.push(ChatMessage::assistant(output.text));
                    }
                    messages.push(ChatMessage::user(note));
                }
            }
            round += 1;
        }
        Ok(())
    }
}
