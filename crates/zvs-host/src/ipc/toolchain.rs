use std::sync::Arc;

use zvs_contract::{ToolchainListInput, ToolchainRef, ToolchainSelectInput};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "toolchain.list",
            |services: Arc<Services>, input: ToolchainListInput| async move {
                services.toolchain.list(input.check_updates).await
            },
        )
        .handle(
            "toolchain.install",
            |services: Arc<Services>, input: ToolchainRef| async move {
                services.toolchain.install(input.runtime).await
            },
        )
        .handle(
            "toolchain.scan",
            |services: Arc<Services>, input: ToolchainRef| async move {
                services.toolchain.scan(input.runtime).await
            },
        )
        .handle(
            "toolchain.select",
            |services: Arc<Services>, input: ToolchainSelectInput| async move {
                services.toolchain.select(&input).await
            },
        )
        .handle(
            "toolchain.update",
            |services: Arc<Services>, input: ToolchainRef| async move {
                services.toolchain.update(input.runtime).await
            },
        )
        .handle(
            "toolchain.remove",
            |services: Arc<Services>, input: ToolchainRef| async move {
                services.toolchain.remove(input.runtime).await
            },
        );
}

#[cfg(test)]
mod tests;
