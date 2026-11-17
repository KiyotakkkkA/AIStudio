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
        .handle("storage.resetChallenge", |services: Arc<Services>, _input: ()| async move {
            Ok(services.storage.reset_challenge())
        })
        .handle("storage.reset", |services: Arc<Services>, input: zvs_contract::StorageResetConfirmation| async move {
            services.storage.reset(input)
        })
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
    async fn reset_confirmation_is_validated_before_any_reset_is_scheduled() {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);
        let refused = server
            .dispatch(
                crate::ipc::Caller::Studio,
                "storage.reset",
                json!({ "challenge": "123456" }),
            )
            .await;
        assert_eq!(refused["ok"], json!(false));
        let challenge = server
            .dispatch(
                crate::ipc::Caller::Studio,
                "storage.resetChallenge",
                json!(null),
            )
            .await;
        assert_eq!(challenge["ok"], json!(true), "{challenge}");
        for caller in [
            crate::ipc::Caller::Cli { connection: 1 },
            crate::ipc::Caller::BrowserUi,
            crate::ipc::Caller::Panel,
        ] {
            for channel in ["storage.resetChallenge", "storage.reset"] {
                let denied = server
                    .dispatch(caller, channel, challenge["data"].clone())
                    .await;
                assert_eq!(denied["ok"], json!(false));
                assert_eq!(denied["error"]["code"], json!("PERMISSION_DENIED"));
            }
        }
        let parsed: zvs_contract::StorageResetConfirmation =
            zvs_contract::parse(challenge["data"].clone()).expect("valid challenge");
        assert_eq!(parsed.challenge.len(), 6);
    }

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
            .dispatch(
                crate::ipc::Caller::Studio,
                "storage.usage",
                json!({ "refresh": true }),
            )
            .await;
        assert_eq!(usage["ok"], json!(true), "{usage}");
        assert_eq!(
            usage["data"]["categories"].as_array().map(Vec::len),
            Some(11)
        );

        let cleared = server
            .dispatch(
                crate::ipc::Caller::Studio,
                "storage.clearCache",
                json!(null),
            )
            .await;
        assert_eq!(cleared["ok"], json!(true), "{cleared}");
        assert_eq!(cleared["data"]["removedBytes"], json!(0));
    }
}
