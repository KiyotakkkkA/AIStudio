use std::sync::Arc;

use zvs_contract::{
    AppError, CreateProviderInput, ProbeRequest, ProviderListFilter, ProviderRef,
    ProvidersAdaptersOutput, ProvidersGetInput, ProvidersRemoveOutput, SetDefaultModelInput,
    UpdateProviderInput,
};

use super::IpcServer;
use crate::services::providers::adapter_descriptors;
use crate::services::{ProbeInput, Services};

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "providers.usage",
            |services: Arc<Services>, input: ProviderRef| async move {
                services.providers.usage(&input.id.to_string()).await
            },
        )
        .handle(
            "providers.adapters",
            |_services: Arc<Services>, _input: ()| async move {
                Ok::<ProvidersAdaptersOutput, AppError>(adapter_descriptors())
            },
        )
        .handle(
            "providers.list",
            |services: Arc<Services>, input: ProviderListFilter| async move {
                services.providers.list(&input)
            },
        )
        .handle(
            "providers.get",
            |services: Arc<Services>, input: ProvidersGetInput| async move {
                services
                    .providers
                    .get(&input.id.to_string(), input.selected_only.unwrap_or(false))
            },
        )
        .handle(
            "providers.create",
            |services: Arc<Services>, input: CreateProviderInput| async move {
                services.providers.create(&input)
            },
        )
        .handle(
            "providers.update",
            |services: Arc<Services>, input: UpdateProviderInput| async move {
                services.providers.update(&input)
            },
        )
        .handle(
            "providers.remove",
            |services: Arc<Services>, input: ProviderRef| async move {
                services.providers.remove(&input.id.to_string())?;
                Ok(ProvidersRemoveOutput {
                    id: input.id,
                    removed: true,
                })
            },
        )
        .handle(
            "providers.probe",
            |services: Arc<Services>, input: ProbeRequest| async move {
                let request = match input {
                    ProbeRequest::Saved(reference) => ProbeInput::Saved(reference.id.to_string()),
                    ProbeRequest::Draft { draft } => ProbeInput::Draft(draft.into()),
                };
                services.providers.probe(request, None).await?.to_dto()
            },
        )
        .handle(
            "providers.setDefaultModel",
            |services: Arc<Services>, input: SetDefaultModelInput| async move {
                let model_id = input.model_id.map(|id| id.to_string());
                services
                    .providers
                    .set_default_model(&input.id.to_string(), model_id.as_deref())
            },
        )
        .handle(
            "providers.refreshAll",
            |services: Arc<Services>, _input: ()| async move {
                services.providers.refresh_all(true).await
            },
        );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::{Value, json};

    use super::*;
    use crate::drivers::ai::ports::DiscoveredModel;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::platform::logger::silent_logger;
    use crate::services::SystemService;
    use crate::testing::{FakeDriver, TestServices, test_services};

    fn server() -> (TestServices, IpcServer) {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        host.drivers
            .set(FakeDriver::with_models(vec![DiscoveredModel::named(
                "model-1",
            )]));
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);
        (host, server)
    }

    async fn call(server: &IpcServer, channel: &str, payload: Value) -> Value {
        let response = server.dispatch(channel, payload).await;
        assert_eq!(response["ok"], json!(true), "{channel}: {response}");
        response["data"].clone()
    }

    #[tokio::test]
    async fn provider_handlers_support_crud_filters_discovery_and_defaults() {
        let (_host, server) = server();
        let row = call(
            &server,
            "providers.create",
            json!({
                "kind": "ollama",
                "name": "Local",
                "baseUrl": "http://localhost:11434",
                "capabilities": ["text"],
            }),
        )
        .await;
        let id = row["id"].clone();
        assert_eq!(row["status"], json!("unknown"));
        assert_eq!(row["adapter"], json!("openai-compatible"));
        assert_eq!(row["settings"], json!({}));
        assert_eq!(
            call(&server, "providers.list", json!({ "capability": "image" })).await,
            json!([])
        );
        let result = call(&server, "providers.probe", json!({ "id": id })).await;
        assert_eq!(result["outcome"]["kind"], json!("ok"));
        assert_eq!(result["providerId"], id);
        let selected = result["provider"]["models"][0]["id"].clone();
        assert_eq!(
            call(
                &server,
                "providers.setDefaultModel",
                json!({ "id": id, "modelId": selected })
            )
            .await["defaultModelId"],
            selected
        );
        assert_eq!(
            call(
                &server,
                "providers.update",
                json!({ "id": id, "name": "Renamed" })
            )
            .await["name"],
            json!("Renamed")
        );
        let refreshed = call(&server, "providers.refreshAll", Value::Null).await;
        assert_eq!(refreshed.as_array().map(Vec::len), Some(1));
        assert_eq!(refreshed[0]["status"], json!("ok"));
        assert_eq!(
            call(&server, "providers.get", json!({ "id": id })).await["name"],
            json!("Renamed")
        );
        assert_eq!(
            call(&server, "providers.remove", json!({ "id": id })).await,
            json!({ "id": id, "removed": true })
        );
        assert_eq!(call(&server, "providers.list", json!({})).await, json!([]));
    }

    #[tokio::test]
    async fn a_draft_probe_writes_nothing_and_answers_with_a_null_provider() {
        let (host, server) = server();
        let result = call(
            &server,
            "providers.probe",
            json!({ "draft": { "kind": "openrouter", "baseUrl": "https://openrouter.ai/api/v1", "capabilities": ["text"] } }),
        )
        .await;
        assert_eq!(result["providerId"], Value::Null);
        assert_eq!(result["provider"], Value::Null);
        assert_eq!(result["status"], json!("ok"));
        assert!(
            host.providers
                .list(&zvs_contract::ProviderListFilter::default())
                .expect("lists")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn unknown_providers_and_bad_payloads_answer_with_the_envelope() {
        let (_host, server) = server();
        let missing = server
            .dispatch(
                "providers.get",
                json!({ "id": "0199aa11-1111-7111-8111-111111111111" }),
            )
            .await;
        assert_eq!(missing["error"]["code"], json!("NOT_FOUND"));
        assert_eq!(missing["error"]["message"], json!("Провайдер не найден"));
        for (channel, payload) in [
            (
                "providers.create",
                json!({ "kind": "ollama", "name": " ", "baseUrl": "http://x", "capabilities": ["text"] }),
            ),
            (
                "providers.create",
                json!({ "kind": "ollama", "name": "A", "baseUrl": "not a url", "capabilities": ["text"] }),
            ),
            ("providers.probe", json!({})),
            (
                "providers.setDefaultModel",
                json!({ "id": "x", "modelId": null }),
            ),
        ] {
            assert_eq!(
                server.dispatch(channel, payload).await["error"]["code"],
                json!("VALIDATION_FAILED"),
                "{channel}"
            );
        }
    }

    #[tokio::test]
    async fn the_adapter_catalogue_lists_every_family() {
        let (_host, server) = server();
        let families: Vec<Value> = call(&server, "providers.adapters", Value::Null)
            .await
            .as_array()
            .expect("a list")
            .iter()
            .map(|descriptor| json!([descriptor["family"], descriptor["implemented"]]))
            .collect();
        assert_eq!(
            families,
            [
                json!(["openai-compatible", true]),
                json!(["openai", true]),
                json!(["copilot", true]),
                json!(["qwen-web", true]),
                json!(["zai-web", true]),
                json!(["local", true]),
            ]
        );
    }
}
