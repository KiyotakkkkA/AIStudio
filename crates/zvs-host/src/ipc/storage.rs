use std::sync::Arc;

use zvs_contract::StorageUsageInput;

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "storage.usage",
            |services: Arc<Services>, input: StorageUsageInput| async move {
                services.storage.usage(input.refresh()).await
            },
        )
        .handle(
            "storage.clearCache",
            |services: Arc<Services>, _input: ()| async move { services.storage.clear_cache().await },
        );
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::json;

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::platform::logger::silent_logger;
    use crate::services::SystemService;
    use crate::testing::test_services;

    #[tokio::test]
    async fn usage_and_cache_clearing_answer_with_valid_output() {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);

        let usage = server
            .dispatch("storage.usage", json!({ "refresh": true }))
            .await;
        assert_eq!(usage["ok"], json!(true), "{usage}");
        assert_eq!(
            usage["data"]["categories"].as_array().map(Vec::len),
            Some(11)
        );

        let cleared = server.dispatch("storage.clearCache", json!(null)).await;
        assert_eq!(cleared["ok"], json!(true), "{cleared}");
        assert_eq!(cleared["data"]["removedBytes"], json!(0));
    }
}
