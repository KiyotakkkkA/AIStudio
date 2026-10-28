use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Instant;

use futures::FutureExt;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::integrations::config_secret_ids;
use zvs_contract::validate::check;
use zvs_contract::{
    AppError, AppErrorCode, CreateIntegrationInput, IntegrationAccountDetailDto,
    IntegrationAccountDto, IntegrationAccountToolDto, IntegrationActivityDto,
    IntegrationActivityInput, IntegrationActivityLevel, IntegrationConfig, IntegrationId,
    IntegrationKind, IntegrationKindDto, IntegrationStatus, IntegrationTestResultDto, Timestamp,
    ToolSource, ToolTier, UpdateIntegrationInput, Validate, WorkerDto, WorkerStatus,
};

use super::tools::register_integration_tool_node;
use super::{
    IntegrationCallTarget, IntegrationConfigView, IntegrationEvent, IntegrationProvider,
    IntegrationSession, IntegrationToolCall, IntegrationToolCaller, account_tool_key, check_config,
    check_provider,
};
use crate::data::repositories::{IntegrationCounter, IntegrationPatch};
use crate::data::schema::{IntegrationAccountRow, IntegrationActivityRow, SecretConsumerKind};
use crate::data::{Database, Repositories};
use crate::kernel::NodeRegistry;
use crate::platform::clock::{SharedClock, SystemClock};
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};
use crate::services::SecretService;
use crate::tools::{ToolDescriptor, ToolRegistry};
use crate::workers::{Backoff, WorkerBody, WorkerFault, WorkerSpec, WorkerSupervisor};

pub const ACCOUNT_NOT_FOUND: &str = "Интеграция не найдена";
pub const KIND_UNAVAILABLE: &str = "Интеграции этого типа недоступны";
pub const UNKNOWN_TRIGGER: &str = "Интеграция не объявляет такой триггер";
pub const EVENT_RETENTION_MS: Timestamp = 30 * 24 * 60 * 60 * 1000;

const WORKER_PREFIX: &str = "integration:";
const ACTIVITY_KEEP: usize = 200;
const ACTIVITY_MESSAGE_MAX: usize = 1000;
const EVENT_ID_MAX: usize = 512;
const CONNECTED: &str = "Подключение установлено";

pub type TriggerSink = Arc<dyn Fn(&Value) -> Result<Vec<String>, AppError> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dispatched {
    Started(Vec<String>),
    Duplicate,
}

pub fn worker_key(account_id: &str) -> String {
    format!("{WORKER_PREFIX}{account_id}")
}

pub struct IntegrationServiceOptions {
    pub database: Arc<Database>,
    pub tools: ToolRegistry,
    pub nodes: Arc<NodeRegistry>,
    pub secrets: Arc<SecretService>,
    pub workers: WorkerSupervisor,
    pub providers: Vec<Arc<dyn IntegrationProvider>>,
    pub clock: SharedClock,
    pub logger: SharedLogger,
    pub backoff: Backoff,
}

impl IntegrationServiceOptions {
    pub fn new(
        database: Arc<Database>,
        tools: ToolRegistry,
        nodes: Arc<NodeRegistry>,
        secrets: Arc<SecretService>,
        workers: WorkerSupervisor,
    ) -> Self {
        Self {
            database,
            tools,
            nodes,
            secrets,
            workers,
            providers: Vec::new(),
            clock: Arc::new(SystemClock),
            logger: silent_logger(),
            backoff: Backoff::default(),
        }
    }
}

struct Inner {
    options: IntegrationServiceOptions,
    providers: BTreeMap<IntegrationKind, Arc<dyn IntegrationProvider>>,
    sink: Mutex<Option<TriggerSink>>,
    observed: Mutex<HashMap<String, (WorkerStatus, u64)>>,
}

#[derive(Clone)]
pub struct IntegrationService {
    inner: Arc<Inner>,
}

fn not_found(id: &str) -> AppError {
    AppError::new(AppErrorCode::NotFound, ACCOUNT_NOT_FOUND).with_detail("id", id)
}

fn unavailable(kind: IntegrationKind) -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, KIND_UNAVAILABLE)
        .with_detail("field", "kind")
        .with_detail("kind", kind.as_str())
}

fn integration_id(id: &str) -> Result<IntegrationId, AppError> {
    IntegrationId::parse(id).ok_or_else(|| not_found(id))
}

fn bounded(message: &str) -> String {
    message.chars().take(ACTIVITY_MESSAGE_MAX).collect()
}

const fn integration_status(status: WorkerStatus) -> IntegrationStatus {
    match status {
        WorkerStatus::Starting => IntegrationStatus::Starting,
        WorkerStatus::Running => IntegrationStatus::Running,
        WorkerStatus::Retrying => IntegrationStatus::Retrying,
        WorkerStatus::Stopped => IntegrationStatus::Stopped,
        WorkerStatus::Failed => IntegrationStatus::Failed,
    }
}

impl IntegrationService {
    pub fn new(options: IntegrationServiceOptions) -> Result<Self, AppError> {
        let mut providers: BTreeMap<IntegrationKind, Arc<dyn IntegrationProvider>> =
            BTreeMap::new();
        for provider in &options.providers {
            check_provider(provider.as_ref())?;
            if providers
                .insert(provider.kind(), Arc::clone(provider))
                .is_some()
            {
                return Err(AppError::new(
                    AppErrorCode::Conflict,
                    "Для этого типа интеграции уже зарегистрирован обработчик",
                )
                .with_detail("kind", provider.kind().as_str()));
            }
        }
        let workers = options.workers.clone();
        let service = Self {
            inner: Arc::new(Inner {
                options,
                providers,
                sink: Mutex::new(None),
                observed: Mutex::new(HashMap::new()),
            }),
        };
        let weak = Arc::downgrade(&service.inner);
        workers.watch(Arc::new(move |worker: &WorkerDto| {
            if let Some(inner) = weak.upgrade() {
                Self { inner }.observe(worker);
            }
        }));
        Ok(service)
    }

    pub fn attach_sink(&self, sink: TriggerSink) {
        *self
            .inner
            .sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(sink);
    }

    fn database(&self) -> &Database {
        &self.inner.options.database
    }

    fn now(&self) -> Timestamp {
        self.inner.options.clock.now()
    }

    fn log(&self, level: LogLevel, message: &str, fields: &[(&str, Value)]) {
        self.inner
            .options
            .logger
            .log(level, "integrations", message, fields);
    }

    fn provider(&self, kind: IntegrationKind) -> Result<Arc<dyn IntegrationProvider>, AppError> {
        self.inner
            .providers
            .get(&kind)
            .cloned()
            .ok_or_else(|| unavailable(kind))
    }

    fn require(&self, id: &str) -> Result<IntegrationAccountRow, AppError> {
        self.database()
            .with_connection(|connection| {
                Ok(Repositories::new(connection).integrations.find(id)?)
            })?
            .ok_or_else(|| not_found(id))
    }

    fn patch(&self, id: &str, patch: &IntegrationPatch) -> Result<IntegrationAccountRow, AppError> {
        self.database()
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .integrations
                    .update(id, patch)?)
            })?
            .ok_or_else(|| not_found(id))
    }

    fn view(&self, row: &IntegrationAccountRow) -> IntegrationConfigView {
        IntegrationConfigView::new(row.config.clone(), Arc::clone(&self.inner.options.secrets))
    }

    fn account_dto(&self, row: &IntegrationAccountRow) -> Result<IntegrationAccountDto, AppError> {
        let (status, status_detail) = if row.enabled {
            match self.inner.options.workers.get(&worker_key(&row.id)) {
                Some(worker) => {
                    let detail = worker.detail.clone().or_else(|| {
                        (worker.status != WorkerStatus::Running)
                            .then(|| worker.last_error.clone())
                            .flatten()
                    });
                    (integration_status(worker.status), detail)
                }
                None => (IntegrationStatus::Stopped, row.last_error.clone()),
            }
        } else {
            (IntegrationStatus::Disabled, None)
        };
        Ok(IntegrationAccountDto {
            id: integration_id(&row.id)?,
            kind: row.kind,
            name: row.name.clone(),
            enabled: row.enabled,
            config: row.config.clone(),
            status,
            status_detail,
            last_activity_at: row.last_activity_at,
            counters: row.counters,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    fn check_secrets(&self, config: &IntegrationConfig) -> Result<(), AppError> {
        for secret in config_secret_ids(config) {
            self.inner.options.secrets.get(&secret.to_string())?;
        }
        Ok(())
    }

    fn validate_config(
        &self,
        provider: &dyn IntegrationProvider,
        config: &IntegrationConfig,
    ) -> Result<(), AppError> {
        check_config(&provider.describe(), config)?;
        self.check_secrets(config)
    }

    fn sync_secret_usage(
        &self,
        id: &str,
        before: &IntegrationConfig,
        after: &IntegrationConfig,
    ) -> Result<(), AppError> {
        let secrets = &self.inner.options.secrets;
        let (before, after) = (config_secret_ids(before), config_secret_ids(after));
        for secret in before.iter().filter(|entry| !after.contains(entry)) {
            secrets.remove_usage(&secret.to_string(), SecretConsumerKind::Integration, id)?;
        }
        for secret in after.iter().filter(|entry| !before.contains(entry)) {
            secrets.add_usage(&secret.to_string(), SecretConsumerKind::Integration, id)?;
        }
        Ok(())
    }

    pub fn kinds(&self) -> Vec<IntegrationKindDto> {
        self.inner
            .providers
            .values()
            .map(|provider| IntegrationKindDto {
                kind: provider.kind(),
                label: provider.label(),
                fields: provider.describe(),
                triggers: provider.triggers(),
                tools: provider.tools().into_iter().map(|spec| spec.tool).collect(),
            })
            .collect()
    }

    pub fn list(&self) -> Result<Vec<IntegrationAccountDto>, AppError> {
        self.database()
            .with_connection(|connection| Ok(Repositories::new(connection).integrations.list()?))?
            .iter()
            .map(|row| self.account_dto(row))
            .collect()
    }

    pub fn account(&self, id: &str) -> Result<IntegrationAccountDto, AppError> {
        self.account_dto(&self.require(id)?)
    }

    pub fn get(&self, id: &str) -> Result<IntegrationAccountDetailDto, AppError> {
        let row = self.require(id)?;
        let (triggers, tools) = match self.provider(row.kind) {
            Ok(provider) => (
                provider.triggers(),
                provider
                    .tools()
                    .into_iter()
                    .map(|spec| IntegrationAccountToolDto {
                        key: account_tool_key(&spec.tool.name, &row.id),
                        tool: spec.tool,
                    })
                    .collect(),
            ),
            Err(_) => (Vec::new(), Vec::new()),
        };
        Ok(IntegrationAccountDetailDto {
            account: self.account_dto(&row)?,
            triggers,
            tools,
        })
    }

    pub async fn create(
        &self,
        input: &CreateIntegrationInput,
    ) -> Result<IntegrationAccountDto, AppError> {
        input.validate()?;
        let provider = self.provider(input.kind)?;
        let config = input.config();
        self.validate_config(provider.as_ref(), &config)?;
        let now = self.now();
        let row = self.database().with_connection(|connection| {
            Ok(Repositories::new(connection)
                .integrations
                .insert(&IntegrationAccountRow {
                    id: new_id().to_string(),
                    kind: input.kind,
                    name: input.name().to_owned(),
                    enabled: input.enabled(),
                    config,
                    last_error: None,
                    last_activity_at: None,
                    counters: zvs_contract::IntegrationCountersDto::default(),
                    created_at: now,
                    updated_at: now,
                })?)
        })?;
        self.sync_secret_usage(&row.id, &IntegrationConfig::new(), &row.config)?;
        self.log(
            LogLevel::Info,
            "Added an integration account",
            &[("accountId", json!(row.id)), ("kind", json!(row.kind))],
        );
        if row.enabled {
            self.launch(&row).await?;
        }
        self.account(&row.id)
    }

    pub async fn update(
        &self,
        input: &UpdateIntegrationInput,
    ) -> Result<IntegrationAccountDto, AppError> {
        input.validate()?;
        let existing = self.require(&input.id.to_string())?;
        if let Some(config) = &input.config {
            self.validate_config(self.provider(existing.kind)?.as_ref(), config)?;
        }
        let row = self.patch(
            &existing.id,
            &IntegrationPatch {
                name: input.name().map(str::to_owned),
                config: input.config.clone(),
                last_error: input.config.as_ref().map(|_| None),
                updated_at: Some(self.now()),
                ..IntegrationPatch::default()
            },
        )?;
        self.sync_secret_usage(&row.id, &existing.config, &row.config)?;
        if row.enabled {
            self.launch(&row).await?;
        }
        self.account(&row.id)
    }

    pub async fn remove(&self, id: &str) -> Result<(), AppError> {
        let row = self.require(id)?;
        self.halt(&row).await?;
        self.sync_secret_usage(&row.id, &row.config, &IntegrationConfig::new())?;
        self.database().with_connection(|connection| {
            Ok(Repositories::new(connection).integrations.remove(&row.id)?)
        })?;
        self.inner
            .observed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&worker_key(&row.id));
        self.log(
            LogLevel::Info,
            "Removed an integration account",
            &[("accountId", json!(row.id))],
        );
        Ok(())
    }

    pub async fn enable(&self, id: &str) -> Result<IntegrationAccountDto, AppError> {
        let row = self.patch(
            id,
            &IntegrationPatch {
                enabled: Some(true),
                last_error: Some(None),
                updated_at: Some(self.now()),
                ..IntegrationPatch::default()
            },
        )?;
        self.launch(&row).await?;
        self.account(&row.id)
    }

    pub async fn disable(&self, id: &str) -> Result<IntegrationAccountDto, AppError> {
        let row = self.patch(
            id,
            &IntegrationPatch {
                enabled: Some(false),
                updated_at: Some(self.now()),
                ..IntegrationPatch::default()
            },
        )?;
        self.halt(&row).await?;
        self.account(&row.id)
    }

    pub async fn test(&self, id: &str) -> Result<IntegrationTestResultDto, AppError> {
        let row = self.require(id)?;
        let provider = self.provider(row.kind)?;
        let started = Instant::now();
        let outcome = provider.test(&self.view(&row)).await;
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        Ok(match outcome {
            Ok(checks) => IntegrationTestResultDto {
                ok: checks.iter().all(|check| check.ok),
                latency_ms,
                checks,
                error: None,
            },
            Err(error) => IntegrationTestResultDto {
                ok: false,
                latency_ms,
                checks: Vec::new(),
                error: Some(bounded(&error.message)),
            },
        })
    }

    pub fn activity(
        &self,
        input: &IntegrationActivityInput,
    ) -> Result<Vec<IntegrationActivityDto>, AppError> {
        input.validate()?;
        let row = self.require(&input.id.to_string())?;
        Ok(self
            .database()
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .integrations
                    .activity(&row.id, input.limit())?)
            })?
            .into_iter()
            .map(|entry| IntegrationActivityDto {
                at: entry.at,
                level: entry.level,
                message: entry.message,
            })
            .collect())
    }

    pub async fn start_enabled(&self) -> Result<(), AppError> {
        let before = self.now().saturating_sub(EVENT_RETENTION_MS);
        let rows = self.database().with_connection(|connection| {
            let integrations = Repositories::new(connection).integrations;
            integrations.prune_events(before)?;
            Ok(integrations.list_enabled()?)
        })?;
        for row in rows {
            if let Err(error) = self.launch(&row).await {
                self.log(
                    LogLevel::Warn,
                    "Could not start an integration account",
                    &[
                        ("accountId", json!(row.id)),
                        ("error", json!(error.message)),
                    ],
                );
                self.record_failure(&row.id, &error.message);
            }
        }
        Ok(())
    }

    pub async fn dispose(&self) {
        let keys: Vec<String> = self
            .inner
            .options
            .workers
            .list()
            .into_iter()
            .map(|worker| worker.key)
            .filter(|key| key.starts_with(WORKER_PREFIX))
            .collect();
        for key in keys {
            self.inner.options.workers.remove(&key).await;
            let id = &key[WORKER_PREFIX.len()..];
            if let Ok(row) = self.require(id)
                && let (Ok(provider), Ok(account)) =
                    (self.provider(row.kind), self.account_dto(&row))
            {
                provider.stop(&account).await;
            }
        }
    }

    async fn launch(&self, row: &IntegrationAccountRow) -> Result<(), AppError> {
        let provider = self.provider(row.kind)?;
        self.withdraw(&row.id)?;
        self.expose(row, provider.as_ref())?;
        let weak = Arc::downgrade(&self.inner);
        let id = row.id.clone();
        let body: WorkerBody = Arc::new(move |worker| {
            let weak = weak.clone();
            let id = id.clone();
            async move {
                let Some(inner) = weak.upgrade() else {
                    return Ok(());
                };
                let service = Self { inner };
                let row = service.require(&id).map_err(WorkerFault::permanent)?;
                let provider = service.provider(row.kind).map_err(WorkerFault::permanent)?;
                let session = IntegrationSession {
                    account: service.account_dto(&row)?,
                    config: service.view(&row),
                    worker,
                    service: service.clone(),
                };
                provider.start(session).await
            }
            .boxed()
        });
        self.inner
            .options
            .workers
            .spawn(
                WorkerSpec::new(worker_key(&row.id), row.name.clone())
                    .with_backoff(self.inner.options.backoff),
                body,
            )
            .await?;
        Ok(())
    }

    async fn halt(&self, row: &IntegrationAccountRow) -> Result<(), AppError> {
        let removed = self
            .inner
            .options
            .workers
            .remove(&worker_key(&row.id))
            .await;
        if removed && let Ok(provider) = self.provider(row.kind) {
            provider.stop(&self.account_dto(row)?).await;
        }
        self.withdraw(&row.id)
    }

    fn expose(
        &self,
        row: &IntegrationAccountRow,
        provider: &dyn IntegrationProvider,
    ) -> Result<(), AppError> {
        for spec in provider.tools() {
            let key = account_tool_key(&spec.tool.name, &row.id);
            self.inner.options.tools.register(&ToolDescriptor {
                source_ref: Some(row.id.clone()),
                display_name: Some(format!("{} · {}", spec.tool.label, row.name)),
                description: spec.tool.description.clone(),
                input_schema: spec.input_schema.clone(),
                read_only: !spec.tool.side_effect,
                destructive: false,
                tier: spec.tool.side_effect.then_some(ToolTier::Ask),
                ..ToolDescriptor::new(key.clone(), ToolSource::Integration)
            })?;
            register_integration_tool_node(
                &self.inner.options.nodes,
                key,
                IntegrationCallTarget {
                    account_id: row.id.clone(),
                    tool: spec.tool.name.clone(),
                },
                spec.tool.side_effect,
                self.caller(),
            )?;
        }
        Ok(())
    }

    fn withdraw(&self, id: &str) -> Result<(), AppError> {
        for tool in self
            .inner
            .options
            .tools
            .deregister_source(ToolSource::Integration, Some(id))?
        {
            self.inner.options.nodes.unregister(&tool.key);
        }
        Ok(())
    }

    pub(super) fn caller(&self) -> IntegrationToolCaller {
        let weak: Weak<Inner> = Arc::downgrade(&self.inner);
        Arc::new(move |target, input, cancel| {
            let weak = weak.clone();
            async move {
                let inner = weak
                    .upgrade()
                    .ok_or_else(|| not_found(&target.account_id))?;
                Self { inner }.call(target, input, cancel).await
            }
            .boxed()
        })
    }

    async fn call(
        &self,
        target: IntegrationCallTarget,
        input: Map<String, Value>,
        cancel: CancellationToken,
    ) -> Result<Value, AppError> {
        let row = self.require(&target.account_id)?;
        let provider = self.provider(row.kind)?;
        let label = provider
            .tools()
            .into_iter()
            .find(|spec| spec.tool.name == target.tool)
            .map_or_else(|| target.tool.clone(), |spec| spec.tool.label);
        self.bump(&row.id, IntegrationCounter::ToolCalls, 1);
        self.note(
            &row.id,
            IntegrationActivityLevel::Info,
            &format!("Вызов инструмента «{label}»"),
        );
        let outcome = provider
            .call_tool(IntegrationToolCall {
                account: self.account_dto(&row)?,
                config: self.view(&row),
                tool: target.tool,
                input,
                cancel,
            })
            .await;
        if let Err(error) = &outcome {
            self.bump(&row.id, IntegrationCounter::Errors, 1);
            self.note(&row.id, IntegrationActivityLevel::Error, &error.message);
        }
        outcome
    }

    pub fn dispatch(
        &self,
        account_id: &str,
        event: &IntegrationEvent,
    ) -> Result<Dispatched, AppError> {
        let row = self.require(account_id)?;
        let provider = self.provider(row.kind)?;
        if !provider
            .triggers()
            .iter()
            .any(|trigger| trigger.name == event.trigger)
        {
            return Err(
                AppError::new(AppErrorCode::ValidationFailed, UNKNOWN_TRIGGER)
                    .with_detail("trigger", event.trigger.as_str()),
            );
        }
        check::non_empty("eventId", &event.event_id)?;
        check::max_len("eventId", &event.event_id, EVENT_ID_MAX)?;
        let now = self.now();
        let fresh = self.database().with_connection(|connection| {
            Ok(Repositories::new(connection).integrations.record_event(
                &row.id,
                &event.trigger,
                &event.event_id,
                now,
            )?)
        })?;
        if !fresh {
            self.bump(&row.id, IntegrationCounter::Duplicates, 1);
            self.note(
                &row.id,
                IntegrationActivityLevel::Info,
                &format!("Повтор события «{}» пропущен", event.trigger),
            );
            return Ok(Dispatched::Duplicate);
        }
        self.bump(&row.id, IntegrationCounter::Events, 1);
        self.note(
            &row.id,
            IntegrationActivityLevel::Info,
            &format!("Событие «{}» принято", event.trigger),
        );
        let sink = self
            .inner
            .sink
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let Some(sink) = sink else {
            return Ok(Dispatched::Started(Vec::new()));
        };
        let input = json!({
            "accountId": row.id,
            "kind": row.kind,
            "trigger": event.trigger,
            "eventId": event.event_id,
            "payload": event.payload,
        });
        match sink(&input) {
            Ok(started) => {
                if !started.is_empty() {
                    self.bump(&row.id, IntegrationCounter::Runs, started.len() as u64);
                }
                for name in &started {
                    self.note(
                        &row.id,
                        IntegrationActivityLevel::Info,
                        &format!("Запущен сценарий «{name}»"),
                    );
                }
                Ok(Dispatched::Started(started))
            }
            Err(error) => {
                self.bump(&row.id, IntegrationCounter::Errors, 1);
                self.note(&row.id, IntegrationActivityLevel::Error, &error.message);
                Err(error)
            }
        }
    }

    pub(super) fn note(&self, account_id: &str, level: IntegrationActivityLevel, message: &str) {
        let row = IntegrationActivityRow {
            account_id: account_id.to_owned(),
            at: self.now(),
            level,
            message: bounded(message),
        };
        if let Err(error) = self.database().with_connection(|connection| {
            Ok(Repositories::new(connection)
                .integrations
                .add_activity(&row, ACTIVITY_KEEP)?)
        }) {
            self.log(
                LogLevel::Warn,
                "Could not record integration activity",
                &[
                    ("accountId", json!(account_id)),
                    ("error", json!(error.message)),
                ],
            );
        }
    }

    fn bump(&self, account_id: &str, counter: IntegrationCounter, by: u64) {
        let at = self.now();
        if let Err(error) = self.database().with_connection(|connection| {
            Ok(Repositories::new(connection)
                .integrations
                .bump(account_id, counter, by, at)?)
        }) {
            self.log(
                LogLevel::Warn,
                "Could not update integration counters",
                &[
                    ("accountId", json!(account_id)),
                    ("error", json!(error.message)),
                ],
            );
        }
    }

    fn record_failure(&self, account_id: &str, message: &str) {
        self.bump(account_id, IntegrationCounter::Errors, 1);
        self.note(account_id, IntegrationActivityLevel::Error, message);
        if let Err(error) = self.patch(
            account_id,
            &IntegrationPatch {
                last_error: Some(Some(bounded(message))),
                ..IntegrationPatch::default()
            },
        ) {
            self.log(
                LogLevel::Warn,
                "Could not record an integration failure",
                &[
                    ("accountId", json!(account_id)),
                    ("error", json!(error.message)),
                ],
            );
        }
    }

    fn observe(&self, worker: &WorkerDto) {
        let Some(id) = worker.key.strip_prefix(WORKER_PREFIX) else {
            return;
        };
        let changed = {
            let mut observed = self
                .inner
                .observed
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let current = (worker.status, worker.restarts);
            observed.insert(worker.key.clone(), current) != Some(current)
        };
        if !changed {
            return;
        }
        match worker.status {
            WorkerStatus::Running => {
                self.note(id, IntegrationActivityLevel::Info, CONNECTED);
                if let Err(error) = self.patch(
                    id,
                    &IntegrationPatch {
                        last_error: Some(None),
                        ..IntegrationPatch::default()
                    },
                ) && error.code != AppErrorCode::NotFound
                {
                    self.log(
                        LogLevel::Warn,
                        "Could not clear an integration failure",
                        &[("accountId", json!(id)), ("error", json!(error.message))],
                    );
                }
            }
            WorkerStatus::Retrying | WorkerStatus::Failed => {
                if let Some(message) = &worker.last_error {
                    self.record_failure(id, message);
                }
            }
            WorkerStatus::Starting | WorkerStatus::Stopped => {}
        }
    }
}
