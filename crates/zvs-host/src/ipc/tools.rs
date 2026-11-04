use std::sync::Arc;

use zvs_contract::{SetToolEnabledInput, SetToolTierInput, ToolListFilter, ToolRef};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "tools.list",
            |services: Arc<Services>, filter: ToolListFilter| async move {
                services.tools.list(&filter)
            },
        )
        .handle(
            "tools.get",
            |services: Arc<Services>, input: ToolRef| async move {
                services.tools.get(&input.id.to_string())
            },
        )
        .handle(
            "tools.setTier",
            |services: Arc<Services>, input: SetToolTierInput| async move {
                services
                    .tools
                    .set_tier(&input.id.to_string(), input.tier, "user")
            },
        )
        .handle(
            "tools.setEnabled",
            |services: Arc<Services>, input: SetToolEnabledInput| async move {
                services
                    .tools
                    .set_enabled(&input.id.to_string(), input.enabled)
            },
        )
        .handle(
            "tools.usage",
            |services: Arc<Services>, input: ToolRef| async move {
                services.tools.usage(&input.id.to_string())
            },
        );
}

#[cfg(test)]
mod tests;
