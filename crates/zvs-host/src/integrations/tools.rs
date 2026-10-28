use std::sync::Arc;

use async_trait::async_trait;
use futures::future::BoxFuture;
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;
use zvs_contract::AppError;

use crate::kernel::{McpNodeInput, NodeDef, NodeRegistry, PermissionRequirement, StepContext};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationCallTarget {
    pub account_id: String,
    pub tool: String,
}

pub type IntegrationToolCaller = Arc<
    dyn Fn(
            IntegrationCallTarget,
            Map<String, Value>,
            CancellationToken,
        ) -> BoxFuture<'static, Result<Value, AppError>>
        + Send
        + Sync,
>;

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

struct IntegrationToolNode {
    key: String,
    target: IntegrationCallTarget,
    side_effect: bool,
    call: IntegrationToolCaller,
}

#[async_trait]
impl NodeDef for IntegrationToolNode {
    type Input = McpNodeInput;
    type Output = Value;

    fn node_type(&self) -> &str {
        &self.key
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(self.key.clone())
    }

    fn side_effect(&self) -> bool {
        self.side_effect
    }

    async fn run(&self, context: &StepContext, input: McpNodeInput) -> Result<Value, AppError> {
        context.check_cancelled()?;
        (self.call)(self.target.clone(), input.0, context.cancel.clone()).await
    }
}

pub fn register_integration_tool_node(
    nodes: &NodeRegistry,
    key: String,
    target: IntegrationCallTarget,
    side_effect: bool,
    call: IntegrationToolCaller,
) -> Result<(), AppError> {
    nodes.unregister(&key);
    nodes.register(IntegrationToolNode {
        key,
        target,
        side_effect,
        call,
    })
}
