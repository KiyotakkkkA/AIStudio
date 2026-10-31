use std::sync::Arc;

use zvs_contract::{AppError, CliOpenInput, CliThemeDto};

use super::IpcServer;
use crate::cli;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle("cli.copilot.login", |services: Arc<Services>, (): ()| async move {
            cli::link_copilot(&services).await
        })
        .handle("cli.copilot.cancelLogin", |services: Arc<Services>, (): ()| async move {
            Ok::<_, AppError>(services.accounts.cancel_link(zvs_contract::AccountFamily::Copilot))
        })
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
        )
        .handle(
            "cli.theme.get",
            |services: Arc<Services>, (): ()| async move {
                Ok::<_, AppError>(services.cli_theme.get())
            },
        )
        .handle(
            "cli.theme.set",
            |services: Arc<Services>, input: CliThemeDto| async move {
                services.cli_theme.set(input)
            },
        );
}

#[cfg(test)]
mod tests;
