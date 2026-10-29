use std::sync::Arc;

use serde_json::json;
use zvs_host::IpcServer;
use zvs_host::cli::CliNotices;
use zvs_host::platform::events::EventBus;
use zvs_host::platform::logger::{LogLevel, SharedLogger};

pub fn launched_by_cli() -> bool {
    is_cli_launch(&std::env::args().collect::<Vec<_>>())
}

pub fn is_cli_launch(args: &[String]) -> bool {
    args.iter()
        .skip(1)
        .any(|arg| arg == zvs_contract::CLI_LAUNCH_ARG)
}

#[cfg(windows)]
pub fn listen(server: Arc<IpcServer>, events: EventBus, notices: CliNotices, logger: SharedLogger) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = accept(server, events, notices, logger.clone()).await {
            logger.log(
                LogLevel::Warn,
                "cli",
                "The CLI pipe stopped",
                &[("error", json!(error.to_string()))],
            );
        }
    });
}

#[cfg(windows)]
async fn accept(
    server: Arc<IpcServer>,
    events: EventBus,
    notices: CliNotices,
    logger: SharedLogger,
) -> std::io::Result<()> {
    let name = zvs_pipe::pipe_name()?;
    let mut listening = zvs_pipe::create_server(&name, true)?;
    logger.log(LogLevel::Info, "cli", "Listening for the CLI", &[]);
    loop {
        listening.connect().await?;
        let connected = listening;
        listening = zvs_pipe::create_server(&name, false)?;
        let dispatch: Arc<dyn zvs_host::cli::CliDispatch> = server.clone();
        tauri::async_runtime::spawn(zvs_host::cli::serve(
            connected,
            dispatch,
            events.clone(),
            notices.clone(),
            logger.clone(),
        ));
    }
}

#[cfg(not(windows))]
pub fn listen(
    _server: Arc<IpcServer>,
    _events: EventBus,
    _notices: CliNotices,
    logger: SharedLogger,
) {
    logger.log(LogLevel::Info, "cli", "The CLI pipe is Windows-only", &[]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_cli_flag_marks_a_cli_launch() {
        let args =
            |list: &[&str]| -> Vec<String> { list.iter().map(|arg| (*arg).to_owned()).collect() };
        assert!(is_cli_launch(&args(&["zvs-studio.exe", "--cli"])));
        assert!(!is_cli_launch(&args(&["--cli"])));
        assert!(!is_cli_launch(&args(&["zvs-studio.exe", "zvs-studio://x"])));
    }
}
