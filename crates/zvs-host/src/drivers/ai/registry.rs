use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};

use serde_json::json;
use zvs_contract::{AppError, AppErrorCode, AuthMode, ErrorDetails, ProviderKind, Timestamp};

use super::adapters::{
    AdapterContext, OLLAMA_CAPABILITIES, OllamaAdapter, SharedEmbeddingEngine, SharedModelStore,
    adapter_entry_named,
};
use super::capabilities::AdapterCapabilities;
use super::errors::session_expired;
use super::http::HttpClient;
use super::identity::ProbeAccountCredentials;
use super::ports::{AiDriver, SharedEmbeddingDriver, SharedImageDriver, SharedTextDriver};
use super::transport::{
    AccountTransport, AccountTransportOptions, ApiKey, ApiTransport, ApiTransportOptions,
    SharedSecretResolver, SharedSessionGateway, SharedTransport,
};
use crate::data::schema::{AccountRow, ProviderRow};
use crate::data::{Database, Repositories};
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};

pub trait ProviderSource: Send + Sync {
    fn find_by_id(&self, id: &str) -> Result<Option<ProviderRow>, AppError>;
}

pub struct DatabaseProviders(Arc<Database>);

impl DatabaseProviders {
    pub const fn new(database: Arc<Database>) -> Self {
        Self(database)
    }
}

impl ProviderSource for DatabaseProviders {
    fn find_by_id(&self, id: &str) -> Result<Option<ProviderRow>, AppError> {
        self.0.with_connection(|connection| {
            Ok(Repositories::new(connection).providers.find_by_id(id)?)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSnapshot {
    pub status: String,
    pub updated_at: Timestamp,
}

pub trait AccountPort: Send + Sync {
    fn snapshot(&self, account_id: &str) -> Option<AccountSnapshot>;

    fn transport(&self, provider: &ProviderRow) -> Result<SharedTransport, AppError>;
}

pub const ACCOUNT_NOT_LINKED: &str = "account-not-linked";

pub fn account_not_linked(provider: &ProviderRow) -> AppError {
    let mut details = ErrorDetails::new();
    details.insert("providerId".to_owned(), json!(provider.id));
    if let Some(account_id) = &provider.account_id {
        details.insert("accountId".to_owned(), json!(account_id));
    }
    details.insert("reason".to_owned(), json!(ACCOUNT_NOT_LINKED));
    session_expired(details)
}

pub type SessionFactory = Arc<dyn Fn(&str) -> SharedSessionGateway + Send + Sync>;

pub const BROWSER_SESSION_UNAVAILABLE: &str = "Сессия браузера недоступна";

pub struct DatabaseAccounts {
    database: Arc<Database>,
    sessions: Option<SessionFactory>,
    secrets: SharedSecretResolver,
    logger: SharedLogger,
}

impl DatabaseAccounts {
    pub fn new(
        database: Arc<Database>,
        sessions: Option<SessionFactory>,
        secrets: SharedSecretResolver,
        logger: SharedLogger,
    ) -> Self {
        Self {
            database,
            sessions,
            secrets,
            logger,
        }
    }

    fn find(&self, account_id: &str) -> Result<Option<AccountRow>, AppError> {
        self.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .accounts
                .find_by_id(account_id)?)
        })
    }

    fn linked(&self, provider: &ProviderRow) -> Result<AccountRow, AppError> {
        let Some(account_id) = &provider.account_id else {
            return Err(account_not_linked(provider));
        };
        let Some(account) = self.find(account_id)? else {
            return Err(account_not_linked(provider));
        };
        if !account.is_linked() {
            let mut details = ErrorDetails::new();
            details.insert("providerId".to_owned(), json!(provider.id));
            details.insert("accountId".to_owned(), json!(account.id));
            details.insert("status".to_owned(), json!(account.status));
            if let Some(detail) = &account.status_detail {
                details.insert("statusDetail".to_owned(), json!(detail));
            }
            return Err(session_expired(details));
        }
        Ok(account)
    }
}

impl AccountPort for DatabaseAccounts {
    fn snapshot(&self, account_id: &str) -> Option<AccountSnapshot> {
        self.find(account_id)
            .ok()
            .flatten()
            .map(|account| AccountSnapshot {
                status: account.status,
                updated_at: account.updated_at,
            })
    }

    fn transport(&self, provider: &ProviderRow) -> Result<SharedTransport, AppError> {
        let account = self.linked(provider)?;
        let Some(sessions) = &self.sessions else {
            return Err(
                AppError::new(AppErrorCode::ValidationFailed, BROWSER_SESSION_UNAVAILABLE)
                    .with_detail("accountId", account.id.clone())
                    .with_detail("partition", account.partition.clone()),
            );
        };
        let session = sessions(&account.partition);
        let credentials =
            ProbeAccountCredentials::new(account, session.clone(), Arc::clone(&self.secrets))?;
        Ok(Arc::new(AccountTransport::new(AccountTransportOptions {
            base_url: provider.base_url.clone(),
            session,
            credentials: Some(Arc::new(credentials)),
            timeout_seconds: provider.settings.timeout_seconds(),
            headers: Vec::new(),
            logger: self.logger.clone(),
        })))
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UnlinkedAccounts;

impl AccountPort for UnlinkedAccounts {
    fn snapshot(&self, _account_id: &str) -> Option<AccountSnapshot> {
        None
    }

    fn transport(&self, provider: &ProviderRow) -> Result<SharedTransport, AppError> {
        Err(account_not_linked(provider))
    }
}

pub struct ProviderRegistryOptions {
    pub providers: Arc<dyn ProviderSource>,
    pub secrets: SharedSecretResolver,
    pub accounts: Arc<dyn AccountPort>,
    pub http: HttpClient,
    pub local_models: Option<SharedModelStore>,
    pub local_engine: Option<SharedEmbeddingEngine>,
    pub logger: SharedLogger,
}

impl ProviderRegistryOptions {
    pub fn new(
        providers: Arc<dyn ProviderSource>,
        secrets: SharedSecretResolver,
        http: HttpClient,
    ) -> Self {
        Self {
            providers,
            secrets,
            accounts: Arc::new(UnlinkedAccounts),
            http,
            local_models: None,
            local_engine: None,
            logger: silent_logger(),
        }
    }
}

struct CacheEntry {
    driver: Arc<AiDriver>,
    fingerprint: String,
    secret_id: Option<String>,
    account_id: Option<String>,
}

pub struct ProviderRegistry {
    providers: Arc<dyn ProviderSource>,
    secrets: SharedSecretResolver,
    accounts: Arc<dyn AccountPort>,
    http: HttpClient,
    local_models: Option<SharedModelStore>,
    local_engine: RwLock<Option<SharedEmbeddingEngine>>,
    logger: SharedLogger,
    cache: Mutex<HashMap<String, CacheEntry>>,
}

fn unsupported(provider_id: &str, capability: &str) -> AppError {
    AppError::new(
        AppErrorCode::Conflict,
        "Провайдер не поддерживает эту возможность",
    )
    .with_detail("providerId", provider_id)
    .with_detail("capability", capability)
}

fn fingerprint_of(row: &ProviderRow) -> String {
    [
        format!("adapter={}", row.adapter),
        format!("auth={}", row.auth_mode),
        format!("base={}", row.base_url),
        format!("secret={}", row.secret_id.as_deref().unwrap_or_default()),
        format!("timeout={}", row.settings.timeout_text()),
        format!("updated={}", row.updated_at),
    ]
    .join("|")
}

impl ProviderRegistry {
    pub fn new(options: ProviderRegistryOptions) -> Self {
        Self {
            providers: options.providers,
            secrets: options.secrets,
            accounts: options.accounts,
            http: options.http,
            local_models: options.local_models,
            local_engine: RwLock::new(options.local_engine),
            logger: options.logger,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn cache(&self) -> MutexGuard<'_, HashMap<String, CacheEntry>> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn size(&self) -> usize {
        self.cache().len()
    }

    pub fn attach_local_engine(&self, engine: SharedEmbeddingEngine) {
        *self
            .local_engine
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(engine);
        self.clear();
    }

    pub fn capabilities(&self, provider_id: &str) -> Result<AdapterCapabilities, AppError> {
        let row = self.row(provider_id)?;
        if !row.is_local() && row.kind == ProviderKind::Ollama {
            return Ok(OLLAMA_CAPABILITIES);
        }
        Ok(adapter_entry_named(&row.adapter)?.capabilities)
    }

    pub fn driver(&self, provider_id: &str) -> Result<Arc<AiDriver>, AppError> {
        let row = self.row(provider_id)?;
        let fingerprint = self.fingerprint(&row);
        if let Some(cached) = self
            .cache()
            .get(provider_id)
            .filter(|cached| cached.fingerprint == fingerprint)
        {
            return Ok(Arc::clone(&cached.driver));
        }
        let driver = Arc::new(self.ephemeral_driver(&row)?);
        self.cache().insert(
            provider_id.to_owned(),
            CacheEntry {
                driver: Arc::clone(&driver),
                fingerprint,
                secret_id: row.secret_id.clone(),
                account_id: row.account_id.clone(),
            },
        );
        Ok(driver)
    }

    pub fn ephemeral_driver(&self, row: &ProviderRow) -> Result<AiDriver, AppError> {
        let local = row.is_local();
        if row.kind == ProviderKind::Ollama && !local {
            let transport = self.transport(row)?;
            return Ok(AiDriver {
                text: Some(Arc::new(OllamaAdapter::new(transport, self.logger.clone()))),
                embedding: None,
                image: None,
            });
        }
        let entry = adapter_entry_named(&row.adapter)?;
        if !local {
            Self::require_auth_mode(&entry.capabilities, row)?;
        }
        let context = AdapterContext {
            transport: if local {
                None
            } else {
                Some(self.transport(row)?)
            },
            logger: self.logger.clone(),
            local_models: self.local_models.clone(),
            local_engine: self
                .local_engine
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
        };
        entry.build(&context)
    }

    pub fn text(&self, provider_id: &str) -> Result<SharedTextDriver, AppError> {
        self.driver(provider_id)?
            .text
            .clone()
            .ok_or_else(|| unsupported(provider_id, "text"))
    }

    pub fn embedding(&self, provider_id: &str) -> Result<SharedEmbeddingDriver, AppError> {
        self.driver(provider_id)?
            .embedding
            .clone()
            .ok_or_else(|| unsupported(provider_id, "embedding"))
    }

    pub fn image(&self, provider_id: &str) -> Result<SharedImageDriver, AppError> {
        self.driver(provider_id)?
            .image
            .clone()
            .ok_or_else(|| unsupported(provider_id, "image"))
    }

    pub fn invalidate(&self, provider_id: &str) {
        let dropped = self.cache().remove(provider_id).is_some();
        if dropped {
            self.logger.log(
                LogLevel::Debug,
                "ai",
                "Dropped a cached provider driver",
                &[("providerId", json!(provider_id))],
            );
        }
    }

    pub fn invalidate_by_secret(&self, secret_id: &str) {
        for provider_id in self.cached_where(|entry| entry.secret_id.as_deref() == Some(secret_id))
        {
            self.invalidate(&provider_id);
        }
    }

    pub fn invalidate_by_account(&self, account_id: &str) {
        for provider_id in
            self.cached_where(|entry| entry.account_id.as_deref() == Some(account_id))
        {
            self.invalidate(&provider_id);
        }
    }

    pub fn clear(&self) {
        self.cache().clear();
    }

    fn cached_where(&self, keep: impl Fn(&CacheEntry) -> bool) -> Vec<String> {
        self.cache()
            .iter()
            .filter(|(_, entry)| keep(entry))
            .map(|(provider_id, _)| provider_id.clone())
            .collect()
    }

    fn transport(&self, row: &ProviderRow) -> Result<SharedTransport, AppError> {
        match row.auth() {
            Some(AuthMode::Api) => Ok(Arc::new(ApiTransport::new(ApiTransportOptions {
                base_url: row.base_url.clone(),
                api_key: row
                    .secret_id
                    .as_ref()
                    .map(|secret_id| ApiKey::new(secret_id.clone(), Arc::clone(&self.secrets))),
                timeout_seconds: row.settings.timeout_seconds(),
                headers: Vec::new(),
                http: self.http.clone(),
                logger: self.logger.clone(),
            }))),
            Some(AuthMode::Account) => self.accounts.transport(row),
            None => Err(AppError::new(
                AppErrorCode::ValidationFailed,
                "Неизвестный режим авторизации",
            )
            .with_detail("providerId", row.id.clone())
            .with_detail("authMode", row.auth_mode.clone())),
        }
    }

    fn fingerprint(&self, row: &ProviderRow) -> String {
        let account = row
            .account_id
            .as_deref()
            .and_then(|account_id| self.accounts.snapshot(account_id));
        [
            fingerprint_of(row),
            format!("account={}", row.account_id.as_deref().unwrap_or_default()),
            format!(
                "accountStatus={}",
                account
                    .as_ref()
                    .map_or("", |account| account.status.as_str())
            ),
            format!(
                "accountUpdated={}",
                account
                    .as_ref()
                    .map_or_else(String::new, |account| account.updated_at.to_string())
            ),
        ]
        .join("|")
    }

    fn require_auth_mode(
        capabilities: &AdapterCapabilities,
        row: &ProviderRow,
    ) -> Result<(), AppError> {
        if row
            .auth()
            .is_some_and(|mode| capabilities.supports_auth_mode(mode))
        {
            return Ok(());
        }
        Err(AppError::new(
            AppErrorCode::ValidationFailed,
            format!(
                "adapter {} cannot run in {} mode",
                capabilities.family, row.auth_mode
            ),
        )
        .with_detail("providerId", row.id.clone())
        .with_detail("family", capabilities.family.as_str())
        .with_detail("authMode", row.auth_mode.clone()))
    }

    fn row(&self, provider_id: &str) -> Result<ProviderRow, AppError> {
        let row = self.providers.find_by_id(provider_id)?.ok_or_else(|| {
            AppError::new(AppErrorCode::NotFound, "Провайдер не найден")
                .with_detail("providerId", provider_id)
        })?;
        if !row.enabled {
            return Err(AppError::new(AppErrorCode::Conflict, "Провайдер отключён")
                .with_detail("providerId", provider_id));
        }
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use serde_json::Value;
    use tokio_util::sync::CancellationToken;
    use zvs_contract::{
        AdapterFamily, LOCAL_BASE_URL, ProviderCapability, ProviderStatus, SecretScope,
    };

    use super::*;
    use crate::data::repositories::{ProviderDraft, ProviderPatch};
    use crate::data::schema::ProviderSettings;
    use crate::drivers::ai::adapters::{
        FileSystemModelStore, LOCAL_CAPABILITIES, LocalEmbeddingEngine,
        OPENAI_COMPATIBLE_CAPABILITIES,
    };
    use crate::drivers::ai::mock::{CountingKey, MockServer, Reply, direct_http, transport_for};
    use crate::drivers::ai::transport::SecretResolver;
    use crate::services::crypto::{CryptoService, Plaintext};
    use crate::services::secrets::{SecretCreateInput, SecretService, SecretUpdateInput};
    use crate::testing::{
        CLOCK_ORIGIN, FakeClock, FakeCrypto, MemoryLogger, TemporaryDatabase, temporary_database,
        temporary_directory,
    };

    const NOW: Timestamp = CLOCK_ORIGIN;

    struct Fixture {
        database: TemporaryDatabase,
        key: Arc<CountingKey>,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                database: temporary_database(),
                key: CountingKey::shared("sk-test"),
            }
        }

        fn registry(&self) -> ProviderRegistry {
            ProviderRegistry::new(ProviderRegistryOptions::new(
                Arc::new(DatabaseProviders::new(self.database.shared())),
                self.key.clone(),
                direct_http(),
            ))
        }

        fn create(&self, draft: &ProviderDraft) -> ProviderRow {
            self.database
                .with_connection(|connection| Repositories::new(connection).providers.create(draft))
                .expect("creates")
        }

        fn update(&self, id: &str, patch: &ProviderPatch) -> ProviderRow {
            self.database
                .with_connection(|connection| {
                    Ok(Repositories::new(connection)
                        .providers
                        .update(id, patch, NOW)?
                        .expect("a row"))
                })
                .expect("updates")
        }

        fn sql(&self, statement: &str, id: &str) {
            self.database
                .with_connection(|connection| Ok(connection.execute(statement, [id])?))
                .expect("runs");
        }
    }

    fn draft() -> ProviderDraft {
        ProviderDraft::new(
            ProviderKind::Ollama,
            "Local",
            "http://localhost:11434/v1",
            vec![ProviderCapability::Text],
            100,
        )
    }

    fn router(base_url: String) -> ProviderDraft {
        ProviderDraft {
            kind: ProviderKind::Openrouter,
            name: "Router".to_owned(),
            base_url,
            ..draft()
        }
    }

    #[test]
    fn the_registry_builds_a_driver_per_adapter_value_caches_it_and_invalidates_on_change() {
        let fixture = Fixture::new();
        let row = fixture.create(&draft());
        let providers = fixture.registry();
        let first = providers.driver(&row.id).expect("builds");
        assert!(Arc::ptr_eq(
            &first,
            &providers.driver(&row.id).expect("cached")
        ));
        assert_eq!(providers.size(), 1);
        assert!(first.text.is_some());
        assert!(first.embedding.is_none());

        fixture.update(
            &row.id,
            &ProviderPatch {
                base_url: Some("http://localhost:11434/v1/".to_owned()),
                updated_at: Some(200),
                ..ProviderPatch::default()
            },
        );
        let second = providers.driver(&row.id).expect("rebuilds");
        assert!(!Arc::ptr_eq(&first, &second));

        providers.invalidate(&row.id);
        assert_eq!(providers.size(), 0);
        assert!(!Arc::ptr_eq(
            &second,
            &providers.driver(&row.id).expect("rebuilds")
        ));

        let stray = fixture.create(&ProviderDraft {
            kind: ProviderKind::Openrouter,
            name: "Stray".to_owned(),
            ..draft()
        });
        fixture.sql(
            "UPDATE provider SET adapter = 'made-up' WHERE id = ?1",
            &stray.id,
        );
        let error = providers.driver(&stray.id).expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::Unknown);
        assert_eq!(error.message, "adapter made-up is not implemented yet");
        assert_eq!(fixture.key.calls(), 0);
    }

    #[tokio::test]
    async fn the_registry_resolves_the_secret_at_point_of_use_and_drops_the_driver_when_it_changes()
    {
        let server = MockServer::start().await;
        server.reply(
            "/api/v1/models",
            Reply::json(json!({ "data": [{ "id": "openai/gpt-4o" }] })),
        );
        let fixture = Fixture::new();
        let crypto = FakeCrypto::shared();
        let secrets = Arc::new(SecretService::new(
            fixture.database.shared(),
            Arc::new(CryptoService::new(crypto.clone())),
            Arc::new(FakeClock::default()),
            silent_logger(),
        ));
        let key = secrets
            .create(SecretCreateInput {
                value: Some(Plaintext::new("sk-first")),
                ..SecretCreateInput::new("custom", "Key", SecretScope::Personal)
            })
            .expect("a secret");
        let row = fixture.create(&ProviderDraft {
            secret_id: Some(key.id.clone()),
            ..router(server.url("/api/v1"))
        });
        let providers = ProviderRegistry::new(ProviderRegistryOptions::new(
            Arc::new(DatabaseProviders::new(fixture.database.shared())),
            secrets.clone(),
            direct_http(),
        ));
        let cancel = CancellationToken::new();

        let text = providers.text(&row.id).expect("a text driver");
        assert_eq!(crypto.decrypt_calls(), 0);
        let models = text.list_models(&cancel).await.expect("lists");
        assert_eq!(
            models
                .iter()
                .map(|model| model.external_id.as_str())
                .collect::<Vec<_>>(),
            ["openai/gpt-4o"]
        );
        assert_eq!(
            server.last().header("authorization").as_deref(),
            Some("Bearer sk-first")
        );
        assert_eq!(crypto.decrypt_calls(), 1);

        secrets
            .update(
                &key.id,
                SecretUpdateInput {
                    value: Some(Plaintext::new("sk-second")),
                    ..SecretUpdateInput::default()
                },
            )
            .expect("updates");
        text.list_models(&cancel).await.expect("lists");
        assert_eq!(
            server.last().header("authorization").as_deref(),
            Some("Bearer sk-second")
        );
        assert_eq!(crypto.decrypt_calls(), 2);

        providers.invalidate_by_secret(&key.id);
        assert_eq!(providers.size(), 0);
        let refreshed = providers.text(&row.id).expect("a text driver");
        refreshed.list_models(&cancel).await.expect("lists");
        assert_eq!(
            server.last().header("authorization").as_deref(),
            Some("Bearer sk-second")
        );
    }

    #[tokio::test]
    async fn no_plaintext_is_resolved_to_build_a_driver_and_none_outlives_a_call() {
        let server = MockServer::start().await;
        server.reply("/v1/models", Reply::json(json!({ "data": [] })));
        let fixture = Fixture::new();
        let key = fixture
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .secrets
                    .create(&crate::data::repositories::SecretDraft {
                        id: None,
                        r#type: "custom".to_owned(),
                        name: "Key".to_owned(),
                        scope: SecretScope::Personal,
                        cipher: None,
                        cipher_version: 1,
                        hint: None,
                        fields: "{}".to_owned(),
                        tags: "[]".to_owned(),
                        note: None,
                        rotation_days: None,
                        rotates_at: None,
                        created_at: NOW,
                        updated_at: NOW,
                    })?
                    .id)
            })
            .expect("a secret");
        let row = fixture.create(&ProviderDraft {
            secret_id: Some(key),
            ..router(server.url("/v1"))
        });
        let providers = fixture.registry();
        let text = providers.text(&row.id).expect("a text driver");
        assert_eq!(fixture.key.calls(), 0);
        for expected in 1..=3 {
            text.list_models(&CancellationToken::new())
                .await
                .expect("lists");
            assert_eq!(fixture.key.calls(), expected);
        }
        assert_eq!(
            server.last().header("authorization").as_deref(),
            Some("Bearer sk-test")
        );
        assert!(Arc::ptr_eq(
            &text,
            &providers.text(&row.id).expect("the cached driver")
        ));
        assert_eq!(fixture.key.calls(), 3);
    }

    #[test]
    fn a_disabled_or_missing_provider_is_refused_before_any_transport_is_built() {
        let fixture = Fixture::new();
        let row = fixture.create(&ProviderDraft {
            enabled: false,
            ..draft()
        });
        let providers = fixture.registry();
        let disabled = providers.driver(&row.id).expect_err("refuses");
        assert_eq!(disabled.code, AppErrorCode::Conflict);
        assert_eq!(disabled.message, "Провайдер отключён");
        let missing = providers.driver("missing").expect_err("refuses");
        assert_eq!(missing.code, AppErrorCode::NotFound);
        assert_eq!(
            missing
                .details
                .and_then(|details| details.get("providerId").cloned()),
            Some(json!("missing"))
        );
        assert_eq!(providers.size(), 0);
    }

    #[test]
    fn account_auth_mode_is_rejected_with_a_clear_message_rather_than_a_silent_fallback() {
        let fixture = Fixture::new();
        let row = fixture.create(&ProviderDraft {
            kind: ProviderKind::Openrouter,
            ..draft()
        });
        fixture.sql(
            "UPDATE provider SET auth_mode = 'account' WHERE id = ?1",
            &row.id,
        );
        let error = fixture.registry().driver(&row.id).expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::ValidationFailed);
        assert_eq!(
            error.message,
            "adapter openai-compatible cannot run in account mode"
        );
        fixture.sql(
            "UPDATE provider SET auth_mode = 'nope' WHERE id = ?1",
            &row.id,
        );
        let unknown = fixture.registry().driver(&row.id).expect_err("refuses");
        assert_eq!(
            unknown.message,
            "adapter openai-compatible cannot run in nope mode"
        );
    }

    #[test]
    fn an_account_family_answers_account_not_linked_until_accounts_exist() {
        let fixture = Fixture::new();
        let row = fixture.create(&ProviderDraft {
            kind: ProviderKind::OpenaiCompatible,
            adapter: AdapterFamily::QwenWeb,
            auth_mode: AuthMode::Account,
            status: ProviderStatus::NeedsRelink,
            ..draft()
        });
        let error = fixture.registry().driver(&row.id).expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::ProviderSessionExpired);
        assert_eq!(
            error.details.map(Value::Object),
            Some(json!({ "providerId": row.id, "reason": "account-not-linked" }))
        );
        let ollama = fixture.create(&ProviderDraft {
            name: "Ollama account".to_owned(),
            auth_mode: AuthMode::Account,
            status: ProviderStatus::Failed,
            ..draft()
        });
        assert_eq!(
            fixture
                .registry()
                .driver(&ollama.id)
                .expect_err("refuses")
                .code,
            AppErrorCode::ProviderSessionExpired
        );
        fixture.sql(
            "UPDATE provider SET auth_mode = 'nope' WHERE id = ?1",
            &ollama.id,
        );
        let unknown = fixture.registry().driver(&ollama.id).expect_err("refuses");
        assert_eq!(unknown.code, AppErrorCode::ValidationFailed);
        assert_eq!(unknown.message, "Неизвестный режим авторизации");
    }

    struct LinkedAccounts {
        status: Mutex<String>,
        transports: AtomicUsize,
    }

    impl AccountPort for LinkedAccounts {
        fn snapshot(&self, _account_id: &str) -> Option<AccountSnapshot> {
            Some(AccountSnapshot {
                status: self.status.lock().expect("status").clone(),
                updated_at: NOW,
            })
        }

        fn transport(&self, _provider: &ProviderRow) -> Result<SharedTransport, AppError> {
            self.transports.fetch_add(1, Ordering::SeqCst);
            Ok(transport_for("http://127.0.0.1:9/api"))
        }
    }

    #[test]
    fn the_cache_follows_the_account_and_is_dropped_by_account() {
        let fixture = Fixture::new();
        fixture
            .database
            .with_connection(|connection| {
                Ok(connection.execute(
                    "INSERT INTO account (id, adapter, partition, external_id, linked_at, updated_at) VALUES ('account-1', 'qwen-web', 'persist:qwen', 'u1', 1, 1)",
                    [],
                )?)
            })
            .expect("an account");
        let row = fixture.create(&ProviderDraft {
            auth_mode: AuthMode::Account,
            account_id: Some("account-1".to_owned()),
            ..draft()
        });
        let accounts = Arc::new(LinkedAccounts {
            status: Mutex::new("linked".to_owned()),
            transports: AtomicUsize::new(0),
        });
        let providers = ProviderRegistry::new(ProviderRegistryOptions {
            accounts: accounts.clone(),
            ..ProviderRegistryOptions::new(
                Arc::new(DatabaseProviders::new(fixture.database.shared())),
                fixture.key.clone(),
                direct_http(),
            )
        });
        let first = providers.driver(&row.id).expect("builds");
        assert!(Arc::ptr_eq(
            &first,
            &providers.driver(&row.id).expect("cached")
        ));
        assert_eq!(accounts.transports.load(Ordering::SeqCst), 1);
        "needs-relink".clone_into(&mut accounts.status.lock().expect("status"));
        let second = providers.driver(&row.id).expect("rebuilds");
        assert!(!Arc::ptr_eq(&first, &second));
        providers.invalidate_by_account("someone-else");
        assert_eq!(providers.size(), 1);
        providers.invalidate_by_account("account-1");
        assert_eq!(providers.size(), 0);
    }

    #[test]
    fn an_account_mode_provider_without_a_live_account_never_reaches_the_network() {
        use crate::data::repositories::AccountDraft;
        use crate::testing::FakeSessionGateway;
        use zvs_contract::{AccountFamily, AccountStatus};

        let fixture = Fixture::new();
        let session = FakeSessionGateway::shared();
        let account = fixture
            .database
            .transaction(|transaction| {
                Ok(Repositories::new(transaction)
                    .accounts
                    .create(&AccountDraft::new(
                        AccountFamily::DeepseekWeb,
                        "persist:browser-work",
                        "user-1",
                        100,
                    ))?)
            })
            .expect("an account");
        let linked = fixture.create(&ProviderDraft {
            kind: ProviderKind::OpenaiCompatible,
            name: "DeepSeek".to_owned(),
            adapter: AdapterFamily::DeepseekWeb,
            auth_mode: AuthMode::Account,
            base_url: "https://chat.deepseek.com/api/v0".to_owned(),
            account_id: Some(account.id.clone()),
            ..draft()
        });
        let sessions: SessionFactory = {
            let session = session.clone();
            Arc::new(move |_partition: &str| -> SharedSessionGateway { session.clone() })
        };
        let registry = |sessions: Option<SessionFactory>| {
            ProviderRegistry::new(ProviderRegistryOptions {
                accounts: Arc::new(DatabaseAccounts::new(
                    fixture.database.shared(),
                    sessions,
                    fixture.key.clone(),
                    silent_logger(),
                )),
                ..ProviderRegistryOptions::new(
                    Arc::new(DatabaseProviders::new(fixture.database.shared())),
                    fixture.key.clone(),
                    direct_http(),
                )
            })
        };
        let providers = registry(Some(sessions.clone()));
        assert!(
            providers.driver(&linked.id).expect("builds").text.is_some(),
            "a linked account must get as far as a built adapter"
        );

        fixture
            .database
            .transaction(|transaction| {
                Ok(Repositories::new(transaction).accounts.update_status(
                    &account.id,
                    AccountStatus::NeedsRelink,
                    Some("Куки очищены".to_owned()),
                    900,
                )?)
            })
            .expect("updates");
        let expired = providers.driver(&linked.id).expect_err("expired");
        assert_eq!(expired.code, AppErrorCode::ProviderSessionExpired);
        assert_eq!(
            expired.details.map(Value::Object),
            Some(json!({
                "providerId": linked.id,
                "accountId": account.id,
                "status": "needs-relink",
                "statusDetail": "Куки очищены",
            }))
        );

        fixture
            .database
            .transaction(|transaction| {
                Ok(Repositories::new(transaction)
                    .accounts
                    .remove(&account.id, 1000)?)
            })
            .expect("removes");
        let detached = fixture
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .providers
                    .find_by_id(&linked.id)?)
            })
            .expect("reads")
            .expect("kept");
        assert_eq!(detached.account_id, None);
        assert_eq!(
            providers.driver(&linked.id).expect_err("detached").code,
            AppErrorCode::ProviderSessionExpired
        );
        assert!(session.requests().is_empty());

        let relinked = fixture
            .database
            .transaction(|transaction| {
                Ok(Repositories::new(transaction)
                    .accounts
                    .create(&AccountDraft::new(
                        AccountFamily::DeepseekWeb,
                        "persist:browser-work",
                        "user-2",
                        1100,
                    ))?)
            })
            .expect("an account");
        fixture.update(
            &linked.id,
            &ProviderPatch {
                account_id: Some(Some(relinked.id.clone())),
                status: Some(ProviderStatus::Unknown),
                updated_at: Some(1200),
                ..ProviderPatch::default()
            },
        );
        assert!(
            providers
                .driver(&linked.id)
                .expect("rebuilds")
                .text
                .is_some()
        );
        let without_sessions = registry(None).driver(&linked.id).expect_err("refuses");
        assert_eq!(without_sessions.code, AppErrorCode::ValidationFailed);
        assert_eq!(without_sessions.message, BROWSER_SESSION_UNAVAILABLE);
        assert!(session.requests().is_empty());
        assert_eq!(fixture.key.calls(), 0);
    }

    #[test]
    fn capabilities_follow_the_family_with_ollama_as_its_own_dialect() {
        let fixture = Fixture::new();
        let ollama = fixture.create(&draft());
        let router = fixture.create(&router("https://openrouter.ai/api/v1".to_owned()));
        let local = fixture.create(&ProviderDraft {
            name: "Мои модели".to_owned(),
            adapter: AdapterFamily::Local,
            base_url: LOCAL_BASE_URL.to_owned(),
            capabilities: vec![ProviderCapability::Embedding],
            ..draft()
        });
        let workspace = temporary_directory("local-models-");
        let providers = ProviderRegistry::new(ProviderRegistryOptions {
            local_models: Some(Arc::new(FileSystemModelStore::new(workspace.path()))),
            ..ProviderRegistryOptions::new(
                Arc::new(DatabaseProviders::new(fixture.database.shared())),
                fixture.key.clone(),
                direct_http(),
            )
        });
        assert_eq!(
            providers.capabilities(&ollama.id).expect("known"),
            OLLAMA_CAPABILITIES
        );
        assert_eq!(
            providers.capabilities(&router.id).expect("known"),
            OPENAI_COMPATIBLE_CAPABILITIES
        );
        assert_eq!(
            providers.capabilities(&local.id).expect("known"),
            LOCAL_CAPABILITIES
        );
        let unsupported = providers.text(&local.id).err().expect("refuses");
        assert_eq!(unsupported.code, AppErrorCode::Conflict);
        assert_eq!(
            unsupported
                .details
                .and_then(|details| details.get("capability").cloned()),
            Some(json!("text"))
        );
        assert_eq!(
            providers.image(&router.id).err().expect("refuses").message,
            "Провайдер не поддерживает эту возможность"
        );
        assert!(providers.embedding(&router.id).is_ok());
    }

    struct NoSecrets;

    impl SecretResolver for NoSecrets {
        fn resolve(&self, _id: &str) -> Result<Plaintext, AppError> {
            panic!("no secret should be resolved");
        }
    }

    struct NoProviders;

    impl ProviderSource for NoProviders {
        fn find_by_id(&self, _id: &str) -> Result<Option<ProviderRow>, AppError> {
            Ok(None)
        }
    }

    struct NoEngine;

    #[async_trait::async_trait]
    impl LocalEmbeddingEngine for NoEngine {
        async fn embed(
            &self,
            _model_name: &str,
            texts: &[String],
            _cancel: &CancellationToken,
        ) -> Result<Vec<Vec<f32>>, AppError> {
            Ok(texts.iter().map(|_| vec![0.0]).collect())
        }
    }

    fn local_row() -> ProviderRow {
        ProviderRow {
            id: "0199dd11-1111-7111-8111-000000000001".to_owned(),
            kind: ProviderKind::Ollama,
            adapter: "local".to_owned(),
            auth_mode: "api".to_owned(),
            name: "Мои модели".to_owned(),
            base_url: LOCAL_BASE_URL.to_owned(),
            secret_id: None,
            account_id: None,
            capabilities: vec![ProviderCapability::Embedding],
            cap_text: false,
            cap_embedding: true,
            cap_image: false,
            enabled: true,
            settings: ProviderSettings::default(),
            default_model_id: None,
            status: ProviderStatus::Unknown,
            status_detail: None,
            last_probe_at: None,
            last_latency_ms: None,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[tokio::test]
    async fn the_local_family_wins_over_the_vendor_so_nothing_reaches_the_network() {
        let workspace = temporary_directory("local-models-");
        std::fs::create_dir_all(workspace.join("embeddings")).expect("a folder");
        std::fs::write(
            workspace.join("embeddings").join("bge-m3-FP16.gguf"),
            [0_u8; 16],
        )
        .expect("a file");
        let registry = ProviderRegistry::new(ProviderRegistryOptions {
            local_models: Some(Arc::new(FileSystemModelStore::new(workspace.path()))),
            ..ProviderRegistryOptions::new(
                Arc::new(NoProviders),
                Arc::new(NoSecrets),
                direct_http(),
            )
        });
        let driver = registry.ephemeral_driver(&local_row()).expect("builds");
        assert!(driver.text.is_none());
        let embedding = driver.embedding.expect("an embedding driver");
        assert_eq!(
            embedding
                .list_models(&CancellationToken::new())
                .await
                .expect("lists")
                .len(),
            1
        );
        assert_eq!(
            embedding
                .embed(
                    &["x".to_owned()],
                    "bge-m3-FP16.gguf",
                    &CancellationToken::new()
                )
                .await
                .expect_err("no engine")
                .code,
            AppErrorCode::Conflict
        );
        registry.attach_local_engine(Arc::new(NoEngine));
        let attached = registry
            .ephemeral_driver(&local_row())
            .expect("builds")
            .embedding
            .expect("an embedding driver");
        assert_eq!(
            attached
                .embed(
                    &["x".to_owned()],
                    "bge-m3-FP16.gguf",
                    &CancellationToken::new()
                )
                .await
                .expect("embeds"),
            [vec![0.0]]
        );
    }

    #[test]
    fn attaching_the_local_engine_drops_every_cached_driver_and_invalidation_is_logged() {
        let fixture = Fixture::new();
        let row = fixture.create(&draft());
        let logger = MemoryLogger::shared();
        let providers = ProviderRegistry::new(ProviderRegistryOptions {
            logger: logger.clone(),
            ..ProviderRegistryOptions::new(
                Arc::new(DatabaseProviders::new(fixture.database.shared())),
                fixture.key.clone(),
                direct_http(),
            )
        });
        providers.driver(&row.id).expect("builds");
        providers.attach_local_engine(Arc::new(NoEngine));
        assert_eq!(providers.size(), 0);
        providers.driver(&row.id).expect("builds");
        providers.invalidate(&row.id);
        providers.invalidate(&row.id);
        assert_eq!(logger.matching("Dropped a cached provider driver").len(), 1);
    }

    #[test]
    fn the_timeout_setting_reaches_the_fingerprint() {
        let fixture = Fixture::new();
        let row = fixture.create(&draft());
        let providers = fixture.registry();
        let first = providers.driver(&row.id).expect("builds");
        fixture.sql(
            "UPDATE provider SET settings = '{\"timeoutSeconds\":5}' WHERE id = ?1",
            &row.id,
        );
        assert!(!Arc::ptr_eq(
            &first,
            &providers.driver(&row.id).expect("rebuilds")
        ));
        let reread = DatabaseProviders::new(fixture.database.shared())
            .find_by_id(&row.id)
            .expect("reads")
            .expect("a row");
        assert!(fingerprint_of(&reread).contains("timeout=5|"));
    }
}
