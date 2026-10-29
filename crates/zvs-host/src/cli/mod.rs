pub mod connection;
pub mod installer;
pub mod path_list;

use std::path::Path;

use serde_json::json;
use zvs_contract::{
    AppError, AppErrorCode, CLI_COMMAND, CliInstallDto, CliOpenInput, CliSessionDto,
    WorkingFolderAccess,
};

pub use connection::{CLI_CHANNELS, CliDispatch, serve};
pub use installer::{CliInstallState, CliInstaller, NoCliInstaller, SharedCliInstaller};
pub use path_list::{path_contains, path_with, path_without};

use crate::platform::shell::TRAY_SETTING_KEY;
use crate::services::Services;
use crate::workspace::folder_access;

pub const CLI_INSTALL_FAILED: &str = "Не удалось установить консольную версию";
pub const CLI_UNINSTALL_FAILED: &str = "Не удалось удалить консольную версию";
pub const CLI_NOT_BUNDLED: &str = "Консольная версия не входит в эту сборку";

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
