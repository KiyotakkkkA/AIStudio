use std::sync::Arc;

use zvs_contract::ArtifactRef;

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "artifacts.save",
            |services: Arc<Services>, input: ArtifactRef| async move {
                services.artifacts.save(&input.id).await
            },
        )
        .handle(
            "artifacts.reveal",
            |services: Arc<Services>, input: ArtifactRef| async move {
                services.artifacts.reveal(&input.id)
            },
        );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::platform::logger::silent_logger;
    use crate::services::SystemService;
    use crate::testing::test_services;

    #[tokio::test]
    async fn the_channels_refuse_ids_outside_the_store_and_unknown_files() {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);
        let escaped = server
            .dispatch("artifacts.reveal", json!({ "id": "../secret.pdf" }))
            .await;
        assert_eq!(escaped["error"]["code"], json!("VALIDATION_FAILED"));
        let missing = server
            .dispatch("artifacts.save", json!({ "id": "run/missing.pdf" }))
            .await;
        assert_eq!(missing["error"]["code"], json!("NOT_FOUND"));
    }
}
