pub mod connection;
pub mod installer;
pub mod notices;
pub mod path_list;
pub mod theme;

use std::path::Path;

use serde_json::json;
use zvs_contract::{
    AppError, AppErrorCode, CLI_COMMAND, CliInstallDto, CliOpenInput, CliSessionDto,
    CreateMcpServerInput, McpServerDto, WorkingFolderAccess,
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
pub const CLI_CONNECT_FAILED: &str = "Не удалось подключить провайдера";
pub const CLI_MCP_SECRET_REFUSED: &str = "Секреты недоступны консольной версии. Укажите значение напрямую или добавьте сервер в приложении";

pub const fn account_label(family: zvs_contract::AccountFamily) -> &'static str {
    use zvs_contract::AccountFamily;
    match family {
        AccountFamily::Openai => "OpenAI",
        AccountFamily::Copilot => "GitHub Copilot",
        AccountFamily::QwenWeb => "Qwen",
        AccountFamily::ZaiWeb => "Z.ai",
    }
}

const fn account_base_url(family: zvs_contract::AccountFamily) -> &'static str {
    use zvs_contract::AccountFamily;
    match family {
        AccountFamily::Openai => zvs_openai::API_BASE,
        AccountFamily::Copilot => zvs_copilot::BASE_URL,
        AccountFamily::QwenWeb => zvs_qwen_web::protocol::API_BASE,
        AccountFamily::ZaiWeb => zvs_zai_web::protocol::API_BASE,
    }
}

const fn account_adapter(family: zvs_contract::AccountFamily) -> zvs_contract::AdapterFamily {
    use zvs_contract::{AccountFamily, AdapterFamily};
    match family {
        AccountFamily::Openai => AdapterFamily::Openai,
        AccountFamily::Copilot => AdapterFamily::Copilot,
        AccountFamily::QwenWeb => AdapterFamily::QwenWeb,
        AccountFamily::ZaiWeb => AdapterFamily::ZaiWeb,
    }
}

fn account_provider_name(account: &zvs_contract::AccountDto) -> String {
    let vendor = account_label(account.adapter);
    if account.adapter != zvs_contract::AccountFamily::Openai {
        return vendor.to_owned();
    }
    let title = account
        .display_name
        .as_deref()
        .or(account.email_masked.as_deref())
        .filter(|named| !named.is_empty())
        .unwrap_or(vendor);
    format!("{vendor} · {title}").chars().take(128).collect()
}

pub fn connect_failed(family: zvs_contract::AccountFamily) -> AppError {
    AppError::new(
        AppErrorCode::ProviderUnreachable,
        format!("{CLI_CONNECT_FAILED}: {}", account_label(family)),
    )
}

pub async fn link_account(
    services: &Services,
    family: zvs_contract::AccountFamily,
) -> Result<zvs_contract::AccountLinkResult, AppError> {
    use zvs_contract::{
        AccountLinkResult, AuthMode, CreateProviderInput, ProviderCapability,
        ProviderConnectionInput, ProviderKind, ProviderListFilter,
    };
    let result = services.accounts.link(family).await?;
    if let AccountLinkResult::Linked { account, .. } = &result {
        let summaries = services.providers.list(&ProviderListFilter::default())?;
        let mut existing = None;
        for summary in summaries
            .into_iter()
            .filter(|provider| provider.adapter.account_family() == Some(family))
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
                name: account_provider_name(account),
                enabled: Some(true),
                connection: ProviderConnectionInput {
                    kind: ProviderKind::OpenaiCompatible,
                    adapter: Some(account_adapter(family)),
                    auth_mode: Some(AuthMode::Account),
                    base_url: account_base_url(family).to_owned(),
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
            return Err(connect_failed(family));
        }
    }
    Ok(result)
}

pub fn create_mcp_server(
    services: &Services,
    input: &CreateMcpServerInput,
) -> Result<McpServerDto, AppError> {
    let refers_to_secret = input
        .env
        .iter()
        .flatten()
        .chain(input.headers.iter().flatten())
        .any(|(_, value)| value.secret_id().is_some());
    if refers_to_secret {
        return Err(AppError::new(
            AppErrorCode::PermissionDenied,
            CLI_MCP_SECRET_REFUSED,
        ));
    }
    services.mcp.create(input)
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
