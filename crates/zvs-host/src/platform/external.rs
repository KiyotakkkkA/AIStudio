use std::process::{Command, Stdio};
use std::sync::Arc;

use zvs_contract::{AppError, AppErrorCode};

pub type UrlOpener = Arc<dyn Fn(&str) -> Result<(), AppError> + Send + Sync>;

pub const OPEN_FAILED: &str = "Не удалось открыть браузер";

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
