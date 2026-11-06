use std::sync::Arc;

use zvs_contract::{AppError, BrowserSettingsDto};

use super::IpcServer;
use crate::services::Services;

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
            json!({ "ok": true, "data": { "enabled": true, "webLinks": "studio", "localLinks": "studio", "fullUrl": false } })
        );
        let saved = server
            .dispatch(
                "browserSettings.set",
                json!({ "enabled": false, "webLinks": "system", "localLinks": "studio", "fullUrl": true }),
            )
            .await;
        assert_eq!(
            saved,
            json!({ "ok": true, "data": { "enabled": false, "webLinks": "system", "localLinks": "studio", "fullUrl": true } })
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
                json!({ "enabled": true, "webLinks": "system", "localLinks": "studio", "fullUrl": true }),
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
