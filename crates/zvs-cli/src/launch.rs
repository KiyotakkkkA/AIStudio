use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::sync::mpsc;

use crate::copy;
use crate::link::{Incoming, Link};

pub const APP_BINARY: &str = "zvs-studio.exe";
pub const LAUNCH_WAIT: Duration = Duration::from_secs(40);
pub const RETRY_EVERY: Duration = Duration::from_millis(250);

pub type Connection = (Link, mpsc::UnboundedReceiver<Incoming>);

pub fn app_candidates(cli_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(parent) = cli_dir.parent() {
        candidates.push(parent.join(APP_BINARY));
    }
    candidates.push(cli_dir.join(APP_BINARY));
    candidates
}

fn find_app() -> Option<PathBuf> {
    let current = std::env::current_exe().ok()?;
    let folder = current.parent()?;
    app_candidates(folder)
        .into_iter()
        .find(|candidate| candidate.is_file())
}

#[cfg(windows)]
fn spawn_app(app: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    std::process::Command::new(app)
        .arg(zvs_contract::CLI_LAUNCH_ARG)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map(drop)
}

#[cfg(windows)]
async fn open(name: &str) -> std::io::Result<Result<Connection, &'static str>> {
    let client = zvs_pipe::connect(name).await?;
    Ok(Link::open(client).await.map_err(|error| match error {
        crate::link::LinkError::Protocol => copy::PROTOCOL_MISMATCH,
        crate::link::LinkError::Closed => copy::APP_UNREACHABLE,
    }))
}

#[cfg(windows)]
pub async fn connect(report: impl Fn(&'static str)) -> Result<Connection, &'static str> {
    report(copy::CONNECTING);
    let name = zvs_pipe::pipe_name().map_err(|_| copy::APP_UNREACHABLE)?;
    match open(&name).await {
        Ok(result) => return result,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(copy::APP_UNREACHABLE);
        }
        Err(_) => {}
    }
    let app = find_app().ok_or(copy::APP_NOT_FOUND)?;
    report(copy::LAUNCHING);
    spawn_app(&app).map_err(|_| copy::APP_NOT_FOUND)?;
    let deadline = tokio::time::Instant::now() + LAUNCH_WAIT;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(RETRY_EVERY).await;
        if let Ok(result) = open(&name).await {
            return result;
        }
    }
    Err(copy::APP_UNREACHABLE)
}

#[cfg(not(windows))]
pub async fn connect(report: impl Fn(&'static str)) -> Result<Connection, &'static str> {
    report(copy::CONNECTING);
    let _ = find_app();
    Err(copy::APP_UNREACHABLE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_is_looked_for_above_the_cli_folder_first() {
        let installed = std::env::temp_dir().join("ZVS AI Studio");
        assert_eq!(
            app_candidates(&installed.join("cli")),
            [
                installed.join(APP_BINARY),
                installed.join("cli").join(APP_BINARY)
            ]
        );
    }
}
