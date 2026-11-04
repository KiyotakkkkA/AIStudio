use serde_json::Value;
use zvs_contract::{
    ApprovalRequestDto, ChatToolCallDto, ChatToolCallStatus, ComputerAction, ComputerActionState,
    ComputerFeed, ComputerStatus, DeltaKind, HostEventDraft, MessageDto, StepEventDto,
};

use crate::services::chat_reply::{append_round, apply_tool_call};

const TOOL_CALL_DOMAIN: &str = "tool-call";
const PERSIST_NODE: &str = "persist";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Absorbed {
    Nothing,
    Token,
    Changed,
}

pub fn tool_call(step: &StepEventDto) -> Option<(usize, ChatToolCallDto)> {
    if step.get("domain").and_then(Value::as_str) != Some(TOOL_CALL_DOMAIN) {
        return None;
    }
    let index = step
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())?;
    let call = serde_json::from_value(step.get("call")?.clone()).ok()?;
    Some((index, call))
}

fn persisted(step: &StepEventDto) -> Option<MessageDto> {
    if step.get("nodeId").and_then(Value::as_str) != Some(PERSIST_NODE)
        || step.get("status").and_then(Value::as_str) != Some("succeeded")
    {
        return None;
    }
    serde_json::from_value(step.get("output")?.clone()).ok()
}

pub fn settled(call: &ChatToolCallDto) -> Option<&str> {
    match call.status {
        ChatToolCallStatus::AwaitingApproval => None,
        _ => call.approval_id.as_deref(),
    }
}

pub fn approval_id(request: &ApprovalRequestDto) -> Option<&str> {
    request.get("id").and_then(Value::as_str)
}

pub fn approval_run(request: &ApprovalRequestDto) -> Option<&str> {
    request.get("runId").and_then(Value::as_str)
}

pub fn approval_kind(request: &ApprovalRequestDto) -> Option<&str> {
    request.get("kind").and_then(Value::as_str)
}

pub fn absorb(feed: &mut ComputerFeed, body: &HostEventDraft) -> Absorbed {
    match body {
        HostEventDraft::Token { delta, kind } => {
            if *kind == Some(DeltaKind::Reasoning) {
                feed.reasoning.push_str(delta);
            } else {
                feed.content.push_str(delta);
            }
            append_round(&mut feed.rounds, &feed.tool_calls, *kind, delta);
            Absorbed::Token
        }
        HostEventDraft::Step { step } => {
            if let Some((index, call)) = tool_call(step) {
                if let Some(approval) = settled(&call).map(str::to_owned) {
                    feed.approvals
                        .retain(|request| approval_id(request) != Some(approval.as_str()));
                }
                apply_tool_call(&mut feed.tool_calls, index, call);
                return Absorbed::Changed;
            }
            match persisted(step) {
                Some(message) => {
                    feed.content = message.content;
                    feed.reasoning = message.reasoning;
                    Absorbed::Changed
                }
                None => Absorbed::Nothing,
            }
        }
        HostEventDraft::Approval { request } => {
            let Some(id) = approval_id(request) else {
                return Absorbed::Nothing;
            };
            if feed
                .approvals
                .iter()
                .any(|known| approval_id(known) == Some(id))
            {
                return Absorbed::Nothing;
            }
            feed.approvals.push(request.clone());
            Absorbed::Changed
        }
        _ => Absorbed::Nothing,
    }
}

pub fn derived_status(
    base: ComputerStatus,
    feed: &ComputerFeed,
    actions: &[ComputerAction],
    approval_open: bool,
) -> ComputerStatus {
    if matches!(base, ComputerStatus::Idle | ComputerStatus::Stopping) {
        return base;
    }
    let waiting = approval_open
        || !feed.approvals.is_empty()
        || feed.tool_calls.iter().any(|call| {
            matches!(
                call.status,
                ChatToolCallStatus::AwaitingApproval | ChatToolCallStatus::AwaitingAnswer
            )
        })
        || actions
            .iter()
            .any(|action| action.state == ComputerActionState::Waiting);
    if waiting {
        return ComputerStatus::Awaiting;
    }
    let running = feed
        .tool_calls
        .iter()
        .any(|call| call.status == ChatToolCallStatus::Running)
        || actions
            .iter()
            .any(|action| action.state == ComputerActionState::Running);
    if running {
        ComputerStatus::Working
    } else {
        ComputerStatus::Starting
    }
}

#[cfg(test)]
mod tests;
