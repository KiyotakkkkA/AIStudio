pub mod connection;
pub mod installer;
pub mod notices;
pub mod path_list;
pub mod theme;

use std::path::Path;

use serde_json::json;
use zvs_contract::{
    AppError, AppErrorCode, CLI_COMMAND, CliInstallDto, CliOpenInput, CliSessionDto,
    WorkingFolderAccess,
};

pub use connection::{CLI_CHANNELS, CliDispatch, serve};
pub use installer::{CliInstallState, CliInstaller, NoCliInstaller, SharedCliInstaller};
pub use notices::CliNotices;
pub use path_list::{path_contains, path_with, path_without};
pub use theme::{CLI_THEME_SAVE_FAILED, CliThemeFile};

use crate::platform::shell::TRAY_SETTING_KEY;
use crate::services::Services;
use crate::workspace::folder_access;

pub const CLI_INSTALL_FAILED: &str = "Не удалось установить консольную версию";
pub const CLI_UNINSTALL_FAILED: &str = "Не удалось удалить консольную версию";
pub const CLI_NOT_BUNDLED: &str = "Консольная версия не входит в эту сборку";

/// Narrow sign-in entry point: the pipe never receives general account/provider editing.
pub async fn link_copilot(
    services: &Services,
) -> Result<zvs_contract::AccountLinkResult, AppError> {
    use zvs_contract::{
        AccountFamily, AccountLinkResult, AdapterFamily, AuthMode, CreateProviderInput,
        ProviderCapability, ProviderConnectionInput, ProviderKind, ProviderListFilter,
    };
    let result = services.accounts.link(AccountFamily::Copilot).await?;
    if let AccountLinkResult::Linked { account, .. } = &result {
        let summaries = services.providers.list(&ProviderListFilter::default())?;
        let mut existing = None;
        for summary in summaries
            .into_iter()
            .filter(|provider| provider.adapter == AdapterFamily::Copilot)
        {
            let provider = services.providers.get(&summary.id.to_string(), false)?;
            if provider.account_id == Some(account.id) {
                existing = Some(provider);
                break;
            }
        }
        let provider = match existing {
            Some(provider) if !provider.summary.enabled => {
                services
                    .providers
                    .update(&zvs_contract::UpdateProviderInput {
                        enabled: Some(true),
                        ..zvs_contract::UpdateProviderInput::new(provider.summary.id)
                    })?
            }
            Some(provider) => provider,
            None => services.providers.create(&CreateProviderInput {
                name: "GitHub Copilot".to_owned(),
                enabled: Some(true),
                connection: ProviderConnectionInput {
                    kind: ProviderKind::OpenaiCompatible,
                    adapter: Some(AdapterFamily::Copilot),
                    auth_mode: Some(AuthMode::Account),
                    base_url: zvs_copilot::BASE_URL.to_owned(),
                    secret_id: None,
                    account_id: Some(Some(account.id)),
                    capabilities: vec![ProviderCapability::Text],
                    settings: None,
                },
            })?,
        };
        let probe = services
            .providers
            .probe(
                crate::services::ProbeInput::Saved(provider.summary.id.to_string()),
                None,
            )
            .await?;
        if !probe.outcome.succeeded() {
            return Err(crate::drivers::ai::copilot::failure());
        }
    }
    Ok(result)
}

pub fn open(services: &Services, input: &CliOpenInput) -> Result<CliSessionDto, AppError> {
    let folder = services.folders.working_folder(Path::new(&input.cwd))?;
    let folder = folder.to_string_lossy().into_owned();
    let access = folder_access(Some(&folder), &services.folders.grants()?, "")
        .unwrap_or(WorkingFolderAccess::Missing);
    Ok(CliSessionDto { folder, access })
}

pub fn status(services: &Services) -> CliInstallDto {
    let state = services.cli.state();
    CliInstallDto {
        available: state.available,
        installed: state.available && state.installed,
        command: CLI_COMMAND.to_owned(),
    }
}

pub fn install(services: &Services) -> Result<CliInstallDto, AppError> {
    if !services.cli.state().available {
        return Err(AppError::new(AppErrorCode::Conflict, CLI_NOT_BUNDLED));
    }
    services.cli.install().map_err(|reason| {
        AppError::new(AppErrorCode::NativeError, CLI_INSTALL_FAILED).with_detail("reason", reason)
    })?;
    let tray = json!(true);
    services.notifications.apply(TRAY_SETTING_KEY, &tray)?;
    services.settings.set(TRAY_SETTING_KEY, &tray)?;
    Ok(status(services))
}

pub fn uninstall(services: &Services) -> Result<CliInstallDto, AppError> {
    services.cli.uninstall().map_err(|reason| {
        AppError::new(AppErrorCode::NativeError, CLI_UNINSTALL_FAILED).with_detail("reason", reason)
    })?;
    Ok(status(services))
}
