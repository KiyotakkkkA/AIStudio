use std::sync::Arc;

use zvs_contract::{
    AccountLinkInput, AppError, CliConnectionRef, CliConsentAnswerInput, CliImageImportInput,
    CliOpenInput, CliSubagentCancelInput, CliThemeDto, CreateMcpServerInput,
};

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
            "cli.images.import",
            |services: Arc<Services>, input: CliImageImportInput| async move {
                cli::import_images(&services, input).await
            },
        )
        .handle(
            "cli.subagents.cancel",
            |services: Arc<Services>, input: CliSubagentCancelInput| async move {
                cli::cancel_subagent(&services, &input.run_id).await
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
        )
        .handle(
            "cli.preferences.get",
            |services: Arc<Services>, (): ()| async move {
                Ok::<_, AppError>(services.cli_preferences.get())
            },
        )
        .handle(
            "cli.preferences.set",
            |services: Arc<Services>, input: zvs_contract::CliPreferencesDto| async move {
                services.cli_preferences.set(input)
            },
        )
        .handle(
            "cli.connections.list",
            |services: Arc<Services>, (): ()| async move {
                Ok::<_, AppError>(services.cli_connections.list())
            },
        )
        .handle(
            "cli.connections.disconnect",
            |services: Arc<Services>, input: CliConnectionRef| async move {
                services.cli_connections.disconnect(input.id)
            },
        )
        .handle(
            "cli.consents.list",
            |services: Arc<Services>, (): ()| async move {
                Ok::<_, AppError>(services.cli_connections.pending())
            },
        )
        .handle(
            "cli.consents.answer",
            |services: Arc<Services>, input: CliConsentAnswerInput| async move {
                services
                    .cli_connections
                    .answer(&input.id, input.confirm)
            },
        );
}

#[cfg(test)]
mod tests;
