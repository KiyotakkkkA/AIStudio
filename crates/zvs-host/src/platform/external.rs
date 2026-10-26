use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use zvs_contract::{AppError, AppErrorCode};

pub type UrlOpener = Arc<dyn Fn(&str) -> Result<(), AppError> + Send + Sync>;
pub type PathRevealer = Arc<dyn Fn(&Path) -> Result<(), AppError> + Send + Sync>;

pub const OPEN_FAILED: &str = "Не удалось открыть браузер";
pub const REVEAL_FAILED: &str = "Не удалось показать файл в папке";
pub const LAUNCH_FAILED: &str = "Не удалось открыть файл в приложении";
pub const BROWSER_LAUNCH_FAILED: &str = "Не удалось открыть файл в браузере";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledBrowser {
    pub id: String,
    pub name: String,
    pub program: PathBuf,
}

pub trait FileLauncher: Send + Sync {
    fn application(&self, _extension: &str) -> Option<String> {
        None
    }

    fn open(&self, path: &Path) -> Result<(), AppError>;

    fn browsers(&self) -> Vec<InstalledBrowser> {
        Vec::new()
    }

    fn open_in_browser(&self, browser: &InstalledBrowser, path: &Path) -> Result<(), AppError> {
        open_with_program(&browser.program, path)
    }
}

pub fn open_with_program(program: &Path, path: &Path) -> Result<(), AppError> {
    Command::new(program)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|cause| {
            AppError::new(AppErrorCode::Unknown, BROWSER_LAUNCH_FAILED)
                .with_detail("raw", cause.to_string())
        })
}

pub type SharedFileLauncher = Arc<dyn FileLauncher>;

fn command(url: &str) -> Command {
    if cfg!(windows) {
        let mut command = Command::new("rundll32");
        command.args(["url.dll,FileProtocolHandler", url]);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg(url);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    }
}

pub fn open_in_system_browser(url: &str) -> Result<(), AppError> {
    let parsed = url::Url::parse(url)
        .map_err(|_| AppError::new(AppErrorCode::ValidationFailed, OPEN_FAILED))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::new(AppErrorCode::ValidationFailed, OPEN_FAILED));
    }
    command(parsed.as_str())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|cause| {
            AppError::new(AppErrorCode::Unknown, OPEN_FAILED).with_detail("raw", cause.to_string())
        })
}

pub fn system_url_opener() -> UrlOpener {
    Arc::new(open_in_system_browser)
}

#[cfg(any(windows, test))]
fn select_argument(path: &Path) -> std::ffi::OsString {
    let mut argument = std::ffi::OsString::from("/select,\"");
    argument.push(path.as_os_str());
    argument.push("\"");
    argument
}

#[cfg(windows)]
fn reveal_command(path: &Path) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("explorer");
    command.raw_arg(select_argument(path));
    command
}

#[cfg(not(windows))]
fn reveal_command(path: &Path) -> Command {
    if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg("-R").arg(path);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(path.parent().unwrap_or(path));
        command
    }
}

pub fn reveal_in_file_manager(path: &Path) -> Result<(), AppError> {
    reveal_command(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|cause| {
            AppError::new(AppErrorCode::Unknown, REVEAL_FAILED)
                .with_detail("raw", cause.to_string())
        })
}

pub fn system_path_revealer() -> PathRevealer {
    Arc::new(reveal_in_file_manager)
}

fn launch_command(path: &Path) -> Command {
    if cfg!(windows) {
        let mut command = Command::new("explorer");
        command.arg(path);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg(path);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    }
}

pub fn open_with_default_application(path: &Path) -> Result<(), AppError> {
    launch_command(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|cause| {
            AppError::new(AppErrorCode::Unknown, LAUNCH_FAILED)
                .with_detail("raw", cause.to_string())
        })
}

pub struct SystemFileLauncher;

impl FileLauncher for SystemFileLauncher {
    fn open(&self, path: &Path) -> Result<(), AppError> {
        open_with_default_application(path)
    }
}

pub fn system_file_launcher() -> SharedFileLauncher {
    Arc::new(SystemFileLauncher)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_selected_path_is_quoted_whole_so_commas_and_spaces_stay_in_the_name() {
        assert_eq!(
            select_argument(Path::new("C:/Отчёты/Итоги, сентябрь — декабрь.html")),
            std::ffi::OsString::from("/select,\"C:/Отчёты/Итоги, сентябрь — декабрь.html\"")
        );
    }
}
