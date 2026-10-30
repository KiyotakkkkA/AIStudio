use regex::Regex;
use rusqlite::params;
use serde_json::json;
use zvs_contract::{ModelCapability, ProbeOutcomeDto, ProviderKind, SecretScope, Validate};

use super::*;
use crate::drivers::ai::mock::direct_http;
use crate::drivers::ai::registry::{DatabaseProviders, ProviderRegistryOptions};
use crate::platform::logger::silent_logger;
use crate::services::crypto::{CryptoService, Plaintext};
use crate::services::secret_policy::DAY_MS;
use crate::services::secrets::SecretCreateInput;
use crate::testing::{
    CLOCK_ORIGIN, FakeClock, FakeCrypto, FakeDriver, FakeDriverSource, FakeScript, MemoryLogger,
    TemporaryDatabase, temporary_database,
};

const NOW: Timestamp = CLOCK_ORIGIN;

struct Fixture {
    database: TemporaryDatabase,
    logger: Arc<MemoryLogger>,
    drivers: Arc<FakeDriverSource>,
    driver: Arc<FakeDriver>,
    secrets: Arc<SecretService>,
    service: ProviderService,
}

impl Fixture {
    fn new() -> Self {
        let database = temporary_database();
        let logger = MemoryLogger::shared();
        let driver = FakeDriver::with_models(vec![model("model-1")]);
        let drivers = FakeDriverSource::shared(driver.clone());
        let secrets = Arc::new(SecretService::new(
            database.shared(),
            Arc::new(CryptoService::new(FakeCrypto::shared())),
            Arc::new(FakeClock::default()),
            silent_logger(),
        ));
        let service = ProviderService::new(ProviderServiceOptions::new(
            database.shared(),
            drivers.clone(),
            Arc::clone(&secrets),
            logger.clone(),
            Arc::new(FakeClock::default()),
        ));
        Self {
            database,
            logger,
            drivers,
            driver,
            secrets,
            service,
        }
    }

    fn with_drivers(&self, drivers: Arc<dyn DriverSource>, timeout: u64) -> ProviderService {
        let mut options = ProviderServiceOptions::new(
            self.database.shared(),
            drivers,
            Arc::clone(&self.secrets),
            silent_logger(),
            Arc::new(FakeClock::default()),
        );
        options.probe_timeout_seconds = timeout;
        ProviderService::new(options)
    }

    fn script(&self, script: FakeScript) {
        self.driver.script(script);
    }

    fn models(&self, models: Vec<DiscoveredModel>) {
        self.script(FakeScript {
            models: Some(models),
            ..FakeScript::default()
        });
    }

    fn create(&self, input: &CreateProviderInput) -> ProviderDto {
        self.service.create(input).expect("creates")
    }

    async fn probe(&self, id: &ProviderId) -> Result<ProbeResult, AppError> {
        self.service
            .probe(ProbeInput::Saved(id.to_string()), None)
            .await
    }

    async fn probe_draft(&self, draft: impl Into<ProviderDraftInput>) -> ProbeResult {
        self.service
            .probe(ProbeInput::Draft(draft.into()), None)
            .await
            .expect("a draft probe resolves")
    }

    fn insert_account(&self, status: &str, email: Option<&str>, expires: Option<i64>) -> String {
        let id = new_id().hyphenated().to_string();
        self.database
            .with_connection(|connection| {
                connection.execute(
                    "INSERT INTO account (id, adapter, partition, external_id, email_masked, token_expires_at, status, linked_at, updated_at) VALUES (?1, 'qwen-web', 'persist:browser-work', ?2, ?3, ?4, ?5, ?6, ?6)",
                    params![id, format!("external-{id}"), email, expires, status, NOW],
                )?;
                Ok(())
            })
            .expect("inserts an account");
        id
    }

    fn insert_account_provider(&self, account_id: String) -> ProviderRow {
        let mut draft = ProviderDraft::new(
            ProviderKind::Ollama,
            "Local",
            "http://localhost:11434",
            vec![ProviderCapability::Text],
            NOW,
        );
        draft.auth_mode = AuthMode::Account;
        draft.adapter = AdapterFamily::QwenWeb;
        draft.account_id = Some(account_id);
        self.database
            .transaction(|transaction| Repositories::new(transaction).providers.create(&draft))
            .expect("inserts a provider")
    }

    fn create_secret(&self, name: &str, value: &str, rotates_at: Option<Timestamp>) -> String {
        let mut input = SecretCreateInput::new("openrouter", name, SecretScope::Personal);
        input.value = Some(Plaintext::new(value));
        input.rotates_at = rotates_at;
        self.secrets.create(input).expect("creates a secret").id
    }
}

fn model(external_id: &str) -> DiscoveredModel {
    DiscoveredModel {
        display_name: "Model One".to_owned(),
        context_window: Some(8192),
        max_output: Some(1024),
        ..DiscoveredModel::named(external_id)
    }
}

fn merged(mut base: Value, extra: Value) -> Value {
    if let (Value::Object(base), Value::Object(extra)) = (&mut base, extra) {
        base.extend(extra);
    }
    base
}

fn create_input(extra: Value) -> CreateProviderInput {
    zvs_contract::parse(merged(
        json!({
            "kind": "ollama",
            "name": "Local",
            "baseUrl": "http://localhost:11434",
            "capabilities": ["text"],
        }),
        extra,
    ))
    .expect("a valid provider input")
}

fn input() -> CreateProviderInput {
    create_input(json!({}))
}

fn update(id: ProviderId, patch: Value) -> UpdateProviderInput {
    zvs_contract::parse(merged(json!({ "id": id }), patch)).expect("a valid update")
}

fn external_ids(models: &[ModelDto]) -> Vec<&str> {
    models
        .iter()
        .map(|model| model.external_id.as_str())
        .collect()
}

#[tokio::test]
async fn selected_models_and_filter_metadata_survive_saving_and_rediscovery() {
    let fixture = Fixture::new();
    let row = fixture.create(&create_input(
        json!({ "settings": { "selectedModelIds": ["model-1", "model-2"] } }),
    ));
    let id = row.summary.id.to_string();
    fixture.models(vec![DiscoveredModel {
        is_free: Some(true),
        no_training: Some(true),
        ..model("model-1")
    }]);
    fixture.probe(&row.summary.id).await.expect("probes");
    let saved = fixture.service.get(&id, false).expect("reads");
    assert_eq!(
        saved.settings.selected_model_ids,
        Some(vec!["model-1".to_owned(), "model-2".to_owned()])
    );
    assert_eq!(saved.models[0].is_free, Some(Some(true)));
    assert_eq!(saved.models[0].no_training, Some(Some(true)));
    let dto = fixture
        .probe(&row.summary.id)
        .await
        .expect("probes")
        .to_dto()
        .expect("maps");
    dto.validate().expect("a valid probe result");
    assert_eq!(
        dto.provider.expect("a saved provider").models[0].is_free,
        Some(Some(true))
    );
    fixture
        .service
        .update(&update(
            row.summary.id,
            json!({ "settings": { "selectedModelIds": [] } }),
        ))
        .expect("updates");
    assert_eq!(
        fixture
            .service
            .get(&id, false)
            .expect("reads")
            .settings
            .selected_model_ids,
        Some(Vec::new())
    );
}

#[tokio::test]
async fn discovery_replaces_stale_models_and_preserves_or_clears_the_default_atomically() {
    let fixture = Fixture::new();
    let row = fixture.create(&input());
    let id = row.summary.id.to_string();
    let first = fixture.probe(&row.summary.id).await.expect("probes");
    assert_eq!(first.outcome.kind(), "ok");
    let provider = first.provider.expect("a saved provider");
    assert_eq!(provider.summary.last_probe_at, Some(NOW));
    assert_eq!(provider.summary.last_latency_ms, Some(0));
    let selected = fixture.service.models(&id).expect("lists")[0].clone();
    fixture
        .service
        .set_default_model(&id, Some(&selected.id.to_string()))
        .expect("sets the default");

    fixture.models(vec![model("model-1"), model("stale")]);
    fixture.probe(&row.summary.id).await.expect("probes");
    assert_eq!(
        fixture
            .service
            .get(&id, false)
            .expect("reads")
            .summary
            .default_model_id,
        Some(selected.id)
    );

    fixture.models(vec![model("replacement")]);
    fixture.probe(&row.summary.id).await.expect("probes");
    assert_eq!(
        external_ids(&fixture.service.models(&id).expect("lists")),
        ["replacement"]
    );
    assert_eq!(
        fixture
            .service
            .get(&id, false)
            .expect("reads")
            .summary
            .default_model_id,
        None
    );
    assert_eq!(
        fixture
            .logger
            .matching("Default model vanished from the vendor")
            .len(),
        1
    );

    let before = fixture.service.get(&id, false).expect("reads");
    fixture.models(vec![model("model-1"), model("model-1")]);
    assert!(fixture.probe(&row.summary.id).await.is_err());
    assert_eq!(fixture.service.get(&id, false).expect("reads"), before);
}

#[tokio::test]
async fn empty_success_clears_discovery_while_failed_probes_retain_models() {
    let fixture = Fixture::new();
    let row = fixture.create(&input());
    let id = row.summary.id.to_string();
    fixture.probe(&row.summary.id).await.expect("probes");
    fixture.script(FakeScript {
        fail_with: Some(AppError::new(
            AppErrorCode::ProviderAuthFailed,
            "private payload",
        )),
        ..FakeScript::default()
    });
    let failure = fixture.probe(&row.summary.id).await.expect("probes");
    assert_eq!(failure.derived.status, ProviderStatus::Failed);
    assert_eq!(fixture.service.models(&id).expect("lists").len(), 1);

    fixture.drivers.set(FakeDriver::with_models(Vec::new()));
    let empty = fixture.probe(&row.summary.id).await.expect("probes");
    assert_eq!(empty.outcome.kind(), "ok-empty");
    assert_eq!(empty.derived.status, ProviderStatus::Ok);
    assert!(fixture.service.models(&id).expect("lists").is_empty());
}

#[tokio::test]
async fn each_failure_code_maps_to_its_kind_without_exposing_vendor_messages() {
    for (code, kind, details) in [
        (AppErrorCode::ProviderAuthFailed, "auth-failed", json!({})),
        (
            AppErrorCode::ProviderSessionExpired,
            "session-expired",
            json!({}),
        ),
        (
            AppErrorCode::ProviderSessionExpired,
            "account-not-linked",
            json!({ "reason": "account-not-linked" }),
        ),
        (AppErrorCode::ProviderUnreachable, "unreachable", json!({})),
        (
            AppErrorCode::RateLimited,
            "rate-limited",
            json!({ "retryAfter": 12 }),
        ),
        (AppErrorCode::Unknown, "error", json!({})),
    ] {
        let fixture = Fixture::new();
        let mut error = AppError::new(code, "private payload");
        if let Value::Object(details) = details {
            error = error.with_details(details);
        }
        fixture.script(FakeScript {
            fail_with: Some(error),
            ..FakeScript::default()
        });
        let result = fixture.probe_draft(input()).await.to_dto().expect("maps");
        assert_eq!(result.outcome.kind(), kind);
        assert_eq!(result.status, ProviderStatus::Failed);
        assert!(
            result
                .status_detail
                .as_ref()
                .is_some_and(|detail| !detail.is_empty())
        );
        let wire = serde_json::to_string(&result).expect("serialises");
        assert!(!wire.contains("private payload"), "{wire}");
        result.validate().expect("a valid probe result");
        if kind == "rate-limited" {
            assert_eq!(
                result.outcome,
                ProbeOutcomeDto::RateLimited {
                    retry_after: Some(12)
                }
            );
        }
        let logged = fixture.logger.matching("Probe did not succeed");
        assert_eq!(logged.len(), 1);
        assert_eq!(logged[0].fields["kind"], json!(kind));
    }
}

#[tokio::test]
async fn draft_discovery_does_not_write_and_dtos_contain_references_only() {
    let fixture = Fixture::new();
    let secret_id = fixture.create_secret("Main key", "sk-secret-value", None);
    let draft = create_input(json!({ "secretId": secret_id }));
    let everything = ProviderListFilter::default();
    let before = fixture.service.list(&everything).expect("lists");
    let result = fixture
        .probe_draft(draft.clone())
        .await
        .to_dto()
        .expect("maps");
    assert_eq!(result.provider_id, None);
    assert_eq!(result.provider, None);
    assert_eq!(fixture.service.list(&everything).expect("lists"), before);
    let saved = fixture.create(&draft);
    assert_eq!(saved.secret_name.as_deref(), Some("Main key"));
    assert!(
        !serde_json::to_string(&saved)
            .expect("serialises")
            .contains("sk-secret-value")
    );
    assert_eq!(saved.summary.status, ProviderStatus::Unknown);
}

#[tokio::test]
async fn account_expiry_uses_seconds_from_the_account_row_and_displays_a_masked_identity() {
    let fixture = Fixture::new();
    let expires = (NOW + 2 * DAY_MS) / 1000;
    let account_id = fixture.insert_account("linked", Some("a***@example.com"), Some(expires));
    let row = fixture.insert_account_provider(account_id);
    let result = fixture
        .service
        .probe(ProbeInput::Saved(row.id), None)
        .await
        .expect("probes");
    assert_eq!(
        result.derived,
        StatusDerivation {
            status: ProviderStatus::Degraded,
            detail: Some("Сессия истекает через 2 дн.".to_owned()),
        }
    );
    assert_eq!(
        result.provider.expect("saved").account_label.as_deref(),
        Some("a***@example.com")
    );
}

#[tokio::test]
async fn the_registry_distinguishes_unlinked_and_expired_accounts_without_calling_a_vendor() {
    let fixture = Fixture::new();
    let registry = Arc::new(ProviderRegistry::new(ProviderRegistryOptions::new(
        Arc::new(DatabaseProviders::new(fixture.database.shared())),
        fixture.secrets.clone(),
        direct_http(),
    )));
    let actual = fixture.with_drivers(registry, DEFAULT_PROBE_TIMEOUT_SECONDS);
    let draft = create_input(json!({ "authMode": "account", "adapter": "qwen-web" }));
    let unlinked = actual
        .probe(ProbeInput::Draft(draft.into()), None)
        .await
        .expect("probes");
    assert_eq!(unlinked.outcome.kind(), "account-not-linked");

    let account_id = fixture.insert_account("needs-relink", None, None);
    let row = fixture.insert_account_provider(account_id);
    let expired = actual
        .probe(ProbeInput::Saved(row.id.clone()), None)
        .await
        .expect("probes");
    assert_eq!(expired.outcome.kind(), "session-expired");
    let listed = actual.list(&ProviderListFilter::default()).expect("lists");
    assert_eq!(listed[0].status, ProviderStatus::Failed);
    assert_eq!(
        listed[0].status_detail.as_deref(),
        Some("Сессия истекла — войдите заново")
    );
    actual.dispose().await;
}

#[tokio::test]
async fn concurrent_probes_join_and_connection_mutations_cannot_race_discovery() {
    let fixture = Fixture::new();
    let row = fixture.create(&input());
    let id = row.summary.id.to_string();
    fixture.script(FakeScript {
        manual: Some(true),
        ..FakeScript::default()
    });
    let first = fixture.service.probe(ProbeInput::Saved(id.clone()), None);
    let second = fixture.service.probe(ProbeInput::Saved(id.clone()), None);
    assert!(fixture.service.is_probing(&id));
    fixture.driver.when_holding().await;
    let refused = fixture
        .service
        .update(&update(
            row.summary.id,
            json!({ "baseUrl": "http://elsewhere" }),
        ))
        .expect_err("refuses");
    assert_eq!(refused.code, AppErrorCode::Conflict);
    assert_eq!(
        fixture.service.remove(&id).expect_err("refuses").code,
        AppErrorCode::Conflict
    );
    fixture.driver.release();
    let (first, second) = futures::join!(first, second);
    assert_eq!(first.expect("probes"), second.expect("joins"));
    assert_eq!(fixture.driver.list_calls(), 1);
    assert_eq!(fixture.service.models(&id).expect("lists").len(), 1);
    assert!(!fixture.service.is_probing(&id));
}

#[tokio::test(start_paused = true)]
async fn the_probe_timeout_includes_a_stalled_driver_factory() {
    let fixture = Fixture::new();
    let actual = fixture.with_drivers(FakeDriverSource::stalled(), 1);
    let result = actual
        .probe(ProbeInput::Draft(input().into()), None)
        .await
        .expect("resolves");
    assert_eq!(result.outcome.kind(), "unreachable");
    actual.dispose().await;
}

#[tokio::test(start_paused = true)]
async fn a_per_provider_timeout_overrides_the_default() {
    let fixture = Fixture::new();
    let actual = fixture.with_drivers(FakeDriverSource::stalled(), 600);
    let started = tokio::time::Instant::now();
    let result = actual
        .probe(
            ProbeInput::Draft(create_input(json!({ "settings": { "timeoutSeconds": 2 } })).into()),
            None,
        )
        .await
        .expect("resolves");
    assert_eq!(result.outcome.kind(), "unreachable");
    assert_eq!(started.elapsed(), Duration::from_secs(2));
}

#[tokio::test]
async fn an_aborted_signal_cancels_the_probe_without_writing_a_status() {
    let fixture = Fixture::new();
    let row = fixture.create(&input());
    let id = row.summary.id.to_string();
    fixture.script(FakeScript {
        manual: Some(true),
        ..FakeScript::default()
    });
    let signal = CancellationToken::new();
    let pending = fixture
        .service
        .probe(ProbeInput::Saved(id.clone()), Some(signal.clone()));
    fixture.driver.when_holding().await;
    signal.cancel();
    assert_eq!(
        pending.await.expect_err("cancelled").code,
        AppErrorCode::RunCancelled
    );
    let stored = fixture.service.get(&id, false).expect("reads");
    assert_eq!(stored.summary.status, ProviderStatus::Unknown);
    assert_eq!(stored.summary.last_probe_at, None);
    assert!(!fixture.service.is_probing(&id));
}

#[tokio::test]
async fn disposing_cancels_a_pending_probe_and_refuses_new_ones() {
    let fixture = Fixture::new();
    let row = fixture.create(&input());
    fixture.script(FakeScript {
        manual: Some(true),
        ..FakeScript::default()
    });
    let pending = fixture
        .service
        .probe(ProbeInput::Saved(row.summary.id.to_string()), None);
    fixture.driver.when_holding().await;
    fixture.service.dispose().await;
    assert_eq!(
        pending.await.expect_err("cancelled").code,
        AppErrorCode::RunCancelled
    );
    assert_eq!(
        fixture
            .probe(&row.summary.id)
            .await
            .expect_err("refused")
            .code,
        AppErrorCode::RunCancelled
    );
    assert_eq!(
        fixture
            .service
            .get(&row.summary.id.to_string(), false)
            .expect("reads")
            .summary
            .status,
        ProviderStatus::Unknown
    );
}

#[tokio::test]
async fn saved_probes_apply_secret_rotation() {
    let fixture = Fixture::new();
    let secret_id = fixture.create_secret("Expiring", "sk-key", Some(NOW - DAY_MS));
    let row = fixture.create(&create_input(json!({ "secretId": secret_id })));
    assert_eq!(
        fixture
            .probe(&row.summary.id)
            .await
            .expect("probes")
            .derived
            .status,
        ProviderStatus::Degraded
    );
}

#[tokio::test]
async fn crud_trims_names_normalises_urls_and_invalidates_the_driver() {
    let fixture = Fixture::new();
    let created = fixture.create(&create_input(json!({
        "name": "  Spaced  ",
        "baseUrl": "  http://local\thost:11434\n",
        "settings": { "temperature": 0.7, "timeoutSeconds": 60 },
        "capabilities": ["text", "embedding", "text"],
    })));
    assert_eq!(created.summary.name, "Spaced");
    assert_eq!(created.base_url, "http://localhost:11434");
    assert_eq!(
        created.summary.capabilities,
        [ProviderCapability::Text, ProviderCapability::Embedding]
    );
    assert_eq!(created.settings.temperature, Some(0.7));
    assert_eq!(
        created.adapter_capabilities.family,
        AdapterFamily::OpenaiCompatible
    );
    let id = created.summary.id.to_string();
    let renamed = fixture
        .service
        .update(&update(created.summary.id, json!({ "name": " Renamed " })))
        .expect("updates");
    assert_eq!(renamed.summary.name, "Renamed");
    assert_eq!(renamed.settings, created.settings);
    assert_eq!(fixture.drivers.invalidated(), std::slice::from_ref(&id));
    assert!(
        fixture
            .service
            .list(&ProviderListFilter {
                capability: Some(ProviderCapability::Image),
                enabled: None,
            })
            .expect("lists")
            .is_empty()
    );
    fixture.service.remove(&id).expect("removes");
    assert_eq!(fixture.drivers.invalidated(), [id.clone(), id.clone()]);
    assert_eq!(
        fixture.service.get(&id, false).expect_err("gone").code,
        AppErrorCode::NotFound
    );
    let missing = fixture
        .service
        .set_default_model(&id, None)
        .expect_err("missing");
    assert_eq!(missing.code, AppErrorCode::NotFound);
    assert_eq!(
        missing
            .details
            .and_then(|details| details.get("providerId").cloned()),
        Some(json!(id))
    );
    assert_eq!(fixture.logger.matching("Created a provider").len(), 1);
    assert_eq!(fixture.logger.matching("Removed a provider").len(), 1);
}

#[tokio::test]
async fn selected_only_reads_list_just_the_chosen_models() {
    let fixture = Fixture::new();
    let row = fixture.create(&create_input(
        json!({ "settings": { "selectedModelIds": ["b"] } }),
    ));
    fixture.models(vec![
        DiscoveredModel {
            capabilities: vec![ModelCapability::Tools],
            ..model("a")
        },
        model("b"),
    ]);
    fixture.probe(&row.summary.id).await.expect("probes");
    let id = row.summary.id.to_string();
    assert_eq!(
        external_ids(&fixture.service.get(&id, true).expect("reads").models),
        ["b"]
    );
    let all = fixture.service.get(&id, false).expect("reads");
    assert_eq!(external_ids(&all.models), ["a", "b"]);
    assert_eq!(all.summary.model_count, 2);
    assert_eq!(all.models[0].capabilities, [ModelCapability::Tools]);
}

#[test]
fn the_provider_service_stays_auth_mode_agnostic() {
    let source = include_str!("../providers.rs");
    assert!(
        !Regex::new(r"match\s+[^{]*auth_?mode")
            .expect("regex")
            .is_match(source)
    );
    assert!(
        !Regex::new(r"auth_?mode\(?\)?\s*(?:==|!=)")
            .expect("regex")
            .is_match(source)
    );
    assert!(!source.contains("AuthMode::Api"));
    assert!(!source.contains("AuthMode::Account"));
}

#[test]
fn adapter_descriptors_list_every_family_with_its_wiring_state() {
    let descriptors = adapter_descriptors();
    assert_eq!(
        descriptors
            .iter()
            .map(|descriptor| (descriptor.capabilities.family, descriptor.implemented))
            .collect::<Vec<_>>(),
        [
            (AdapterFamily::OpenaiCompatible, true),
            (AdapterFamily::Openai, true),
            (AdapterFamily::QwenWeb, true),
            (AdapterFamily::ZaiWeb, true),
            (AdapterFamily::Local, true),
        ]
    );
    let local = &descriptors
        .iter()
        .find(|descriptor| descriptor.capabilities.family == AdapterFamily::Local)
        .expect("the local family")
        .capabilities;
    assert!(local.embedding);
    assert!(!local.streaming);
}

struct CredentialCheckingDriver {
    credentials: Arc<crate::drivers::ai::identity::ProbeAccountCredentials>,
}

#[async_trait]
impl crate::drivers::ai::ports::TextGenerationDriver for CredentialCheckingDriver {
    fn capabilities(&self) -> AdapterCapabilities {
        crate::testing::FAKE_CAPABILITIES
    }

    async fn generate(
        &self,
        _request: &crate::drivers::ai::ports::GenerateRequest,
        _cancel: &CancellationToken,
    ) -> Result<crate::drivers::ai::ports::GenerateResult, AppError> {
        Err(cancelled())
    }

    fn stream(
        &self,
        _request: &crate::drivers::ai::ports::GenerateRequest,
        _cancel: &CancellationToken,
    ) -> crate::drivers::ai::ports::DeltaStream {
        Box::pin(futures::stream::empty())
    }

    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        use crate::drivers::ai::transport::AccountCredentials;
        self.credentials.current(cancel).await?;
        Ok(vec![model("model-1")])
    }
}

struct CredentialCheckingDrivers(Arc<CredentialCheckingDriver>);

#[async_trait]
impl DriverSource for CredentialCheckingDrivers {
    async fn ephemeral_driver(&self, _row: &ProviderRow) -> Result<AiDriver, AppError> {
        Ok(AiDriver {
            text: Some(self.0.clone()),
            embedding: None,
            image: None,
        })
    }

    fn invalidate(&self, _provider_id: &str) {}
}

#[tokio::test]
async fn account_draft_credentials_verify_identity_without_changing_the_database() {
    use crate::data::repositories::{AccountDraft, SecretFilter};
    use crate::drivers::ai::identity::ProbeAccountCredentials;
    use crate::testing::{FakeIdentityProbes, FakeSessionGateway, ProbeStep, fake_identity};
    use zvs_contract::AccountFamily;

    let fixture = Fixture::new();
    let account = fixture
        .database
        .transaction(|transaction| {
            Ok(Repositories::new(transaction)
                .accounts
                .create(&AccountDraft::new(
                    AccountFamily::QwenWeb,
                    "persist:browser-work",
                    "same-id",
                    NOW,
                ))?)
        })
        .expect("an account");
    let probes = FakeIdentityProbes::shared();
    probes.always(ProbeStep::Identity(fake_identity(
        "same-id",
        "private-token",
    )));
    let credentials = Arc::new(ProbeAccountCredentials::with_probe(
        account.clone(),
        FakeSessionGateway::shared(),
        fixture.secrets.clone(),
        (probes.factory())(AccountFamily::QwenWeb),
    ));
    let actual = fixture.with_drivers(
        Arc::new(CredentialCheckingDrivers(Arc::new(
            CredentialCheckingDriver { credentials },
        ))),
        DEFAULT_PROBE_TIMEOUT_SECONDS,
    );
    let draft = create_input(json!({
        "adapter": "qwen-web",
        "authMode": "account",
        "accountId": account.id,
    }));
    let result = actual
        .probe(ProbeInput::Draft(draft.clone().into()), None)
        .await
        .expect("probes")
        .to_dto()
        .expect("a dto");
    assert_eq!(result.outcome.kind(), "ok");
    assert!(
        !serde_json::to_string(&result)
            .expect("serialises")
            .contains("private-token")
    );
    let stored = |id: &str| {
        fixture
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).accounts.find_by_id(id)?)
            })
            .expect("reads")
    };
    assert_eq!(stored(&account.id), Some(account.clone()));
    assert!(
        fixture
            .secrets
            .list(&SecretFilter::default())
            .expect("lists")
            .is_empty()
    );
    assert!(
        fixture
            .service
            .list(&ProviderListFilter::default())
            .expect("lists")
            .is_empty()
    );
    probes.always(ProbeStep::Identity(fake_identity(
        "different-id",
        "private-token",
    )));
    let mismatch = actual
        .probe(ProbeInput::Draft(draft.into()), None)
        .await
        .expect("probes");
    assert_eq!(mismatch.outcome.kind(), "session-expired");
    assert_eq!(stored(&account.id), Some(account));
    actual.dispose().await;
}
