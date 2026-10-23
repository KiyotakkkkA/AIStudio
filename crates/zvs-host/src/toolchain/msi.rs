use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use zvs_contract::{AppError, AppErrorCode};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
pub const MSI_TIMEOUT: Duration = Duration::from_secs(30 * 60);
pub const MSI_FAILED: &str = "Не удалось распаковать установочный пакет";
pub const MSI_UNSUPPORTED: &str = "Распаковка пакетов MSI доступна только в Windows";

#[async_trait]
pub trait MsiExtractor: Send + Sync {
    async fn extract(&self, package: &Path, target: &Path) -> Result<(), AppError>;
}

pub type SharedMsiExtractor = Arc<dyn MsiExtractor>;

pub struct MsiexecExtractor {
    pub timeout: Duration,
}

impl Default for MsiexecExtractor {
    fn default() -> Self {
        Self {
            timeout: MSI_TIMEOUT,
        }
    }
}

fn failed(reason: impl Into<String>) -> AppError {
    AppError::new(AppErrorCode::NativeError, MSI_FAILED).with_detail("reason", reason.into())
}

pub fn remove_packages(target: &Path) {
    let Ok(entries) = std::fs::read_dir(target) else {
        return;
    };
    let packages: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("msi"))
        })
        .collect();
    for package in packages {
        let _ = std::fs::remove_file(package);
    }
}

#[async_trait]
impl MsiExtractor for MsiexecExtractor {
    async fn extract(&self, package: &Path, target: &Path) -> Result<(), AppError> {
        if !cfg!(windows) {
            return Err(AppError::new(
                AppErrorCode::UnsupportedFormat,
                MSI_UNSUPPORTED,
            ));
        }
        std::fs::create_dir_all(target).map_err(|error| failed(error.to_string()))?;
        let mut command = tokio::process::Command::new("msiexec");
        command
            .arg("/a")
            .arg(package)
            .arg("/qn")
            .arg(format!("TARGETDIR={}", target.display()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);
        let status = tokio::time::timeout(self.timeout, command.status())
            .await
            .map_err(|_| failed("timeout"))?
            .map_err(|error| failed(error.to_string()))?;
        if !status.success() {
            return Err(failed(format!(
                "msiexec exit code {}",
                status
                    .code()
                    .map_or_else(|| "?".to_owned(), |code| code.to_string())
            )));
        }
        remove_packages(target);
        Ok(())
    }
}
