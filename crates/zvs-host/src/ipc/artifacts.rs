use std::sync::Arc;

use zvs_contract::{ArtifactRef, OpenArtifactInBrowserInput};

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
        )
        .handle(
            "artifacts.open",
            |services: Arc<Services>, input: ArtifactRef| async move {
                services.artifacts.open(&input.id)
            },
        )
        .handle(
            "artifacts.application",
            |services: Arc<Services>, input: ArtifactRef| async move {
                services.artifacts.application(&input.id)
            },
        )
        .handle(
            "artifacts.browsers",
            |services: Arc<Services>, _input: ()| async move {
                Ok::<_, zvs_contract::AppError>(services.artifacts.browsers())
            },
        )
        .handle(
            "artifacts.openInBrowser",
            |services: Arc<Services>, input: OpenArtifactInBrowserInput| async move {
                services
                    .artifacts
                    .open_in_browser(&input.id, &input.browser)
            },
        )
        .handle(
            "artifacts.openInApp",
            |services: Arc<Services>, input: ArtifactRef| async move {
                services.artifacts.open_in_app(&input.id).await
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
        for channel in ["artifacts.open", "artifacts.application"] {
            let escaped = server
                .dispatch(channel, json!({ "id": "../secret.pdf" }))
                .await;
            assert_eq!(
                escaped["error"]["code"],
                json!("VALIDATION_FAILED"),
                "{channel}"
            );
            let missing = server
                .dispatch(channel, json!({ "id": "run/missing.pdf" }))
                .await;
            assert_eq!(missing["error"]["code"], json!("NOT_FOUND"), "{channel}");
        }
    }

    #[tokio::test]
    async fn a_kept_file_opens_and_reports_its_application() {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        let root = host.shared().artifacts.root().to_path_buf();
        std::fs::create_dir_all(root.join("run")).expect("a run folder");
        std::fs::write(root.join("run").join("a.pdf"), b"%PDF-1.7").expect("writes");
        host.shared()
            .artifacts
            .keep("run", "r1", &[root.join("run").join("a.pdf")]);
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);
        let opened = server
            .dispatch("artifacts.open", json!({ "id": "run/a.pdf" }))
            .await;
        assert_eq!(opened["ok"], json!(true), "{opened}");
        let application = server
            .dispatch("artifacts.application", json!({ "id": "run/a.pdf" }))
            .await;
        assert_eq!(application["data"], json!({}), "{application}");
    }
}
