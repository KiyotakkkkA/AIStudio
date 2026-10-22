use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use zvs_contract::ToolchainRuntime;
use zvs_contract::validate::js_slice;

use super::definitions::{Version, companion_name, definition, primary_name};
use crate::runtimes::Platform;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

#[async_trait]
pub trait VersionProbe: Send + Sync {
    async fn run(&self, executable: &Path) -> Result<String, String>;
}

pub type SharedProbe = Arc<dyn VersionProbe>;

pub struct ProcessProbe {
    pub timeout: Duration,
}

impl Default for ProcessProbe {
    fn default() -> Self {
        Self {
            timeout: PROBE_TIMEOUT,
        }
    }
}

#[async_trait]
impl VersionProbe for ProcessProbe {
    async fn run(&self, executable: &Path) -> Result<String, String> {
        let mut command = tokio::process::Command::new(executable);
        command
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);
        let output = tokio::time::timeout(self.timeout, command.output())
            .await
            .map_err(|_| "не ответил за отведённое время".to_owned())?
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "завершился с кодом {}{}",
                output
                    .status
                    .code()
                    .map_or_else(|| "?".to_owned(), |code| code.to_string()),
                if stderr.trim().is_empty() {
                    String::new()
                } else {
                    format!(": {}", js_slice(stderr.trim(), 200))
                }
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probed {
    pub executable: PathBuf,
    pub version: Version,
}

fn same_name(path: &Path, name: &str) -> bool {
    path.file_name()
        .is_some_and(|file| file.to_string_lossy().eq_ignore_ascii_case(name))
}

pub fn primary_for(runtime: ToolchainRuntime, platform: Platform, picked: &Path) -> PathBuf {
    if same_name(picked, companion_name(runtime, platform))
        && let Some(directory) = picked.parent()
    {
        return directory.join(primary_name(runtime, platform));
    }
    picked.to_path_buf()
}

pub async fn probe(
    probe: &dyn VersionProbe,
    runtime: ToolchainRuntime,
    platform: Platform,
    executable: &Path,
) -> Result<Probed, String> {
    let definition = definition(runtime);
    let primary = primary_name(runtime, platform);
    if !executable.is_file() {
        return Err(format!("Файл не найден: {}", executable.display()));
    }
    if !same_name(executable, primary) {
        return Err(format!("Ожидался файл {primary}"));
    }
    let companion = companion_name(runtime, platform);
    if !executable
        .parent()
        .is_some_and(|directory| directory.join(companion).is_file())
    {
        return Err(format!("Рядом с {primary} нет {companion}"));
    }
    let output = probe
        .run(executable)
        .await
        .map_err(|reason| format!("{primary} --version: {reason}"))?;
    let first = output.lines().next().unwrap_or_default().trim();
    let version = first
        .starts_with(definition.banner)
        .then(|| Version::find(first))
        .flatten()
        .ok_or_else(|| {
            format!(
                "Не удалось определить версию {} по ответу «{}»",
                definition.display_name,
                js_slice(first, 120)
            )
        })?;
    if version < definition.minimum {
        return Err(format!(
            "Версия {version} ниже минимальной {}",
            definition.minimum
        ));
    }
    Ok(Probed {
        executable: executable.to_path_buf(),
        version,
    })
}
