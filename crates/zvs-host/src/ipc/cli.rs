use std::sync::Arc;

use zvs_contract::{AppError, CliOpenInput};

use super::IpcServer;
use crate::cli;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "cli.open",
            |services: Arc<Services>, input: CliOpenInput| async move {
                cli::open(&services, &input)
            },
        )
        .handle(
            "cli.status",
            |services: Arc<Services>, (): ()| async move {
                Ok::<_, AppError>(cli::status(&services))
            },
        )
        .handle(
            "cli.install",
            |services: Arc<Services>, (): ()| async move { cli::install(&services) },
        )
        .handle(
            "cli.uninstall",
            |services: Arc<Services>, (): ()| async move { cli::uninstall(&services) },
        );
}

#[cfg(test)]
mod tests;
