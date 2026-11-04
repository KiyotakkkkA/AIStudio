use serde_json::Value;
use zvs_contract::{
    APPS_APPROVAL_KIND, ApprovalRequestDto, ChatToolCallDto, ChatToolCallStatus, ComputerCall,
    ComputerRequest, StepEventDto,
};

use crate::commands::SHELL_RUN;

pub const REPLY_KEPT: usize = 2000;
pub const CALLS_KEPT: usize = 30;
pub const DETAIL_KEPT: usize = 1200;
const TOOL_CALL_DOMAIN: &str = "tool-call";
const COMPUTER_PREFIX: &str = "computer.";

pub fn keep_tail(text: &mut String, kept: usize) {
    let count = text.chars().count();
    if count > kept {
        let cut = text
            .char_indices()
            .nth(count - kept)
            .map_or(0, |(index, _)| index);
        text.drain(..cut);
    }
}

fn keep_head(text: &str, kept: usize) -> String {
    if text.chars().count() <= kept {
        return text.to_owned();
    }
    let mut head: String = text.chars().take(kept).collect();
    head.push('…');
    head
}

pub fn tool_call(step: &StepEventDto) -> Option<ChatToolCallDto> {
    if step.get("domain").and_then(Value::as_str) != Some(TOOL_CALL_DOMAIN) {
        return None;
    }
    serde_json::from_value(step.get("call")?.clone()).ok()
}

fn text_field<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

pub fn call_entry(call: &ChatToolCallDto) -> ComputerCall {
    let summary = (call.key == SHELL_RUN)
        .then(|| text_field(&call.arguments, "/reason").map(|reason| keep_head(reason, 200)))
        .flatten();
    ComputerCall {
        id: call.id.clone(),
        key: call.key.clone(),
        status: call.status,
        summary,
    }
}

pub fn merge_call(calls: &mut Vec<ComputerCall>, call: ComputerCall) {
    match calls.iter_mut().find(|entry| entry.id == call.id) {
        Some(entry) => *entry = call,
        None => calls.push(call),
    }
    if calls.len() > CALLS_KEPT {
        let extra = calls.len() - CALLS_KEPT;
        calls.drain(..extra);
    }
}

pub fn settled(call: &ChatToolCallDto) -> Option<&str> {
    match call.status {
        ChatToolCallStatus::AwaitingApproval => None,
        _ => call.approval_id.as_deref(),
    }
}

fn quoted(argument: &str) -> String {
    if !argument.is_empty() && !argument.contains([' ', '\t', '"']) {
        return argument.to_owned();
    }
    format!("\"{}\"", argument.replace('"', "\\\""))
}

fn joined(value: &Value, list: &str, field: &str) -> Option<String> {
    let lines: Vec<&str> = value
        .get(list)?
        .as_array()?
        .iter()
        .filter_map(|entry| entry.get(field).and_then(Value::as_str))
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn process_detail(process: &Value) -> Option<String> {
    if let Some(source) = text_field(process, "/source") {
        return Some(source.to_owned());
    }
    let argv: Vec<String> = process
        .get("argv")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(quoted)
        .collect();
    (!argv.is_empty()).then(|| argv.join(" "))
}

pub struct PendingRequest {
    pub request: ComputerRequest,
    pub run: String,
}

pub fn pending_request(request: &ApprovalRequestDto) -> Option<PendingRequest> {
    let value = Value::Object(request.clone());
    let id = text_field(&value, "/id")?.to_owned();
    let run = text_field(&value, "/runId")?.to_owned();
    let kind = text_field(&value, "/kind").unwrap_or("approval").to_owned();
    let tool = text_field(&value, "/tool")
        .or_else(|| text_field(&value, "/subject"))?
        .to_owned();
    if kind == APPS_APPROVAL_KIND || tool.starts_with(COMPUTER_PREFIX) {
        return None;
    }
    let detail = match kind.as_str() {
        "process" => value.get("process").and_then(process_detail),
        "files" => joined(&value, "changes", "path"),
        "folder" => joined(&value, "folders", "path"),
        _ => text_field(&value, "/detail").map(str::to_owned),
    };
    Some(PendingRequest {
        request: ComputerRequest {
            id,
            kind,
            action: text_field(&value, "/action").unwrap_or_default().to_owned(),
            tool,
            reason: text_field(&value, "/process/reason").map(str::to_owned),
            detail: detail.map(|detail| keep_head(&detail, DETAIL_KEPT)),
        },
        run,
    })
}

#[cfg(test)]
mod tests;
