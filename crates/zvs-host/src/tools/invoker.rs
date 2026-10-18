use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;
use zvs_contract::{
    AppError, AppErrorCode, GraphNode, HostEventDraft, RetryPolicy, RunGraph, RunKind, RunStatus,
    StartRunInput, ToolInvokeInput, ToolInvokeResultDto, ToolSource, ToolTier,
};

use super::registry::ToolRegistry;
use crate::kernel::NodeRegistry;
use crate::platform::clock::SharedClock;
use crate::services::RunService;

pub const NODE_ID: &str = "tool";
pub const UNAVAILABLE_MESSAGE: &str =
    "Инструмент сейчас недоступен: его источник отключён или удалён";
pub const OFF_MESSAGE: &str = "Инструмент выключен: разрешение «off» запрещает любой вызов";
pub const CONFIRM_MESSAGE: &str = "Вызов требует явного подтверждения";
pub const NO_NODE_MESSAGE: &str =
    "Инструмент нельзя вызвать: источник не предоставил для него исполняемый узел";
pub const FAILED_MESSAGE: &str = "Инструмент завершился с ошибкой";
pub const REPORTED_MESSAGE: &str = "Инструмент вернул ошибку";

#[derive(Clone)]
pub struct ToolInvoker {
    tools: ToolRegistry,
    nodes: Arc<NodeRegistry>,
    runs: RunService,
    clock: SharedClock,
}

fn reported_error(output: &Value) -> Option<String> {
    let Value::Object(map) = output else {
        return None;
    };
    if map.get("isError") != Some(&Value::Bool(true)) {
        return None;
    }
    let text = map
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    Some(if text.is_empty() {
        REPORTED_MESSAGE.to_owned()
    } else {
        text.to_owned()
    })
}

impl ToolInvoker {
    pub fn new(
        tools: ToolRegistry,
        nodes: Arc<NodeRegistry>,
        runs: RunService,
        clock: SharedClock,
    ) -> Self {
        Self {
            tools,
            nodes,
            runs,
            clock,
        }
    }

    pub async fn invoke(&self, request: &ToolInvokeInput) -> Result<ToolInvokeResultDto, AppError> {
        let tool = self.tools.get(&request.id.to_string())?.tool;
        let details = |error: AppError| error.with_detail("key", tool.key.clone());
        if !tool.available {
            let code = if tool.source == ToolSource::Mcp {
                AppErrorCode::McpUnavailable
            } else {
                AppErrorCode::Conflict
            };
            return Err(details(AppError::new(code, UNAVAILABLE_MESSAGE)));
        }
        if !tool.enabled || tool.tier == ToolTier::Off {
            return Err(details(AppError::new(
                AppErrorCode::PermissionDenied,
                OFF_MESSAGE,
            )));
        }
        let confirmed = request.confirmed();
        if tool.needs_confirmation() && !confirmed {
            return Err(details(AppError::new(
                AppErrorCode::ApprovalDenied,
                CONFIRM_MESSAGE,
            )));
        }
        let node_type = self
            .nodes
            .for_tool(&tool.key)
            .ok_or_else(|| details(AppError::new(AppErrorCode::Conflict, NO_NODE_MESSAGE)))?
            .node_type()
            .to_owned();
        let started_at = self.clock.now();
        let handle = self.runs.start(StartRunInput {
            kind: RunKind::Job,
            subject_id: None,
            title: Some(format!("Проба инструмента {}", tool.key)),
            retry_of_id: None,
            graph: RunGraph::new(vec![GraphNode {
                dependencies: Some(Vec::new()),
                input: Some(request.input()),
                bindings: Some(BTreeMap::new()),
                retry: Some(RetryPolicy::default()),
                ..GraphNode::new(NODE_ID, node_type)
            }]),
            input: Some(Value::Null),
            concurrency: Some(1),
        })?;
        let runs = self.runs.clone();
        let run_id = handle.id;
        let subscription = self.runs.subscribe(
            &run_id.to_string(),
            Arc::new(move |event| {
                if !confirmed {
                    return;
                }
                let HostEventDraft::Approval { request } = &event.body else {
                    return;
                };
                if let Some(approval) = request.get("id").and_then(Value::as_str) {
                    let _ = runs.approve(&run_id, approval, false);
                }
            }),
        );
        if confirmed
            && let Some(pending) = self
                .runs
                .detail(&run_id)
                .ok()
                .and_then(|detail| detail.summary.approval)
        {
            let _ = self.runs.approve(&run_id, &pending.id, false);
        }
        self.runs.wait(&run_id).await;
        if let Ok(subscription) = subscription {
            subscription.unsubscribe();
        }
        let run = self.runs.get(&run_id)?;
        let step = self
            .runs
            .steps(&run_id)?
            .into_iter()
            .rfind(|step| step.node_id == NODE_ID);
        let output = step
            .as_ref()
            .and_then(|step| step.output.clone())
            .unwrap_or(Value::Null);
        let reported = reported_error(&output);
        let succeeded = run.status == RunStatus::Succeeded;
        let now = self.clock.now();
        let elapsed = match &step {
            None => now - started_at,
            Some(step) => step.finished_at.unwrap_or(now) - step.started_at,
        };
        Ok(ToolInvokeResultDto {
            run_id,
            ok: succeeded && reported.is_none(),
            duration_ms: u64::try_from(elapsed.max(0)).unwrap_or(0),
            output,
            error: if succeeded {
                reported
            } else {
                Some(
                    step.and_then(|step| step.error)
                        .or(run.error)
                        .unwrap_or_else(|| FAILED_MESSAGE.to_owned()),
                )
            },
        })
    }
}
