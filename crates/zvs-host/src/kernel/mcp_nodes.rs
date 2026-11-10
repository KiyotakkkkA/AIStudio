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
use super::types::{PermissionRequirement, PlainNode, StepContext};
use crate::access::{AccessRequest, Admitted, Effect};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpNodeTarget {
    pub key: String,
    pub server_id: String,
    pub tool: String,
    pub read_only: bool,
    pub destructive: bool,
    pub timeout: Option<Duration>,
    pub read_resource: bool,
}

impl McpNodeTarget {
    pub const fn effect(&self) -> Effect {
        if self.destructive {
            Effect::Opaque { destructive: true }
        } else if self.read_only {
            Effect::Read
        } else {
            Effect::Opaque { destructive: false }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpCallTarget {
    pub server_id: String,
    pub tool: String,
    pub timeout: Option<Duration>,
    pub read_resource: bool,
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
impl PlainNode for McpToolNode {
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

    fn access(&self, _input: &McpNodeInput, request: AccessRequest) -> AccessRequest {
        let effect = self.target.effect();
        if request.effect.benign() || !effect.benign() {
            request.with_effect(effect)
        } else {
            request
        }
    }

    async fn execute(
        &self,
        context: &StepContext,
        admitted: Admitted<McpNodeInput>,
    ) -> Result<McpCallResultDto, AppError> {
        let input = admitted.into_inner();
        context.check_cancelled()?;
        (self.call)(
            McpCallTarget {
                server_id: self.target.server_id.clone(),
                tool: self.target.tool.clone(),
                timeout: self.target.timeout,
                read_resource: self.target.read_resource,
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
