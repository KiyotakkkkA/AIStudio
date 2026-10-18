use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;
use zvs_contract::validate::check;
use zvs_contract::{AppError, McpCallResultDto, Validate};

use super::registry::NodeRegistry;
use super::types::{NodeDef, PermissionRequirement, StepContext};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpNodeTarget {
    pub key: String,
    pub server_id: String,
    pub tool: String,
    pub read_only: bool,
    pub timeout: Option<Duration>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpCallTarget {
    pub server_id: String,
    pub tool: String,
    pub timeout: Option<Duration>,
}

pub type McpToolCaller = Arc<
    dyn Fn(
            McpCallTarget,
            Map<String, Value>,
            CancellationToken,
        ) -> BoxFuture<'static, Result<McpCallResultDto, AppError>>
        + Send
        + Sync,
>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct McpNodeInput(pub Map<String, Value>);

impl<'de> Deserialize<'de> for McpNodeInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<Map<String, Value>>::deserialize(deserializer)
            .map(|arguments| Self(arguments.unwrap_or_default()))
    }
}

impl Validate for McpNodeInput {
    fn validate(&self) -> Result<(), AppError> {
        self.0
            .keys()
            .try_for_each(|name| check::max_len(name, name, 200))
    }
}

pub struct McpToolNode {
    target: McpNodeTarget,
    call: McpToolCaller,
}

#[async_trait]
impl NodeDef for McpToolNode {
    type Input = McpNodeInput;
    type Output = McpCallResultDto;

    fn node_type(&self) -> &str {
        &self.target.key
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(self.target.key.clone())
    }

    fn side_effect(&self) -> bool {
        !self.target.read_only
    }

    async fn run(
        &self,
        context: &StepContext,
        input: McpNodeInput,
    ) -> Result<McpCallResultDto, AppError> {
        context.check_cancelled()?;
        (self.call)(
            McpCallTarget {
                server_id: self.target.server_id.clone(),
                tool: self.target.tool.clone(),
                timeout: self.target.timeout,
            },
            input.0,
            context.cancel.clone(),
        )
        .await
    }
}

pub fn register_mcp_tool_node(
    nodes: &NodeRegistry,
    target: McpNodeTarget,
    call: McpToolCaller,
) -> Result<(), AppError> {
    nodes.unregister(&target.key);
    nodes.register(McpToolNode { target, call })
}
