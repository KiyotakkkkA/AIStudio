use serde_json::{Value, json};
use zvs_contract::{
    APPS_APPROVAL_KIND, AppError, AppErrorCode, ApprovalRequestDto, ComputerAppDto,
    FILES_APPROVAL_KIND, FOLDER_APPROVAL_KIND, FolderRequestEntryDto, PROCESS_APPROVAL_KIND,
};

use super::request::{AccessRequest, Asking, Effect, Resource};
use crate::data::schema::ApprovalDecision;
use crate::kernel::StepContext;
use crate::services::approvals::{ApprovalService, PendingReview};
use crate::services::permissions::GLOBAL_SCOPE;

pub const APPROVAL_DENIED: &str = "Approval denied";
pub const FOLDER_SUBJECT: &str = "folder.access";

const RESERVED: &[&str] = &[
    "id",
    "runId",
    "subject",
    "scope",
    "expiresAt",
    "kind",
    "tool",
    "action",
];

fn tool_scope(request: &AccessRequest) -> String {
    if let Some(scope) = &request.prompt.scope {
        return scope.clone();
    }
    request
        .actor
        .scopes
        .first()
        .cloned()
        .unwrap_or_else(|| GLOBAL_SCOPE.to_owned())
}

fn kind_of(request: &AccessRequest) -> Option<String> {
    if let Some(kind) = &request.prompt.kind {
        return Some(kind.clone());
    }
    match &request.effect {
        Effect::Process { .. } => Some(PROCESS_APPROVAL_KIND.to_owned()),
        Effect::Files { .. } => Some(FILES_APPROVAL_KIND.to_owned()),
        _ => None,
    }
}

fn insert(approval: &mut ApprovalRequestDto, key: &str, value: Value) {
    approval.insert(key.to_owned(), value);
}

fn insert_fields(approval: &mut ApprovalRequestDto, request: &AccessRequest) {
    for (key, value) in &request.prompt.fields {
        if !RESERVED.contains(&key.as_str()) {
            approval.insert(key.clone(), value.clone());
        }
    }
}

fn folders(request: &AccessRequest) -> Vec<FolderRequestEntryDto> {
    request
        .resources
        .iter()
        .filter_map(|resource| match resource {
            Resource::Folder { path, creates } => Some(FolderRequestEntryDto {
                path: path.clone(),
                creates: creates.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn apps(request: &AccessRequest) -> Vec<ComputerAppDto> {
    request
        .resources
        .iter()
        .filter_map(|resource| match resource {
            Resource::App { exe, path, icon } => Some(ComputerAppDto {
                app: exe.clone(),
                path: path.clone(),
                icon: icon.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn changes(request: &AccessRequest) -> Option<Value> {
    match &request.effect {
        Effect::Files { changes, .. } if !changes.is_empty() => Some(json!(changes)),
        _ => None,
    }
}

pub fn tool_approval(
    approvals: &ApprovalService,
    request: &AccessRequest,
) -> Result<ApprovalRequestDto, AppError> {
    let mut approval =
        approvals.create(&request.actor.run_id, &request.tool, &tool_scope(request))?;
    if let Some(kind) = kind_of(request) {
        insert(&mut approval, "kind", json!(kind));
    }
    if let Some(conversation) = &request.prompt.conversation {
        insert(&mut approval, "conversationId", json!(conversation));
    }
    if let Some(action) = &request.prompt.action {
        insert(&mut approval, "tool", json!(request.tool));
        insert(&mut approval, "action", json!(action));
    }
    if matches!(
        request.effect,
        Effect::Process { .. } | Effect::Files { .. }
    ) && let Some(id) = approval.get("id").and_then(Value::as_str)
    {
        approvals.remember(
            id,
            PendingReview {
                run_id: request.actor.run_id.clone(),
                action: request.effect.reviewed(),
                conversation: request.prompt.conversation.clone(),
            },
        );
    }
    match &request.effect {
        Effect::Process { process, .. } => insert(&mut approval, "process", json!(process)),
        Effect::Files { changes, .. } => insert(&mut approval, "changes", json!(changes)),
        Effect::Desktop { apps, .. } => insert(&mut approval, "apps", json!(apps)),
        _ => {}
    }
    for resource in &request.resources {
        if let Resource::Site(origin) = resource {
            insert(&mut approval, "origin", json!(origin));
        }
    }
    insert_fields(&mut approval, request);
    Ok(approval)
}

pub fn grant_approval(
    approvals: &ApprovalService,
    request: &AccessRequest,
    merged: bool,
) -> Result<ApprovalRequestDto, AppError> {
    let entries = folders(request);
    if entries.is_empty() {
        let grant = request.prompt.grant.clone().unwrap_or_default();
        let tool = grant.tool.unwrap_or_else(|| request.tool.clone());
        let scope = grant.scope.unwrap_or_else(|| GLOBAL_SCOPE.to_owned());
        let mut approval = approvals.create(&request.actor.run_id, &tool, &scope)?;
        insert(&mut approval, "kind", json!(APPS_APPROVAL_KIND));
        insert(&mut approval, "tool", json!(tool));
        if let Some(action) = grant.action.or_else(|| request.prompt.action.clone()) {
            insert(&mut approval, "action", json!(action));
        }
        insert(
            &mut approval,
            "conversationId",
            json!(request.prompt.conversation.clone().unwrap_or_default()),
        );
        insert(&mut approval, "apps", json!(apps(request)));
        if let Some(detail) = grant.detail {
            insert(&mut approval, "detail", json!(detail));
        }
        return Ok(approval);
    }
    let (subject, scope) = if merged {
        (request.tool.clone(), tool_scope(request))
    } else {
        (FOLDER_SUBJECT.to_owned(), GLOBAL_SCOPE.to_owned())
    };
    let mut approval = approvals.create(&request.actor.run_id, &subject, &scope)?;
    insert(&mut approval, "kind", json!(FOLDER_APPROVAL_KIND));
    insert(
        &mut approval,
        "conversationId",
        json!(request.prompt.conversation.clone().unwrap_or_default()),
    );
    insert(&mut approval, "tool", json!(request.tool));
    if let Some(action) = &request.prompt.action {
        insert(&mut approval, "action", json!(action));
    }
    insert(&mut approval, "folders", json!(entries));
    if let Some(changes) = changes(request) {
        insert(&mut approval, "changes", changes);
    }
    Ok(approval)
}

async fn ask(
    context: &StepContext,
    request: &AccessRequest,
    asking: Asking,
    approval: ApprovalRequestDto,
) -> Result<ApprovalDecision, AppError> {
    let attendant = request.prompt.attendant.clone();
    if let Some(attendant) = &attendant {
        attendant.asking(asking, &approval);
    }
    let decision = context.request_approval(approval.clone()).await;
    match &attendant {
        Some(attendant) => attendant.answered(asking, &approval, decision),
        None => decision,
    }
}

pub async fn ask_tool(
    approvals: &ApprovalService,
    context: &StepContext,
    request: &AccessRequest,
) -> Result<(), AppError> {
    let approval = tool_approval(approvals, request)?;
    match ask(context, request, Asking::Tool, approval).await? {
        ApprovalDecision::Approved => Ok(()),
        ApprovalDecision::Denied => Err(AppError::new(
            AppErrorCode::ApprovalDenied,
            request
                .prompt
                .denied
                .clone()
                .unwrap_or_else(|| APPROVAL_DENIED.to_owned()),
        )),
    }
}

pub async fn ask_grant(
    approvals: &ApprovalService,
    context: &StepContext,
    request: &AccessRequest,
    merged: bool,
) -> Result<(), AppError> {
    if request.grant_resources().is_empty() {
        return Ok(());
    }
    let approval = grant_approval(approvals, request, merged)?;
    match ask(context, request, Asking::Grant, approval).await? {
        ApprovalDecision::Approved => Ok(()),
        ApprovalDecision::Denied => {
            Err(AppError::new(AppErrorCode::ApprovalDenied, APPROVAL_DENIED))
        }
    }
}

pub async fn grant(
    approvals: &ApprovalService,
    context: &StepContext,
    mut request: AccessRequest,
) -> Result<(), AppError> {
    context.check_cancelled()?;
    if request.actor.run_id.is_empty() {
        request.actor.run_id = context.run_id.to_string();
    }
    ask_grant(approvals, context, &request, false).await
}
