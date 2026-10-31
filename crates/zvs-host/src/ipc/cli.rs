use std::sync::Arc;

use zvs_contract::{AccountLinkInput, AppError, CliOpenInput, CliThemeDto, CreateMcpServerInput};

use super::IpcServer;
use crate::cli;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "cli.accounts.link",
            |services: Arc<Services>, input: AccountLinkInput| async move {
                cli::link_account(&services, input.adapter).await
            },
        )
        .handle(
            "cli.accounts.cancelLink",
            |services: Arc<Services>, input: AccountLinkInput| async move {
                Ok::<_, AppError>(services.accounts.cancel_link(input.adapter))
            },
        )
        .handle(
            "cli.mcp.create",
            |services: Arc<Services>, input: CreateMcpServerInput| async move {
                cli::create_mcp_server(&services, &input)
            },
        )
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
