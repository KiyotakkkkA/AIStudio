use std::sync::Arc;

use zvs_contract::{
    AppError, CreateMcpServerInput, McpRemoveOutput, McpServerRef, UpdateMcpServerInput,
};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "mcp.list",
            |services: Arc<Services>, _input: ()| async move { services.mcp.list() },
        )
        .handle(
            "mcp.get",
            |services: Arc<Services>, input: McpServerRef| async move {
                services.mcp.get(&input.id.to_string())
            },
        )
        .handle(
            "mcp.create",
            |services: Arc<Services>, input: CreateMcpServerInput| async move {
                services.mcp.create(&input)
            },
        )
        .handle(
            "mcp.update",
            |services: Arc<Services>, input: UpdateMcpServerInput| async move {
                services.mcp.update(&input).await
            },
        )
        .handle(
            "mcp.remove",
            |services: Arc<Services>, input: McpServerRef| async move {
                services.mcp.remove(&input.id.to_string()).await?;
                Ok::<McpRemoveOutput, AppError>(McpRemoveOutput {
                    id: input.id,
                    removed: true,
                })
            },
        )
        .handle(
            "mcp.start",
            |services: Arc<Services>, input: McpServerRef| async move {
                let id = input.id.to_string();
                let _ = services.mcp.start(&id).await;
                services.mcp.server(&id)
            },
        )
        .handle(
            "mcp.stop",
            |services: Arc<Services>, input: McpServerRef| async move {
                let id = input.id.to_string();
                services.mcp.stop(&id).await;
                services.mcp.server(&id)
            },
        )
        .handle(
            "mcp.test",
            |services: Arc<Services>, input: McpServerRef| async move {
                services.mcp.test(&input.id.to_string()).await
            },
        )
        .handle(
            "mcp.health",
            |services: Arc<Services>, input: McpServerRef| async move {
                services.mcp.health(&input.id.to_string())
            },
        )
        .handle(
            "mcp.signIn",
            |services: Arc<Services>, input: McpServerRef| async move {
                services.mcp.sign_in(&input.id.to_string()).await
            },
        )
        .handle(
            "mcp.cancelSignIn",
            |services: Arc<Services>, input: McpServerRef| async move {
                services.mcp.cancel_sign_in(&input.id.to_string())
            },
        )
        .handle(
            "mcp.signOut",
            |services: Arc<Services>, input: McpServerRef| async move {
                services.mcp.sign_out(&input.id.to_string()).await
            },
        );
}

#[cfg(test)]
mod tests;
