use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, AppErrorCode, ToolTier};

use super::IntegrationApproval;
use crate::access::{AccessRequest, Admitted, Effect};
use crate::kernel::{McpNodeInput, NodeDef, NodeRegistry, PermissionRequirement, StepContext};

pub const APPROVAL_UNAVAILABLE: &str =
    "Действие выполняется только после подтверждения, а подтверждение сейчас недоступно";

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
}

pub struct IntegrationCall {
    input: Map<String, Value>,
    plan: Option<IntegrationApproval>,
}

#[async_trait]
impl NodeDef for IntegrationToolNode {
    type Input = McpNodeInput;
    type Output = Value;
    type Prepared = IntegrationCall;

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

    fn side_effect(&self) -> bool {
        self.spec.side_effect
    }

    async fn prepare(
        &self,
        context: &StepContext,
        input: McpNodeInput,
    ) -> Result<IntegrationCall, AppError> {
        context.check_cancelled()?;
        let plan = if self.spec.per_call_approval {
            let plan = self
                .gateway
                .approval(&self.spec.target, &input.0, &context.cancel)
                .await?
                .ok_or_else(unavailable)?;
            context.check_cancelled()?;
            Some(plan)
        } else {
            None
        };
        Ok(IntegrationCall {
            input: input.0,
            plan,
        })
    }

    fn access(&self, prepared: &IntegrationCall, request: AccessRequest) -> AccessRequest {
        let Some(plan) = &prepared.plan else {
            return request;
        };
        let mut request = request.with_effect(Effect::Remote {
            service: self.spec.target.tool.clone(),
            action: plan.action.clone(),
        });
        request.always_ask = true;
        request.prompt.kind = Some(plan.kind.clone());
        request.prompt.action = Some(plan.action.clone());
        request.prompt.fields = plan
            .fields
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        request.prompt.denied = Some(plan.denied.clone());
        request
    }

    async fn execute(
        &self,
        context: &StepContext,
        admitted: Admitted<IntegrationCall>,
    ) -> Result<Value, AppError> {
        let IntegrationCall { input, plan } = admitted.into_inner();
        context.check_cancelled()?;
        self.gateway
            .call(
                self.spec.target.clone(),
                input,
                plan.map(|plan| plan.fields),
                context.cancel.clone(),
            )
            .await
    }
}

pub fn register_integration_tool_node(
    nodes: &NodeRegistry,
    spec: IntegrationToolNodeSpec,
    gateway: SharedGateway,
) -> Result<(), AppError> {
    nodes.unregister(&spec.key);
    nodes.register(IntegrationToolNode { spec, gateway })
}
