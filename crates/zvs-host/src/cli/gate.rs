use serde_json::Value;
use zvs_contract::{
    AllowRuleScope, AppError, AppErrorCode, BackgroundShellId, CLI_CONFIRM_SETTING_KEY,
    CliChangeDto, ConversationId, ExecMode, ExecPolicyDto, FolderAccessAnswer, RunId, StreamId,
};

use crate::ipc::{ChannelPolicy, Effect, Scope};
use crate::services::Services;

pub const CLI_DIALOG_REFUSED: &str = "Диалог не был открыт в этой консольной сессии";
const OPEN_DIALOG: &str = "chat.conversations.get";
const CREATE_DIALOG: &str = "chat.conversations.create";

fn refused() -> AppError {
    AppError::new(AppErrorCode::PermissionDenied, CLI_DIALOG_REFUSED)
}

fn text<'a>(payload: &'a Value, field: &str) -> Option<&'a str> {
    payload.get(field).and_then(Value::as_str)
}

const fn rank(mode: ExecMode) -> u8 {
    match mode {
        ExecMode::AlwaysAsk => 0,
        ExecMode::SafetyAsk => 1,
        ExecMode::AutoBypass => 2,
        ExecMode::NoRestrictions => 3,
    }
}

pub fn confirms_every_loosening(services: &Services) -> bool {
    services
        .settings
        .get(CLI_CONFIRM_SETTING_KEY)
        .ok()
        .flatten()
        .and_then(|setting| setting.value.as_bool())
        .unwrap_or(true)
}

pub fn policy_change(
    current: &ExecPolicyDto,
    next: &ExecPolicyDto,
    strict: bool,
) -> Option<CliChangeDto> {
    if rank(next.mode) > rank(current.mode) {
        if strict || next.mode == ExecMode::NoRestrictions {
            return Some(CliChangeDto::Mode {
                from: current.mode,
                to: next.mode,
            });
        }
        return None;
    }
    if strict && next.judge != current.judge {
        return Some(CliChangeDto::Judge);
    }
    None
}

fn run_dialog(services: &Services, id: &str) -> Result<Option<String>, AppError> {
    let Some(run) = RunId::parse(id) else {
        return Ok(None);
    };
    let run = services.runs.get(&run).map_err(|_| refused())?;
    run.subject_id.map(Some).ok_or_else(refused)
}

fn shell_dialog(services: &Services, id: &str) -> Result<Option<String>, AppError> {
    let Some(shell) = BackgroundShellId::parse(id) else {
        return Ok(None);
    };
    let shell = services.shells.get(None, &shell).map_err(|_| refused())?;
    Ok(Some(shell.owner_id))
}

fn dialog(services: &Services, scope: Scope, payload: &Value) -> Result<Option<String>, AppError> {
    match scope {
        Scope::Any => Ok(None),
        Scope::Dialog(field) | Scope::Owner(field) => Ok(text(payload, field)
            .and_then(ConversationId::parse)
            .map(|id| id.to_string())),
        Scope::Run(field) => text(payload, field).map_or(Ok(None), |id| run_dialog(services, id)),
        Scope::Shell(field) => {
            text(payload, field).map_or(Ok(None), |id| shell_dialog(services, id))
        }
    }
}

fn loosening(
    services: &Services,
    channel: &str,
    payload: &Value,
) -> Result<Option<CliChangeDto>, AppError> {
    match channel {
        "execPolicy.set" => {
            let Ok(next) = serde_json::from_value::<ExecPolicyDto>(payload.clone()) else {
                return Ok(None);
            };
            let current = services.exec_policy.get()?;
            Ok(policy_change(
                &current,
                &next,
                confirms_every_loosening(services),
            ))
        }
        "execPolicy.allowRule" => Ok(payload
            .get("scope")
            .cloned()
            .and_then(|scope| serde_json::from_value::<AllowRuleScope>(scope).ok())
            .map(|scope| CliChangeDto::AllowRule { scope })),
        "runs.approve" => Ok(payload
            .get("always")
            .and_then(Value::as_bool)
            .filter(|always| *always)
            .map(|_| CliChangeDto::AlwaysApproval)),
        "folders.answer" => Ok(payload
            .get("answer")
            .cloned()
            .and_then(|answer| serde_json::from_value::<FolderAccessAnswer>(answer).ok())
            .filter(|answer| *answer == FolderAccessAnswer::Always)
            .map(|_| CliChangeDto::AlwaysFolder)),
        _ => Ok(None),
    }
}

async fn open_dialog(services: &Services, connection: u64, dialog: &str) -> Result<(), AppError> {
    let access = &services.cli_connections;
    if !access.released(dialog) {
        let Some(id) = ConversationId::parse(dialog) else {
            return Ok(());
        };
        let Ok(detail) = services.chat.get(&id) else {
            return Ok(());
        };
        access
            .confirm(
                connection,
                CliChangeDto::OpenDialog {
                    conversation_id: id,
                    title: detail.conversation.title,
                },
            )
            .await?;
        access.release(dialog);
    }
    access.adopt(connection, dialog);
    Ok(())
}

pub async fn admit(
    services: &Services,
    connection: u64,
    row: &ChannelPolicy,
    payload: &Value,
) -> Result<(), AppError> {
    let access = &services.cli_connections;
    access.ensure_open(connection)?;
    if let Some(dialog) = dialog(services, row.scope, payload)?
        && !access.owns(connection, &dialog)
    {
        if row.channel != OPEN_DIALOG {
            return Err(refused());
        }
        open_dialog(services, connection, &dialog).await?;
    }
    if row.effect == Effect::Loosen
        && let Some(change) = loosening(services, row.channel, payload)?
    {
        access.confirm(connection, change).await?;
    }
    Ok(())
}

pub fn settle(services: &Services, connection: u64, channel: &str, envelope: &Value) {
    if channel != CREATE_DIALOG {
        return;
    }
    if let Some(id) = envelope
        .get("data")
        .and_then(|data| data.get("id"))
        .and_then(Value::as_str)
        .and_then(ConversationId::parse)
    {
        let id = id.to_string();
        services.cli_connections.release(&id);
        services.cli_connections.adopt(connection, &id);
    }
}

pub fn sees(services: &Services, connection: u64, stream: StreamId) -> bool {
    services.cli_connections.sees(connection, stream, || {
        services.runs.subject_of_stream(&stream)
    })
}

#[cfg(test)]
mod tests;
