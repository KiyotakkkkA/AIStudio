use std::sync::Arc;

use zvs_contract::{
    AppError, InstallRuntimeInput, RuntimeRef, RuntimesRemoveOutput, RuntimesStopOutput,
};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "runtimes.list",
            |services: Arc<Services>, _input: ()| async move { services.runtimes.overview().await },
        )
        .handle(
            "runtimes.install",
            |services: Arc<Services>, input: InstallRuntimeInput| async move {
                services.runtimes.install(&input).await
            },
        )
        .handle(
            "runtimes.stop",
            |services: Arc<Services>, input: RuntimeRef| async move {
                Ok::<RuntimesStopOutput, AppError>(RuntimesStopOutput {
                    stopped: services.runtimes.stop(&input.id),
                })
            },
        )
        .handle(
            "runtimes.remove",
            |services: Arc<Services>, input: RuntimeRef| async move {
                services.runtimes.uninstall(&input.id).await?;
                Ok::<RuntimesRemoveOutput, AppError>(RuntimesRemoveOutput {
                    id: input.id,
                    removed: true,
                })
            },
        );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::{Value, json};
    use zvs_contract::RuntimeOverviewDto;

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::platform::logger::silent_logger;
    use crate::runtimes::INSTALLED_KEY;
    use crate::services::SystemService;
    use crate::testing::test_services;

    #[tokio::test]
    async fn every_runtime_channel_reaches_the_service_without_touching_the_network() {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);

        let listed = server.dispatch("runtimes.list", Value::Null).await;
        assert_eq!(listed["ok"], json!(true), "{listed}");
        let overview: RuntimeOverviewDto =
            serde_json::from_value(listed["data"].clone()).expect("an overview");
        assert!(!overview.plan.ready);
        assert_eq!(overview.runtimes.len(), 4);

        assert_eq!(
            server
                .dispatch("runtimes.stop", json!({ "id": "llama-cpp-cpu" }))
                .await,
            json!({ "ok": true, "data": { "stopped": false } })
        );
        assert_eq!(
            server
                .dispatch("runtimes.remove", json!({ "id": "llama-cpp-cpu" }))
                .await["error"]["code"],
            json!("CONFLICT")
        );

        let install_path = host.downloads_dir().join("runtimes").join("llama-cpp-cpu");
        std::fs::create_dir_all(&install_path).expect("creates");
        host.settings
            .set(
                INSTALLED_KEY,
                &json!({
                    "llama-cpp-cpu": {
                        "tag": "b1",
                        "installPath": install_path.to_string_lossy(),
                        "executablePath": install_path.join("llama-server.exe").to_string_lossy(),
                        "sizeBytes": 5,
                    }
                }),
            )
            .expect("stores");
        assert_eq!(
            server
                .dispatch("runtimes.remove", json!({ "id": "llama-cpp-cpu" }))
                .await,
            json!({ "ok": true, "data": { "id": "llama-cpp-cpu", "removed": true } })
        );
        assert!(!install_path.exists());
        assert_eq!(
            server
                .dispatch("runtimes.install", json!({ "id": "ab" }))
                .await["error"]["code"],
            json!("VALIDATION_FAILED")
        );
        assert_eq!(
            server
                .dispatch("runtimes.install", json!({ "id": "llama-cpp-nope" }))
                .await["error"]["code"],
            json!("NOT_FOUND")
        );
    }
}
