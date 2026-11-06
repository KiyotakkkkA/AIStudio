use std::sync::Arc;

use zvs_contract::{
    AppError, AppErrorCode, BrowserDownloadRef, BrowserSettingsDto, VectorSourceKind,
};

use super::IpcServer;
use crate::browser::PROFILE_UNAVAILABLE;
use crate::services::Services;

pub const FOLDER_PICKER_UNAVAILABLE: &str = "Окно выбора папки недоступно";

async fn pick_download_dir(services: Arc<Services>) -> Result<BrowserSettingsDto, AppError> {
    let picker = services
        .picker
        .clone()
        .ok_or_else(|| AppError::new(AppErrorCode::NotImplemented, FOLDER_PICKER_UNAVAILABLE))?;
    let current = services.browser_settings.get();
    let Some(folder) = picker
        .pick(VectorSourceKind::Folder)
        .await?
        .into_iter()
        .next()
    else {
        return Ok(current);
    };
    services.browser_settings.set(&BrowserSettingsDto {
        download_dir: Some(folder),
        ..current
    })
}

async fn clear_data(services: Arc<Services>) -> Result<(), AppError> {
    let profile = services
        .browser_profile
        .clone()
        .ok_or_else(|| AppError::new(AppErrorCode::NotImplemented, PROFILE_UNAVAILABLE))?;
    profile.clear_data().await?;
    services.browser_downloads.clear()?;
    services.browser_library.clear_history()?;
    Ok(())
}

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "browserSettings.get",
            |services: Arc<Services>, _input: ()| async move {
                Ok::<_, AppError>(services.browser_settings.get())
            },
        )
        .handle(
            "browserSettings.set",
            |services: Arc<Services>, input: BrowserSettingsDto| async move {
                services.browser_settings.set(&input)
            },
        )
        .handle(
            "browserSettings.pickDownloadDir",
            |services: Arc<Services>, _input: ()| pick_download_dir(services),
        )
        .handle(
            "browserSettings.clearData",
            |services: Arc<Services>, _input: ()| clear_data(services),
        )
        .handle(
            "browserDownloads.list",
            |services: Arc<Services>, _input: ()| async move { services.browser_downloads.list() },
        )
        .handle(
            "browserDownloads.remove",
            |services: Arc<Services>, input: BrowserDownloadRef| async move {
                services.browser_downloads.remove(&input.id)
            },
        )
        .handle(
            "browserDownloads.clear",
            |services: Arc<Services>, _input: ()| async move { services.browser_downloads.clear() },
        )
        .handle(
            "browserDownloads.open",
            |services: Arc<Services>, input: BrowserDownloadRef| async move {
                services.browser_downloads.open(&input.id)
            },
        )
        .handle(
            "browserDownloads.reveal",
            |services: Arc<Services>, input: BrowserDownloadRef| async move {
                services.browser_downloads.reveal(&input.id)
            },
        );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use zvs_contract::ToolSource;

    use super::*;
    use crate::browser::capability::BROWSER_CLICK;
    use crate::browser::tool_switch;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::platform::logger::silent_logger;
    use crate::services::SystemService;
    use crate::testing::{TestServices, test_services};
    use crate::tools::ToolDescriptor;

    fn server() -> (TestServices, IpcServer) {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);
        (host, server)
    }

    #[tokio::test]
    async fn turning_the_browser_off_hides_its_command_and_withdraws_its_tools() {
        let (host, server) = server();
        host.tools
            .register(&ToolDescriptor::new(BROWSER_CLICK, ToolSource::Browser))
            .expect("registers");
        host.browser_settings
            .subscribe(tool_switch(host.tools.clone()));
        let commands = |host: &TestServices| -> Vec<String> {
            host.chat
                .commands()
                .into_iter()
                .map(|command| command.command.as_str().to_owned())
                .collect()
        };
        assert!(commands(&host).contains(&"browser-use".to_owned()));
        assert_eq!(
            server.dispatch("browserSettings.get", json!(null)).await,
            json!({ "ok": true, "data": { "enabled": true, "webLinks": "studio", "localLinks": "studio", "fullUrl": false, "passwordAutosave": false, "formAutofill": true, "downloadDir": null, "askDownload": false } })
        );
        let saved = server
            .dispatch(
                "browserSettings.set",
                json!({ "enabled": false, "webLinks": "system", "localLinks": "studio", "fullUrl": true, "passwordAutosave": false, "formAutofill": true, "downloadDir": null, "askDownload": false }),
            )
            .await;
        assert_eq!(
            saved,
            json!({ "ok": true, "data": { "enabled": false, "webLinks": "system", "localLinks": "studio", "fullUrl": true, "passwordAutosave": false, "formAutofill": true, "downloadDir": null, "askDownload": false } })
        );
        assert!(!commands(&host).contains(&"browser-use".to_owned()));
        let click = host
            .tools
            .find_by_key(BROWSER_CLICK)
            .expect("reads")
            .expect("registered");
        assert!(!click.available);
        server
            .dispatch(
                "browserSettings.set",
                json!({ "enabled": true, "webLinks": "system", "localLinks": "studio", "fullUrl": true, "passwordAutosave": false, "formAutofill": true, "downloadDir": null, "askDownload": false }),
            )
            .await;
        assert!(
            host.tools
                .find_by_key(BROWSER_CLICK)
                .expect("reads")
                .expect("registered")
                .available
        );
    }
}
