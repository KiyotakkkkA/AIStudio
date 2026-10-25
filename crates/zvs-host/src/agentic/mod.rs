pub mod node;
pub mod plan;
pub mod service;
pub mod store;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use zvs_contract::AppError;

pub use node::{AGENTIC_DOMAIN, AGENTIC_SESSION, SessionNode, budget_exceeded};
pub use service::{AgenticService, AgenticServiceOptions};
pub use store::{SESSION_NOT_FOUND, SessionStore};

use crate::kernel::NodeRegistry;
use crate::tools::ToolRegistry;

pub fn register_agentic_nodes(
    registry: &Arc<NodeRegistry>,
    store: SessionStore,
    tools: ToolRegistry,
) -> Result<(), AppError> {
    registry.register(SessionNode::new(store, tools))
}
