use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, AppErrorCode, ToolAuthorisationDto, ToolAuthorisationKind, ToolTier};

use super::IntegrationApproval;
use crate::kernel::{
    ApprovalDecision, McpNodeInput, NodeDef, NodeRegistry, PermissionRequirement, StepContext,
};
use crate::services::ApprovalService;

pub const APPROVAL_UNAVAILABLE: &str =
    "Действие выполняется только после подтверждения, а подтверждение сейчас недоступно";

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationCallTarget {
    pub account_id: String,
    pub tool: String,
}

#[async_trait]
pub trait IntegrationGateway: Send + Sync {
    async fn approval(
        &self,
        target: &IntegrationCallTarget,
        input: &Map<String, Value>,
        cancel: &CancellationToken,
    ) -> Result<Option<IntegrationApproval>, AppError>;

    async fn call(
        &self,
        target: IntegrationCallTarget,
        input: Map<String, Value>,
        approved: Option<Map<String, Value>>,
        cancel: CancellationToken,
    ) -> Result<Value, AppError>;
}

pub type SharedGateway = Arc<dyn IntegrationGateway>;

pub fn account_tool_key(tool: &str, account_id: &str) -> String {
    let compact: Vec<char> = account_id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    let suffix: String = compact[compact.len().saturating_sub(8)..].iter().collect();
    match tool.split_once('.') {
        Some((head, rest)) => format!("{head}.{suffix}.{rest}"),
        None => format!("{tool}.{suffix}"),
    }
}

fn unavailable() -> AppError {
    AppError::new(AppErrorCode::Conflict, APPROVAL_UNAVAILABLE)
}

pub struct IntegrationToolNodeSpec {
    pub key: String,
    pub target: IntegrationCallTarget,
    pub side_effect: bool,
    pub per_call_approval: bool,
}

struct IntegrationToolNode {
    spec: IntegrationToolNodeSpec,
    gateway: SharedGateway,
    approvals: Option<Arc<ApprovalService>>,
}

impl IntegrationToolNode {
    async fn approve(
        &self,
        context: &StepContext,
        input: &Map<String, Value>,
    ) -> Result<Map<String, Value>, AppError> {
        let ticket = context.take_deferred_approval().ok_or_else(unavailable)?;
        let approvals = self.approvals.as_ref().ok_or_else(unavailable)?;
        let plan = self
            .gateway
            .approval(&self.spec.target, input, &context.cancel)
            .await?
            .ok_or_else(unavailable)?;
        context.check_cancelled()?;
        let mut request =
            approvals.create(&context.run_id.to_string(), &ticket.subject, &ticket.scope)?;
        request.insert("kind".to_owned(), json!(plan.kind));
        request.insert("tool".to_owned(), json!(self.spec.key));
        request.insert("action".to_owned(), json!(plan.action));
        for (key, value) in &plan.fields {
            if !RESERVED.contains(&key.as_str()) {
                request.insert(key.clone(), value.clone());
            }
        }
        match context.request_approval(request).await? {
            ApprovalDecision::Approved => {
                context.authorise(ToolAuthorisationDto::of(ToolAuthorisationKind::Approved))?;
                Ok(plan.fields)
            }
            ApprovalDecision::Denied => {
                Err(AppError::new(AppErrorCode::ApprovalDenied, plan.denied))
            }
        }
    }
}

#[async_trait]
impl NodeDef for IntegrationToolNode {
    type Input = McpNodeInput;
    type Output = Value;

    fn node_type(&self) -> &str {
        &self.spec.key
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(self.spec.key.clone())
    }

    fn permission_for(&self, _input: &Value) -> PermissionRequirement {
        if self.spec.per_call_approval {
            PermissionRequirement::tool_at(self.spec.key.clone(), ToolTier::Ask)
        } else {
            self.permission()
        }
    }

    fn defers_approval(&self) -> bool {
        self.spec.per_call_approval
    }

    fn side_effect(&self) -> bool {
        self.spec.side_effect
    }

    async fn run(&self, context: &StepContext, input: McpNodeInput) -> Result<Value, AppError> {
        context.check_cancelled()?;
        let approved = if self.spec.per_call_approval {
            Some(self.approve(context, &input.0).await?)
        } else {
            None
        };
        context.check_cancelled()?;
        self.gateway
            .call(
                self.spec.target.clone(),
                input.0,
                approved,
                context.cancel.clone(),
            )
            .await
    }
}

pub fn register_integration_tool_node(
    nodes: &NodeRegistry,
    spec: IntegrationToolNodeSpec,
    gateway: SharedGateway,
    approvals: Option<Arc<ApprovalService>>,
) -> Result<(), AppError> {
    nodes.unregister(&spec.key);
    nodes.register(IntegrationToolNode {
        spec,
        gateway,
        approvals,
    })
}
