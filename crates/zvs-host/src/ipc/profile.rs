use std::sync::Arc;

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server.handle(
        "profile.avatar",
        |services: Arc<Services>, _input: ()| async move { services.profile_avatar.load().await },
    );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::Value;

    use super::*;
    use crate::ipc::Caller;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::platform::logger::silent_logger;
    use crate::services::SystemService;
    use crate::testing::test_services;

    #[tokio::test]
    async fn only_the_studio_can_load_the_avatar() {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);

        let response = server
            .dispatch(Caller::Studio, "profile.avatar", Value::Null)
            .await;
        assert_eq!(response["ok"], true, "{response}");
        for caller in [
            Caller::Cli { connection: 1 },
            Caller::BrowserUi,
            Caller::Panel,
        ] {
            let response = server.dispatch(caller, "profile.avatar", Value::Null).await;
            assert_eq!(response["error"]["code"], "PERMISSION_DENIED");
        }
    }
}
