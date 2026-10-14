use std::collections::HashMap;
use std::future::{pending, ready};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use futures::FutureExt;
use futures::future::{BoxFuture, Shared, join_all};
use serde_json::{Value, json};
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;
use zvs_contract::validate::{js_trim, normalize_url};
use zvs_contract::{
    AccountId, AdapterCapabilitiesDto, AdapterDescriptorDto, AdapterFamily, AppError, AppErrorCode,
    AuthMode, CreateProviderInput, HonoursDto, ModelDto, ModelId, ProbeResultDto,
    ProviderCapability, ProviderConnectionInput, ProviderDto, ProviderId, ProviderListFilter,
    ProviderSettingsDto, ProviderStatus, ProviderSummaryDto, SecretId, Timestamp,
    UpdateProviderInput,
};

use super::probe_outcome::{ProbeOutcome, discovery_outcome, failure_outcome, non_negative};
use super::provider_status::{
    CredentialLifetime, CredentialSource, StatusDerivation, derive_status,
};
use super::secrets::SecretService;
use crate::data::repositories::{ModelDraft, ProviderDraft, ProviderFilter, ProviderPatch};
use crate::data::schema::{AccountRow, ModelRow, ProviderRow, ProviderSettings};
use crate::data::{DataError, Database, Repositories};
use crate::drivers::ai::adapters::{adapter_entry, adapter_entry_named};
use crate::drivers::ai::capabilities::AdapterCapabilities;
use crate::drivers::ai::errors::{cancelled, raw_error_text, timed_out};
use crate::drivers::ai::ports::{AiDriver, DiscoveredModel};
use crate::drivers::ai::registry::ProviderRegistry;
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger};

pub const DEFAULT_PROBE_TIMEOUT_SECONDS: u64 = 30;
pub const LINKED_ACCOUNT_LABEL: &str = "Связанный аккаунт";

#[async_trait]
pub trait DriverSource: Send + Sync {
    async fn ephemeral_driver(&self, row: &ProviderRow) -> Result<AiDriver, AppError>;

    fn invalidate(&self, provider_id: &str);
}

#[async_trait]
impl DriverSource for ProviderRegistry {
    async fn ephemeral_driver(&self, row: &ProviderRow) -> Result<AiDriver, AppError> {
        Self::ephemeral_driver(self, row)
    }

    fn invalidate(&self, provider_id: &str) {
        Self::invalidate(self, provider_id);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderDraftInput {
    pub connection: ProviderConnectionInput,
    pub name: Option<String>,
}

impl From<ProviderConnectionInput> for ProviderDraftInput {
    fn from(connection: ProviderConnectionInput) -> Self {
        Self {
            connection,
            name: None,
        }
    }
}

impl From<CreateProviderInput> for ProviderDraftInput {
    fn from(input: CreateProviderInput) -> Self {
        Self {
            connection: input.connection,
            name: Some(input.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProbeInput {
    Saved(String),
    Draft(ProviderDraftInput),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProbeResult {
    pub provider_id: Option<String>,
    pub outcome: ProbeOutcome,
    pub derived: StatusDerivation,
    pub checked_at: Timestamp,
    pub provider: Option<ProviderDto>,
}

impl ProbeResult {
    pub fn to_dto(&self) -> Result<ProbeResultDto, AppError> {
        Ok(ProbeResultDto {
            provider_id: self
                .provider_id
                .as_deref()
                .map(|id| stored_id(ProviderId::parse, id))
                .transpose()?,
            outcome: self.outcome.to_dto(),
            status: self.derived.status,
            status_detail: self.derived.detail.clone(),
            checked_at: self.checked_at,
            provider: self.provider.clone(),
        })
    }
}

pub struct ProviderServiceOptions {
    pub database: Arc<Database>,
    pub drivers: Arc<dyn DriverSource>,
    pub secrets: Arc<SecretService>,
    pub logger: SharedLogger,
    pub clock: SharedClock,
    pub probe_timeout_seconds: u64,
}

impl ProviderServiceOptions {
    pub fn new(
        database: Arc<Database>,
        drivers: Arc<dyn DriverSource>,
        secrets: Arc<SecretService>,
        logger: SharedLogger,
        clock: SharedClock,
    ) -> Self {
        Self {
            database,
            drivers,
            secrets,
            logger,
            clock,
            probe_timeout_seconds: DEFAULT_PROBE_TIMEOUT_SECONDS,
        }
    }
}

type ProbeFuture = BoxFuture<'static, Result<ProbeResult, AppError>>;
type SharedProbe = Shared<ProbeFuture>;

struct Inner {
    database: Arc<Database>,
    drivers: Arc<dyn DriverSource>,
    secrets: Arc<SecretService>,
    logger: SharedLogger,
    clock: SharedClock,
    timeout_seconds: u64,
    in_flight: Mutex<HashMap<String, SharedProbe>>,
    shutdown: CancellationToken,
    disposed: AtomicBool,
}

#[derive(Clone)]
pub struct ProviderService {
    inner: Arc<Inner>,
}

struct Settle {
    inner: Arc<Inner>,
    id: String,
}

impl Drop for Settle {
    fn drop(&mut self) {
        self.inner.in_flight().remove(&self.id);
    }
}

impl ProviderService {
    pub fn new(options: ProviderServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                database: options.database,
                drivers: options.drivers,
                secrets: options.secrets,
                logger: options.logger,
                clock: options.clock,
                timeout_seconds: options.probe_timeout_seconds,
                in_flight: Mutex::new(HashMap::new()),
                shutdown: CancellationToken::new(),
                disposed: AtomicBool::new(false),
            }),
        }
    }

    pub fn list(&self, filter: &ProviderListFilter) -> Result<Vec<ProviderSummaryDto>, AppError> {
        self.inner.list(filter)
    }

    pub fn get(&self, id: &str, selected_only: bool) -> Result<ProviderDto, AppError> {
        let row = self.inner.require(id)?;
        self.inner.to_dto(&row, selected_only)
    }

    pub fn create(&self, input: &CreateProviderInput) -> Result<ProviderDto, AppError> {
        let inner = &self.inner;
        let now = inner.clock.now();
        let connection = &input.connection;
        let mut draft = ProviderDraft::new(
            connection.kind,
            js_trim(&input.name),
            normalize_url(&connection.base_url),
            connection.capabilities.clone(),
            now,
        );
        draft.adapter = connection.adapter();
        draft.auth_mode = connection.auth_mode();
        draft.secret_id = connection.secret_id().map(|id| id.to_string());
        draft.account_id = connection.account_id().map(|id| id.to_string());
        draft.settings = settings_column(&connection.settings())?;
        draft.enabled = input.enabled();
        let row = inner
            .database
            .transaction(|transaction| Repositories::new(transaction).providers.create(&draft))?;
        inner.logger.log(
            LogLevel::Info,
            "providers",
            "Created a provider",
            &[
                ("providerId", json!(row.id)),
                ("adapter", json!(row.adapter)),
            ],
        );
        inner.to_dto(&row, false)
    }

    pub fn update(&self, input: &UpdateProviderInput) -> Result<ProviderDto, AppError> {
        let inner = &self.inner;
        let id = input.id.to_string();
        self.require_idle(&id)?;
        inner.require(&id)?;
        let now = inner.clock.now();
        let patch = ProviderPatch {
            name: input.name.as_deref().map(|name| js_trim(name).to_owned()),
            adapter: input.adapter,
            auth_mode: input.auth_mode,
            base_url: input.base_url.as_deref().map(normalize_url),
            secret_id: input
                .secret_id
                .map(|secret_id| secret_id.map(|id| id.to_string())),
            account_id: input
                .account_id
                .map(|account_id| account_id.map(|id| id.to_string())),
            capabilities: input.capabilities.clone(),
            settings: input.settings.as_ref().map(settings_column).transpose()?,
            enabled: input.enabled,
            updated_at: Some(now),
            ..ProviderPatch::default()
        };
        let row = inner
            .database
            .transaction(|transaction| {
                Repositories::new(transaction)
                    .providers
                    .update(&id, &patch, now)
            })?
            .ok_or_else(|| not_found(&id))?;
        inner.drivers.invalidate(&id);
        inner.to_dto(&row, false)
    }

    pub fn remove(&self, id: &str) -> Result<(), AppError> {
        let inner = &self.inner;
        self.require_idle(id)?;
        inner.require(id)?;
        inner.database.transaction(|transaction| {
            Ok(Repositories::new(transaction).providers.remove(id)?)
        })?;
        inner.drivers.invalidate(id);
        inner.logger.log(
            LogLevel::Info,
            "providers",
            "Removed a provider",
            &[("providerId", json!(id))],
        );
        Ok(())
    }

    pub fn set_default_model(
        &self,
        id: &str,
        model_id: Option<&str>,
    ) -> Result<ProviderDto, AppError> {
        let inner = &self.inner;
        let now = inner.clock.now();
        let row = inner
            .database
            .transaction(|transaction| {
                Repositories::new(transaction)
                    .providers
                    .set_default_model(id, model_id, now)
            })?
            .ok_or_else(|| not_found(id))?;
        inner.to_dto(&row, false)
    }

    pub fn models(&self, id: &str) -> Result<Vec<ModelDto>, AppError> {
        self.inner.require(id)?;
        self.inner
            .data(|repositories| Ok(repositories.models.list_by_provider(id, false)?))?
            .iter()
            .map(model_dto)
            .collect()
    }

    pub fn is_probing(&self, id: &str) -> bool {
        self.inner.in_flight().contains_key(id)
    }

    pub fn probe(&self, input: ProbeInput, signal: Option<CancellationToken>) -> ProbeFuture {
        if self.inner.disposed() {
            return ready(Err(cancelled())).boxed();
        }
        let id = match input {
            ProbeInput::Draft(draft) => {
                let inner = Arc::clone(&self.inner);
                return async move { inner.probe_draft(&draft).await }.boxed();
            }
            ProbeInput::Saved(id) => id,
        };
        let mut in_flight = self.inner.in_flight();
        if let Some(joined) = in_flight.get(&id) {
            return joined.clone().boxed();
        }
        let Ok(runtime) = Handle::try_current() else {
            return ready(Err(AppError::new(
                AppErrorCode::Unknown,
                "Проверка провайдера недоступна вне среды выполнения",
            )))
            .boxed();
        };
        let settle = Settle {
            inner: Arc::clone(&self.inner),
            id: id.clone(),
        };
        let task = runtime.spawn(async move {
            let result = settle.inner.probe_saved(&settle.id, signal).await;
            drop(settle);
            result
        });
        let shared = async move { task.await.unwrap_or_else(|_| Err(cancelled())) }
            .boxed()
            .shared();
        in_flight.insert(id, shared.clone());
        drop(in_flight);
        shared.boxed()
    }

    pub async fn refresh_all(&self, only_idle: bool) -> Result<Vec<ProviderSummaryDto>, AppError> {
        let enabled = ProviderListFilter {
            capability: None,
            enabled: Some(true),
        };
        let rows = self.inner.data(|repositories| {
            Ok(repositories.providers.list(&ProviderFilter {
                capability: None,
                enabled: Some(true),
            })?)
        })?;
        for row in rows {
            if only_idle && self.is_probing(&row.id) {
                continue;
            }
            self.probe(ProbeInput::Saved(row.id), None).await?;
        }
        self.list(&enabled)
    }

    pub async fn dispose(&self) {
        self.inner.disposed.store(true, Ordering::SeqCst);
        self.inner.shutdown.cancel();
        let pending: Vec<SharedProbe> = self.inner.in_flight().values().cloned().collect();
        join_all(pending).await;
    }

    fn require_idle(&self, id: &str) -> Result<(), AppError> {
        if self.is_probing(id) {
            return Err(AppError::new(
                AppErrorCode::Conflict,
                "Дождитесь завершения проверки провайдера",
            ));
        }
        Ok(())
    }
}

impl Inner {
    fn in_flight(&self) -> MutexGuard<'_, HashMap<String, SharedProbe>> {
        self.in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn disposed(&self) -> bool {
        self.disposed.load(Ordering::SeqCst)
    }

    fn data<T>(
        &self,
        work: impl FnOnce(&Repositories<'_>) -> Result<T, DataError>,
    ) -> Result<T, AppError> {
        self.database
            .with_connection(|connection| work(&Repositories::new(connection)))
    }

    fn require(&self, id: &str) -> Result<ProviderRow, AppError> {
        self.data(|repositories| Ok(repositories.providers.find_by_id(id)?))?
            .ok_or_else(|| not_found(id))
    }

    fn account(&self, account_id: &str) -> Option<AccountRow> {
        self.data(|repositories| Ok(repositories.accounts.find_by_id(account_id)?))
            .ok()
            .flatten()
    }

    fn list(&self, filter: &ProviderListFilter) -> Result<Vec<ProviderSummaryDto>, AppError> {
        let now = self.clock.now();
        self.data(|repositories| {
            repositories
                .providers
                .list(&ProviderFilter {
                    capability: filter.capability,
                    enabled: filter.enabled,
                })?
                .into_iter()
                .map(|row| {
                    let count = repositories.models.list_by_provider(&row.id, false)?.len();
                    let account = match &row.account_id {
                        Some(account_id) => repositories.accounts.find_by_id(account_id)?,
                        None => None,
                    };
                    Ok((row, account, count))
                })
                .collect::<Result<Vec<_>, DataError>>()
        })?
        .into_iter()
        .map(|(row, account, count)| {
            summary_dto(&account_status(row, account.as_ref(), now), count)
        })
        .collect()
    }

    fn to_dto(&self, row: &ProviderRow, selected_only: bool) -> Result<ProviderDto, AppError> {
        let now = self.clock.now();
        let models = self.data(|repositories| {
            Ok(repositories
                .models
                .list_by_provider(&row.id, selected_only)?)
        })?;
        let account = row
            .account_id
            .as_deref()
            .and_then(|account_id| self.account(account_id));
        let secret_name = row
            .secret_id
            .as_deref()
            .and_then(|secret_id| self.secrets.get(secret_id).ok())
            .map(|secret| secret.name);
        let family = AdapterFamily::parse(&row.adapter).ok_or_else(corrupt)?;
        Ok(ProviderDto {
            summary: summary_dto(
                &account_status(row.clone(), account.as_ref(), now),
                models.len(),
            )?,
            base_url: row.base_url.clone(),
            secret_id: row
                .secret_id
                .as_deref()
                .map(|id| stored_id(SecretId::parse, id))
                .transpose()?,
            secret_name,
            account_id: row
                .account_id
                .as_deref()
                .map(|id| stored_id(AccountId::parse, id))
                .transpose()?,
            account_label: account.as_ref().map(account_label),
            settings: settings_dto(&row.settings)?,
            adapter_capabilities: capabilities_dto(&adapter_entry(family).capabilities),
            models: models.iter().map(model_dto).collect::<Result<_, _>>()?,
        })
    }

    fn lifetimes(&self, row: &ProviderRow) -> Vec<CredentialLifetime> {
        let mut lifetimes = Vec::new();
        if let Some(secret_id) = &row.secret_id {
            lifetimes.push(CredentialLifetime {
                source: CredentialSource::Secret,
                expires_at: self
                    .secrets
                    .get(secret_id)
                    .ok()
                    .and_then(|secret| secret.rotates_at),
            });
        }
        if let Some(account_id) = &row.account_id {
            lifetimes.push(CredentialLifetime {
                source: CredentialSource::Account,
                expires_at: self
                    .account(account_id)
                    .and_then(|account| account.token_expires_at)
                    .map(seconds_to_millis),
            });
        }
        lifetimes
    }

    async fn probe_saved(
        &self,
        id: &str,
        signal: Option<CancellationToken>,
    ) -> Result<ProbeResult, AppError> {
        let row = self.require(id)?;
        let outcome = self.run_probe(&row, signal.as_ref()).await;
        if self.disposed() || signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
            return Err(cancelled());
        }
        let checked_at = self.clock.now();
        let derived = derive_status(&outcome, &self.lifetimes(&row), checked_at);
        let stored = self.persist(&row, &outcome, &derived, checked_at)?;
        Ok(ProbeResult {
            provider_id: Some(id.to_owned()),
            outcome,
            derived,
            checked_at,
            provider: Some(self.to_dto(&stored, false)?),
        })
    }

    async fn probe_draft(&self, draft: &ProviderDraftInput) -> Result<ProbeResult, AppError> {
        let row = draft_row(draft, self.clock.now())?;
        let outcome = self.run_probe(&row, None).await;
        if self.disposed() {
            return Err(cancelled());
        }
        let checked_at = self.clock.now();
        Ok(ProbeResult {
            provider_id: None,
            derived: derive_status(&outcome, &self.lifetimes(&row), checked_at),
            outcome,
            checked_at,
            provider: None,
        })
    }

    async fn run_probe(
        &self,
        row: &ProviderRow,
        signal: Option<&CancellationToken>,
    ) -> ProbeOutcome {
        if let Some(account_id) = &row.account_id
            && !self
                .account(account_id)
                .is_some_and(|account| account.is_linked())
        {
            return ProbeOutcome::SessionExpired;
        }
        let seconds = row
            .settings
            .timeout_seconds()
            .unwrap_or(self.timeout_seconds as f64);
        let limit = Duration::try_from_secs_f64(seconds)
            .unwrap_or(Duration::from_secs(self.timeout_seconds));
        let controller = self.shutdown.child_token();
        let started = self.clock.now();
        let result = tokio::select! {
            biased;
            () = controller.cancelled() => Err(cancelled()),
            () = cancelled_or_pending(signal) => {
                controller.cancel();
                Err(cancelled())
            }
            () = tokio::time::sleep(limit) => {
                controller.cancel();
                Err(timed_out(seconds))
            }
            result = self.discover(row, &controller) => result,
        };
        match result {
            Ok(models) => {
                let latency = u64::try_from(self.clock.now() - started).unwrap_or(0);
                let live = adapter_entry_named(&row.adapter)
                    .is_ok_and(|entry| entry.capabilities.live_model_list);
                discovery_outcome(latency, models, live)
            }
            Err(error) => {
                let outcome = failure_outcome(&error);
                self.logger.log(
                    LogLevel::Warn,
                    "providers",
                    "Probe did not succeed",
                    &[
                        ("raw", json!(raw_error_text(&error))),
                        ("providerId", json!(row.id)),
                        ("kind", json!(outcome.kind())),
                    ],
                );
                outcome
            }
        }
    }

    async fn discover(
        &self,
        row: &ProviderRow,
        controller: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        let driver = self.drivers.ephemeral_driver(row).await?;
        if controller.is_cancelled() {
            return Err(cancelled());
        }
        if let Some(text) = &driver.text {
            return text.list_models(controller).await;
        }
        if let Some(embedding) = &driver.embedding {
            return embedding.list_models(controller).await;
        }
        if let Some(image) = &driver.image {
            return image.list_models(controller).await;
        }
        Err(AppError::new(
            AppErrorCode::Conflict,
            "Провайдер не предоставляет список моделей",
        )
        .with_detail("providerId", row.id.clone()))
    }

    fn persist(
        &self,
        row: &ProviderRow,
        outcome: &ProbeOutcome,
        derived: &StatusDerivation,
        checked_at: Timestamp,
    ) -> Result<ProviderRow, AppError> {
        self.database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            if outcome.succeeded() {
                let drafts: Vec<ModelDraft> = outcome
                    .models()
                    .iter()
                    .map(|model| ModelDraft::discovered(model, checked_at))
                    .collect();
                repositories
                    .models
                    .replace_for_provider(&row.id, &drafts, checked_at)?;
                let kept = repositories
                    .providers
                    .find_by_id(&row.id)?
                    .and_then(|stored| stored.default_model_id);
                if let Some(default_model_id) = &row.default_model_id
                    && kept.is_none()
                {
                    self.logger.log(
                        LogLevel::Info,
                        "providers",
                        "Default model vanished from the vendor",
                        &[
                            ("providerId", json!(row.id)),
                            ("defaultModelId", json!(default_model_id)),
                        ],
                    );
                }
            }
            let latency = outcome
                .latency_ms()
                .map(|latency| i64::try_from(latency).unwrap_or(i64::MAX));
            repositories
                .providers
                .set_status(
                    &row.id,
                    derived.status,
                    derived.detail.clone(),
                    latency,
                    checked_at,
                )?
                .ok_or_else(|| not_found(&row.id).into())
        })
    }
}

async fn cancelled_or_pending(signal: Option<&CancellationToken>) {
    match signal {
        Some(signal) => signal.cancelled().await,
        None => pending().await,
    }
}

fn seconds_to_millis(seconds: i64) -> Timestamp {
    seconds.saturating_mul(1000)
}

fn not_found(provider_id: &str) -> AppError {
    AppError::new(AppErrorCode::NotFound, "Провайдер не найден")
        .with_detail("providerId", provider_id)
}

fn corrupt() -> AppError {
    AppError::new(
        AppErrorCode::DbError,
        "Провайдер хранится в повреждённом виде",
    )
}

fn stored_id<T>(parse: fn(&str) -> Option<T>, value: &str) -> Result<T, AppError> {
    parse(value).ok_or_else(corrupt)
}

fn account_status(
    mut row: ProviderRow,
    account: Option<&AccountRow>,
    now: Timestamp,
) -> ProviderRow {
    if row.account_id.is_none() || account.is_some_and(AccountRow::is_linked) {
        return row;
    }
    let derived = derive_status(&ProbeOutcome::SessionExpired, &[], now);
    row.status = derived.status;
    row.status_detail = derived.detail;
    row
}

fn account_label(account: &AccountRow) -> String {
    account
        .email_masked
        .clone()
        .or_else(|| account.display_name.clone())
        .unwrap_or_else(|| LINKED_ACCOUNT_LABEL.to_owned())
}

fn settings_column(settings: &ProviderSettingsDto) -> Result<ProviderSettings, AppError> {
    match serde_json::to_value(settings) {
        Ok(Value::Object(map)) => Ok(ProviderSettings(map)),
        _ => Err(corrupt()),
    }
}

fn settings_dto(settings: &ProviderSettings) -> Result<ProviderSettingsDto, AppError> {
    serde_json::from_value(Value::Object(settings.0.clone())).map_err(|_| corrupt())
}

fn draft_row(draft: &ProviderDraftInput, now: Timestamp) -> Result<ProviderRow, AppError> {
    let connection = &draft.connection;
    let capabilities = connection.capabilities.clone();
    Ok(ProviderRow {
        id: format!("draft-{}", new_id().hyphenated()),
        kind: connection.kind,
        adapter: connection.adapter().as_str().to_owned(),
        auth_mode: connection.auth_mode().as_str().to_owned(),
        name: draft.name.clone().unwrap_or_else(|| "draft".to_owned()),
        base_url: normalize_url(&connection.base_url),
        secret_id: connection.secret_id().map(|id| id.to_string()),
        account_id: connection.account_id().map(|id| id.to_string()),
        cap_text: capabilities.contains(&ProviderCapability::Text),
        cap_embedding: capabilities.contains(&ProviderCapability::Embedding),
        cap_image: capabilities.contains(&ProviderCapability::Image),
        capabilities,
        enabled: true,
        settings: settings_column(&connection.settings())?,
        default_model_id: None,
        status: ProviderStatus::Unknown,
        status_detail: None,
        last_probe_at: None,
        last_latency_ms: None,
        created_at: now,
        updated_at: now,
    })
}

pub fn capabilities_dto(capabilities: &AdapterCapabilities) -> AdapterCapabilitiesDto {
    AdapterCapabilitiesDto {
        family: capabilities.family,
        auth_modes: capabilities.auth_modes.to_vec(),
        streaming: capabilities.streaming,
        live_model_list: capabilities.live_model_list,
        embedding: capabilities.embedding,
        image: capabilities.image,
        honours: HonoursDto {
            temperature: capabilities.honours.temperature,
            top_k: capabilities.honours.top_k,
            top_p: capabilities.honours.top_p,
            max_output_tokens: capabilities.honours.max_output_tokens,
        },
        tools: capabilities.tools,
    }
}

pub fn adapter_descriptors() -> Vec<AdapterDescriptorDto> {
    AdapterFamily::ALL
        .iter()
        .map(|family| {
            let entry = adapter_entry(*family);
            AdapterDescriptorDto {
                capabilities: capabilities_dto(&entry.capabilities),
                implemented: entry.implemented,
            }
        })
        .collect()
}

pub fn summary_dto(row: &ProviderRow, model_count: usize) -> Result<ProviderSummaryDto, AppError> {
    Ok(ProviderSummaryDto {
        id: stored_id(ProviderId::parse, &row.id)?,
        kind: row.kind,
        adapter: AdapterFamily::parse(&row.adapter).ok_or_else(corrupt)?,
        auth_mode: AuthMode::parse(&row.auth_mode).ok_or_else(corrupt)?,
        name: row.name.clone(),
        capabilities: row.capabilities.clone(),
        enabled: row.enabled,
        status: row.status,
        status_detail: row.status_detail.clone(),
        last_probe_at: row.last_probe_at,
        last_latency_ms: non_negative(row.last_latency_ms),
        model_count: model_count as u64,
        default_model_id: row
            .default_model_id
            .as_deref()
            .map(|id| stored_id(ModelId::parse, id))
            .transpose()?,
        updated_at: row.updated_at,
    })
}

pub fn model_dto(row: &ModelRow) -> Result<ModelDto, AppError> {
    Ok(ModelDto {
        is_free: Some(row.is_free),
        no_training: Some(row.no_training),
        id: stored_id(ModelId::parse, &row.id)?,
        provider_id: stored_id(ProviderId::parse, &row.provider_id)?,
        external_id: row.external_id.clone(),
        display_name: row.display_name.clone(),
        family: row.family.clone(),
        context_window: non_negative(row.context_window),
        max_output: non_negative(row.max_output),
        size_bytes: non_negative(row.size_bytes),
        capabilities: row.capabilities.clone(),
        available: row.available,
        unavailable_reason: row.unavailable_reason.clone(),
        discovered_at: row.discovered_at,
    })
}

#[cfg(test)]
mod tests;
