use std::sync::Arc;

use zvs_contract::{
    AppError, AppErrorCode, CreateSecretInput, SecretDto, SecretFilter, SecretInput, SecretRef,
    SecretsListOutput, SecretsRemoveOutput, UpdateSecretInput,
};

use super::IpcServer;
use crate::data::repositories::SecretFilter as RepositoryFilter;
use crate::services::secrets::{
    USER_SECRET, public_not_found, to_secret_dto, to_secret_summary_dto,
};
use crate::services::{
    Plaintext, SecretCreateInput, SecretService, SecretSummary, SecretUpdateInput, Services,
};

pub const VALUE_REQUIRED: &str = "Значение секрета обязательно";

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "secrets.list",
            |services: Arc<Services>, input: SecretFilter| async move {
                list(&services.secrets, &input)
            },
        )
        .handle(
            "secrets.get",
            |services: Arc<Services>, input: SecretRef| async move {
                to_secret_dto(&require_public_secret(
                    &services.secrets,
                    &input.id.to_string(),
                )?)
            },
        )
        .handle(
            "secrets.create",
            |services: Arc<Services>, input: CreateSecretInput| async move {
                create(&services.secrets, input)
            },
        )
        .handle(
            "secrets.update",
            |services: Arc<Services>, input: UpdateSecretInput| async move {
                update(&services.secrets, input)
            },
        )
        .handle(
            "secrets.remove",
            |services: Arc<Services>, input: SecretRef| async move {
                let id = input.id.to_string();
                require_public_secret(&services.secrets, &id)?;
                services.secrets.remove(&id)?;
                Ok(SecretsRemoveOutput {
                    id: input.id,
                    removed: true,
                })
            },
        );
}

fn list(secrets: &SecretService, input: &SecretFilter) -> Result<SecretsListOutput, AppError> {
    secrets
        .list(&RepositoryFilter {
            scope: input.scope,
            r#type: Some(USER_SECRET),
            query: input.query.as_deref(),
        })?
        .iter()
        .map(to_secret_summary_dto)
        .collect()
}

fn create(secrets: &SecretService, input: CreateSecretInput) -> Result<SecretDto, AppError> {
    require_value(input.value.as_ref())?;
    let tags = input.tags();
    let created = secrets.create(SecretCreateInput {
        value: input.value.as_ref().map(Plaintext::from),
        tags,
        note: input.note,
        rotation_days: input
            .rotation_days
            .and_then(|days| i64::try_from(days).ok()),
        rotates_at: None,
        ..SecretCreateInput::new(USER_SECRET, input.name, input.scope)
    })?;
    to_secret_dto(&created)
}

fn update(secrets: &SecretService, input: UpdateSecretInput) -> Result<SecretDto, AppError> {
    let id = input.id.to_string();
    require_public_secret(secrets, &id)?;
    let updated = secrets.update(
        &id,
        SecretUpdateInput {
            name: input.name,
            scope: input.scope,
            value: input.value.as_ref().map(Plaintext::from),
            tags: input.tags,
            note: input.note.map(Some),
            rotation_days: input
                .rotation_days
                .map(|days| days.and_then(|days| i64::try_from(days).ok())),
            rotates_at: None,
        },
    )?;
    to_secret_dto(&updated)
}

fn require_public_secret(secrets: &SecretService, id: &str) -> Result<SecretSummary, AppError> {
    let secret = secrets.get(id)?;
    if secret.r#type == USER_SECRET {
        Ok(secret)
    } else {
        Err(public_not_found())
    }
}

fn require_value(value: Option<&SecretInput>) -> Result<(), AppError> {
    if value.is_none_or(|value| value.expose().trim().is_empty()) {
        Err(
            AppError::new(AppErrorCode::ValidationFailed, VALUE_REQUIRED)
                .with_detail("field", "value"),
        )
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::{Value, json};
    use zvs_contract::SecretScope;

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::platform::logger::silent_logger;
    use crate::services::SystemService;
    use crate::testing::{MemoryLogger, TestServices, test_services};

    const VALUE: &str = "osk_live_0a1b2c3d4e5f60718293a4b5c6d7e8f4f2a";
    const MISSING: &str = "01992958-f480-7000-8000-0000000000ff";

    fn host() -> TestServices {
        test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ))
    }

    fn server_for(host: &TestServices) -> IpcServer {
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);
        server
    }

    fn draft() -> Value {
        json!({
            "name": "Ollama — personal",
            "scope": "personal",
            "value": VALUE,
            "tags": ["llm"],
        })
    }

    fn with(mut base: Value, patch: Value) -> Value {
        for (key, value) in patch.as_object().cloned().expect("an object") {
            base[key] = value;
        }
        base
    }

    fn data(response: Value) -> Value {
        assert_eq!(response["ok"], json!(true), "{response}");
        response["data"].clone()
    }

    fn code(response: &Value) -> &str {
        response["error"]["code"].as_str().unwrap_or("OK")
    }

    #[tokio::test]
    async fn secrets_create_returns_the_hint_and_no_value() {
        let host = host();
        let server = server_for(&host);
        let created = data(server.dispatch("secrets.create", draft()).await);
        assert!(created.get("value").is_none());
        assert!(created.get("cipher").is_none());
        assert!(!created.to_string().contains(VALUE));
        assert_eq!(created["hint"], json!("osk_live_…4f2a"));
        assert!(created.get("fields").is_none());
        assert!(created.get("type").is_none());
        assert_eq!(created["usageCount"], json!(0));
        assert_eq!(created["rotationStatus"], json!("ok"));
        serde_json::from_value::<SecretDto>(created).expect("matches the contract");
    }

    #[tokio::test]
    async fn secrets_create_demands_a_value() {
        let host = host();
        let server = server_for(&host);
        for payload in [
            with(draft(), json!({ "value": "   " })),
            json!({ "name": "Без значения", "scope": "personal", "tags": [] }),
        ] {
            let response = server.dispatch("secrets.create", payload).await;
            assert_eq!(code(&response), "VALIDATION_FAILED");
            assert_eq!(response["error"]["message"], json!(VALUE_REQUIRED));
        }
        assert_eq!(
            data(server.dispatch("secrets.list", json!({})).await),
            json!([])
        );
    }

    #[tokio::test]
    async fn secrets_list_and_get_carry_the_hint_and_never_a_value() {
        let host = host();
        let server = server_for(&host);
        let created = data(server.dispatch("secrets.create", draft()).await);
        let listed = data(
            server
                .dispatch("secrets.list", json!({ "scope": "personal" }))
                .await,
        );
        let rows = listed.as_array().expect("an array");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["hint"], json!("osk_live_…4f2a"));
        assert!(rows[0].get("fields").is_none());
        assert!(!listed.to_string().contains(VALUE));
        serde_json::from_value::<SecretsListOutput>(listed).expect("matches the contract");

        let fetched = data(
            server
                .dispatch("secrets.get", json!({ "id": created["id"] }))
                .await,
        );
        assert_eq!(fetched, created);
        assert_eq!(
            data(
                server
                    .dispatch("secrets.list", json!({ "query": "personal" }))
                    .await
            )
            .as_array()
            .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            code(
                &server
                    .dispatch("secrets.get", json!({ "id": MISSING }))
                    .await
            ),
            "NOT_FOUND"
        );
    }

    #[tokio::test]
    async fn secrets_update_without_a_value_leaves_the_credential_alone() {
        let host = host();
        let server = server_for(&host);
        let created = data(server.dispatch("secrets.create", draft()).await);
        let encrypt_calls = host.crypto.encrypt_calls();
        let renamed = data(
            server
                .dispatch(
                    "secrets.update",
                    json!({ "id": created["id"], "name": "Ollama — work" }),
                )
                .await,
        );
        assert_eq!(renamed["name"], json!("Ollama — work"));
        assert_eq!(renamed["hint"], created["hint"]);
        assert_eq!(host.crypto.encrypt_calls(), encrypt_calls);

        let replaced = data(
            server
                .dispatch(
                    "secrets.update",
                    json!({ "id": created["id"], "value": "osk_live_ffffffffffffffffffffffffffffffff9999" }),
                )
                .await,
        );
        assert_eq!(replaced["hint"], json!("osk_live_…9999"));
        assert_eq!(
            code(
                &server
                    .dispatch("secrets.update", json!({ "id": MISSING, "name": "x" }))
                    .await
            ),
            "NOT_FOUND"
        );
    }

    #[tokio::test]
    async fn secrets_update_sets_and_clears_the_rotation_reminder() {
        let host = host();
        let server = server_for(&host);
        let created = data(server.dispatch("secrets.create", draft()).await);
        let reminded = data(
            server
                .dispatch(
                    "secrets.update",
                    json!({ "id": created["id"], "rotationDays": 7 }),
                )
                .await,
        );
        assert_eq!(reminded["rotationStatus"], json!("due-soon"));
        assert!(reminded["rotatesAt"].is_i64());
        let cleared = data(
            server
                .dispatch(
                    "secrets.update",
                    json!({ "id": created["id"], "rotationDays": null }),
                )
                .await,
        );
        assert_eq!(cleared["rotatesAt"], Value::Null);
        assert_eq!(cleared["rotationStatus"], json!("ok"));
    }

    #[tokio::test]
    async fn account_tokens_remain_host_internal_and_do_not_invalidate_the_public_list() {
        let host = host();
        let server = server_for(&host);
        let token = host
            .secrets
            .create(SecretCreateInput {
                value: Some(Plaintext::new("internal-account-credential")),
                ..SecretCreateInput::new(
                    "account-token",
                    "qwen-web account token",
                    SecretScope::Personal,
                )
            })
            .expect("creates");
        assert_eq!(
            data(server.dispatch("secrets.list", json!({})).await),
            json!([])
        );
        let visible = data(server.dispatch("secrets.create", draft()).await);
        assert_eq!(
            data(server.dispatch("secrets.list", json!({})).await)
                .as_array()
                .expect("an array")
                .iter()
                .map(|secret| secret["id"].clone())
                .collect::<Vec<_>>(),
            [visible["id"].clone()]
        );
        for (channel, payload) in [
            ("secrets.get", json!({ "id": token.id })),
            (
                "secrets.update",
                json!({ "id": token.id, "value": "replacement" }),
            ),
            ("secrets.remove", json!({ "id": token.id })),
        ] {
            let response = server.dispatch(channel, payload).await;
            assert_eq!(code(&response), "NOT_FOUND", "{channel}");
            assert!(response["error"].get("details").is_none());
        }
        assert_eq!(
            host.secrets.resolve(&token.id).expect("resolves").expose(),
            "internal-account-credential"
        );
    }

    #[tokio::test]
    async fn secrets_remove_deletes_an_unused_secret_and_names_the_consumers_of_a_used_one() {
        let host = host();
        let server = server_for(&host);
        let created = data(server.dispatch("secrets.create", draft()).await);
        let id = created["id"].as_str().expect("an id").to_owned();
        host.secrets
            .add_usage(
                &id,
                crate::data::schema::SecretConsumerKind::Provider,
                "provider-1",
            )
            .expect("adds");
        host.secrets
            .add_usage(
                &id,
                crate::data::schema::SecretConsumerKind::VectorStore,
                "store-1",
            )
            .expect("adds");
        assert_eq!(
            data(server.dispatch("secrets.get", json!({ "id": id })).await)["usageCount"],
            json!(2)
        );

        let refused = server.dispatch("secrets.remove", json!({ "id": id })).await;
        assert_eq!(code(&refused), "CONFLICT");
        assert_eq!(
            refused["error"]["details"]["consumers"],
            json!([
                { "kind": "provider", "id": "provider-1" },
                { "kind": "vector_store", "id": "store-1" },
            ])
        );

        host.secrets
            .remove_usage(
                &id,
                crate::data::schema::SecretConsumerKind::Provider,
                "provider-1",
            )
            .expect("removes");
        host.secrets
            .remove_usage(
                &id,
                crate::data::schema::SecretConsumerKind::VectorStore,
                "store-1",
            )
            .expect("removes");
        assert_eq!(
            data(server.dispatch("secrets.remove", json!({ "id": id })).await),
            json!({ "id": id, "removed": true })
        );
        assert_eq!(
            data(server.dispatch("secrets.list", json!({})).await),
            json!([])
        );
        assert_eq!(
            code(
                &server
                    .dispatch("secrets.remove", json!({ "id": MISSING }))
                    .await
            ),
            "NOT_FOUND"
        );
    }

    #[tokio::test]
    async fn no_secrets_channel_response_or_log_line_carries_the_value() {
        let host = host();
        let logger = MemoryLogger::shared();
        let mut server = IpcServer::new(host.shared(), logger.clone()).with_output_validation(true);
        register(&mut server);
        let mut responses = Vec::new();
        let created = data(server.dispatch("secrets.create", draft()).await);
        responses.push(created.clone());
        for (channel, payload) in [
            ("secrets.list", json!({})),
            ("secrets.get", json!({ "id": created["id"] })),
            (
                "secrets.update",
                json!({ "id": created["id"], "value": VALUE }),
            ),
            (
                "secrets.update",
                json!({ "id": created["id"], "value": "  " }),
            ),
            (
                "secrets.create",
                with(draft(), json!({ "note": VALUE, "value": "  " })),
            ),
            ("secrets.create", draft()),
            ("secrets.remove", json!({ "id": created["id"] })),
        ] {
            responses.push(server.dispatch(channel, payload).await);
        }
        for response in &responses {
            assert!(!response.to_string().contains(VALUE), "{response}");
        }
        for entry in logger.entries() {
            assert!(
                !Value::Object(entry.fields.clone())
                    .to_string()
                    .contains(VALUE)
                    && !entry.message.contains(VALUE),
                "{entry:?}"
            );
        }
    }
}
