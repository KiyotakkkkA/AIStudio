use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use futures::FutureExt;
use futures::future::{BoxFuture, Shared, join_all};
use serde_json::{Map, Value, json};
use tokio::runtime::Handle;
use tokio::task::AbortHandle;
use tokio_util::sync::CancellationToken;
use zvs_contract::mcp::{MCP_INSTRUCTIONS_MAX, check_namespace};
use zvs_contract::validate::{invalid, js_slice};
use zvs_contract::{
    AppError, AppErrorCode, CreateMcpServerInput, McpCallResultDto, McpCapabilityDto,
    McpCapabilityKind, McpHealthDto, McpLogLevel, McpLogLineDto, McpOAuthState, McpServerDetailDto,
    McpServerDto, McpServerId, McpServerStatus, McpTestResultDto, McpTransport, McpValue,
    Timestamp, ToolSource, UpdateMcpServerInput, ids::is_wire_uuid,
};

use crate::data::repositories::{CapabilityCounts, McpServerPatch};
use crate::data::schema::{McpCapabilityRow, McpServerRow, SecretConsumerKind};
use crate::data::{Database, Repositories};
use crate::drivers::mcp::oauth::{
    self, AUTH_REQUIRED, ClientRegistration, CodeExchange, Loopback, OAuthGrant, Pkce,
    SIGN_IN_TIMEOUT, is_auth_required,
};
use crate::drivers::mcp::{
    LaunchSpec, McpClient, McpClientOptions, McpDiscovery, McpEndpoint, SpawnServer,
    TransportHooks, call_result, infer_destructive, infer_read_only, isolated_toolchain_env,
};
use crate::kernel::{
    McpCallTarget, McpNodeTarget, McpToolCaller, NodeRegistry, register_mcp_tool_node,
};
use crate::platform::clock::{SharedClock, SystemClock};
use crate::platform::external::UrlOpener;
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};
use crate::services::SecretService;
use crate::services::crypto::Plaintext;
use crate::toolchain::CommandLauncher;
use crate::tools::{ToolDescriptor, ToolRegistry};

pub const DAY_MS: Timestamp = 24 * 60 * 60 * 1000;
pub const LOG_LINES: usize = 50;
pub const CALL_SAMPLES: usize = 500;
pub const SERVER_NOT_FOUND: &str = "MCP-сервер не найден";
pub const SERVICE_STOPPED: &str = "Сервис MCP остановлен";
pub const SERVER_DISABLED: &str = "MCP-сервер выключен";
pub const NOT_CONNECTED: &str = "MCP-сервер не подключён";
pub const CANCELLED: &str = "Операция отменена";
pub const OAUTH_HTTP_ONLY: &str = "Вход через браузер доступен только для http-серверов";
pub const OAUTH_NO_REGISTRATION: &str =
    "Сервер авторизации не поддерживает автоматическую регистрацию клиентов";
pub const RESOURCE_READER: &str = "read_resource";
pub const RESOURCE_NOT_FOUND: &str = "У MCP-сервера нет такого ресурса";
pub const OAUTH_CORRUPT: &str = "Сохранённый вход повреждён — войдите заново";

pub struct McpServiceOptions {
    pub database: Arc<Database>,
    pub tools: ToolRegistry,
    pub nodes: Arc<NodeRegistry>,
    pub secrets: Arc<SecretService>,
    pub http: reqwest::Client,
    pub logger: SharedLogger,
    pub clock: SharedClock,
    pub spawn: Option<SpawnServer>,
    pub request_timeout: Duration,
    pub call_timeout: Duration,
    pub backoff: Duration,
    pub max_backoff: Duration,
    pub max_restarts: u64,
    pub client_version: String,
    pub toolchain_dir: Option<PathBuf>,
    pub toolchain: Option<Arc<dyn CommandLauncher>>,
    pub runtime: Option<Handle>,
    pub opener: Option<UrlOpener>,
    pub client_name: String,
    pub sign_in_timeout: Duration,
}

impl McpServiceOptions {
    pub fn new(
        database: Arc<Database>,
        tools: ToolRegistry,
        nodes: Arc<NodeRegistry>,
        secrets: Arc<SecretService>,
        http: reqwest::Client,
    ) -> Self {
        Self {
            database,
            tools,
            nodes,
            secrets,
            http,
            logger: silent_logger(),
            clock: Arc::new(SystemClock),
            spawn: None,
            request_timeout: Duration::from_secs(30),
            call_timeout: Duration::from_secs(60),
            backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(60),
            max_restarts: 5,
            client_version: "0.0.0".to_owned(),
            toolchain_dir: None,
            toolchain: None,
            runtime: None,
            opener: None,
            client_name: "ZVS AI Studio".to_owned(),
            sign_in_timeout: SIGN_IN_TIMEOUT,
        }
    }
}

type Starting = Shared<BoxFuture<'static, Result<(), AppError>>>;

#[derive(Debug, Clone, Copy)]
struct CallSample {
    at: Timestamp,
    ms: u64,
    failed: bool,
}

#[derive(Default)]
struct Supervisor {
    client: Option<Arc<McpClient>>,
    starting: Option<Starting>,
    started_at: Option<Timestamp>,
    restarts: u64,
    failures: u64,
    calls: VecDeque<CallSample>,
    log: VecDeque<McpLogLineDto>,
    retry: Option<AbortHandle>,
    intentional: bool,
    generation: u64,
    sign_in: Option<CancellationToken>,
}

struct Inner {
    options: McpServiceOptions,
    supervisors: Mutex<HashMap<String, Supervisor>>,
    closed: AtomicBool,
}

#[derive(Clone)]
pub struct McpService {
    inner: Arc<Inner>,
}

pub trait McpServers: Send + Sync {
    fn usable(&self) -> HashSet<String>;
}

pub fn derive_namespace(name: &str) -> String {
    let parts: Vec<String> = name
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_lowercase)
        .collect();
    let joined: String = parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            if index == 0 {
                part.clone()
            } else {
                let mut characters = part.chars();
                characters.next().map_or_else(String::new, |first| {
                    first.to_ascii_uppercase().to_string() + characters.as_str()
                })
            }
        })
        .collect();
    let trimmed = joined.trim_start_matches(|character: char| !character.is_ascii_alphabetic());
    let mut characters = trimmed.chars();
    let Some(first) = characters.next() else {
        return "server".to_owned();
    };
    let namespace = first.to_ascii_lowercase().to_string() + characters.as_str();
    namespace.chars().take(48).collect()
}

pub fn tool_key(namespace: &str, tool: &str) -> String {
    let mut suffix = String::new();
    let mut in_run = false;
    for character in tool.chars() {
        if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
            suffix.push(character);
            in_run = false;
        } else if !in_run {
            suffix.push('-');
            in_run = true;
        }
    }
    let suffix = suffix.trim_matches('-');
    if suffix.is_empty() {
        format!("{namespace}.call")
    } else {
        format!(
            "{namespace}.{}",
            suffix.chars().take(64).collect::<String>()
        )
    }
}

fn median(sorted: &[u64]) -> f64 {
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[middle] as f64
    } else {
        (sorted[middle - 1] + sorted[middle]) as f64 / 2.0
    }
}

fn id_text() -> String {
    new_id().hyphenated().to_string()
}

fn not_found(id: &str) -> AppError {
    AppError::new(AppErrorCode::NotFound, SERVER_NOT_FOUND).with_detail("serverId", id)
}

fn malformed() -> AppError {
    AppError::new(
        AppErrorCode::Unknown,
        "MCP-сервер хранится в повреждённом виде",
    )
}

fn capability_dto(row: McpCapabilityRow) -> Result<McpCapabilityDto, AppError> {
    Ok(McpCapabilityDto {
        id: row.id,
        server_id: McpServerId::parse(&row.server_id).ok_or_else(malformed)?,
        kind: row.kind,
        name: row.name,
        title: row.title,
        description: row.description,
        tool_key: row.tool_key,
        uri: row.uri,
        mime_type: row.mime_type,
        schema: row.schema,
        read_only: row.read_only,
        destructive: row.destructive,
        discovered_at: row.discovered_at,
    })
}

fn oauth_state(row: &McpServerRow, signed_in: bool) -> McpOAuthState {
    if row.last_error.as_deref() == Some(AUTH_REQUIRED) {
        McpOAuthState::Required
    } else if signed_in {
        McpOAuthState::SignedIn
    } else {
        McpOAuthState::None
    }
}

fn has_authorization(headers: &BTreeMap<String, String>) -> bool {
    headers
        .keys()
        .any(|name| name.eq_ignore_ascii_case("authorization"))
}

fn server_dto(
    row: &McpServerRow,
    counts: CapabilityCounts,
    signed_in: bool,
) -> Result<McpServerDto, AppError> {
    Ok(McpServerDto {
        id: McpServerId::parse(&row.id).ok_or_else(malformed)?,
        name: row.name.clone(),
        namespace: row.namespace.clone(),
        transport: row.transport,
        command: row.command.clone(),
        args: row.args.clone(),
        env: row.env.clone(),
        url: row.url.clone(),
        headers: row.headers.clone(),
        enabled: row.enabled,
        startup: row.startup,
        status: row.status,
        oauth: oauth_state(row, signed_in),
        protocol_version: row.protocol_version.clone(),
        server_name: row.server_name.clone(),
        server_version: row.server_version.clone(),
        instructions: row.instructions.clone(),
        last_error: row.last_error.clone(),
        last_connected_at: row.last_connected_at,
        tool_count: counts.tool,
        resource_count: counts.resource,
        prompt_count: counts.prompt,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn validate_shape(
    transport: McpTransport,
    command: Option<&str>,
    url: Option<&str>,
) -> Result<(), AppError> {
    if transport == McpTransport::Stdio && command.is_none_or(str::is_empty) {
        return Err(
            AppError::new(AppErrorCode::ValidationFailed, "Укажите команду запуска")
                .with_detail("field", "command"),
        );
    }
    if transport == McpTransport::Http && url.is_none_or(str::is_empty) {
        return Err(
            AppError::new(AppErrorCode::ValidationFailed, "Укажите адрес сервера")
                .with_detail("field", "url"),
        );
    }
    Ok(())
}

fn description(value: Option<&str>) -> String {
    js_slice(value.unwrap_or_default().trim(), 4000).to_owned()
}

fn title(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(|title| js_slice(title, 200).to_owned())
}

impl McpService {
    pub fn new(options: McpServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                supervisors: Mutex::new(HashMap::new()),
                closed: AtomicBool::new(false),
            }),
        }
    }

    fn database(&self) -> &Database {
        &self.inner.options.database
    }

    fn now(&self) -> Timestamp {
        self.inner.options.clock.now()
    }

    fn supervisors(&self) -> MutexGuard<'_, HashMap<String, Supervisor>> {
        self.inner
            .supervisors
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn with_supervisor<T>(&self, id: &str, work: impl FnOnce(&mut Supervisor) -> T) -> T {
        work(self.supervisors().entry(id.to_owned()).or_default())
    }

    fn log(&self, level: LogLevel, message: &str, fields: &[(&str, Value)]) {
        self.inner.options.logger.log(level, "mcp", message, fields);
    }

    fn require(&self, id: &str) -> Result<McpServerRow, AppError> {
        if !is_wire_uuid(id) {
            return Err(invalid("id", "uuid"));
        }
        self.find(id)?.ok_or_else(|| not_found(id))
    }

    fn find(&self, id: &str) -> Result<Option<McpServerRow>, AppError> {
        self.database().with_connection(|connection| {
            Ok(Repositories::new(connection).mcp_servers.find_by_id(id)?)
        })
    }

    fn patch(&self, id: &str, patch: &McpServerPatch) -> Result<McpServerRow, AppError> {
        self.database()
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .mcp_servers
                    .update(id, patch)?)
            })?
            .ok_or_else(|| not_found(id))
    }

    fn counts(&self, id: &str) -> Result<CapabilityCounts, AppError> {
        self.database().with_connection(|connection| {
            Ok(Repositories::new(connection)
                .mcp_servers
                .capability_counts(id)?)
        })
    }

    pub fn list(&self) -> Result<Vec<McpServerDto>, AppError> {
        let (rows, counts, signed_in) = self.database().with_connection(|connection| {
            let servers = Repositories::new(connection).mcp_servers;
            let rows = servers.list()?;
            let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
            let counts = servers.counts_for_servers(&ids)?;
            let signed_in: HashSet<String> = servers.oauth_server_ids()?.into_iter().collect();
            Ok((rows, counts, signed_in))
        })?;
        rows.iter()
            .map(|row| {
                server_dto(
                    row,
                    counts.get(&row.id).copied().unwrap_or_default(),
                    signed_in.contains(&row.id),
                )
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Result<McpServerDetailDto, AppError> {
        let row = self.require(id)?;
        Ok(McpServerDetailDto {
            server: self.dto(&row)?,
            capabilities: self.capabilities(&row.id, None)?,
        })
    }

    pub fn server(&self, id: &str) -> Result<McpServerDto, AppError> {
        let row = self.require(id)?;
        self.dto(&row)
    }

    fn dto(&self, row: &McpServerRow) -> Result<McpServerDto, AppError> {
        let signed_in = self.oauth_cipher(&row.id)?.is_some();
        server_dto(row, self.counts(&row.id)?, signed_in)
    }

    pub fn capabilities(
        &self,
        id: &str,
        kind: Option<McpCapabilityKind>,
    ) -> Result<Vec<McpCapabilityDto>, AppError> {
        let row = self.require(id)?;
        self.database()
            .with_connection(|connection| {
                let servers = Repositories::new(connection).mcp_servers;
                Ok(match kind {
                    None => servers.capabilities(&row.id)?,
                    Some(kind) => servers.capabilities_of_kind(&row.id, kind)?,
                })
            })?
            .into_iter()
            .map(capability_dto)
            .collect()
    }

    fn unique_namespace(&self, candidate: &str, own_id: Option<&str>) -> Result<String, AppError> {
        check_namespace("namespace", candidate)?;
        let base = zvs_contract::validate::js_trim(candidate).to_owned();
        for suffix in 0..100 {
            let namespace = if suffix == 0 {
                base.clone()
            } else {
                format!("{base}{}", suffix + 1)
            };
            let taken = self.database().with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .mcp_servers
                    .find_by_namespace(&namespace)?)
            })?;
            if taken.is_none_or(|row| Some(row.id.as_str()) == own_id) {
                return Ok(namespace);
            }
        }
        Err(AppError::new(
            AppErrorCode::Conflict,
            "Не удалось подобрать свободное пространство имён",
        ))
    }

    fn sync_secret_usage(
        &self,
        id: &str,
        before: &[String],
        after: &[String],
    ) -> Result<(), AppError> {
        let secrets = &self.inner.options.secrets;
        for secret in before.iter().filter(|entry| !after.contains(entry)) {
            secrets.remove_usage(secret, SecretConsumerKind::McpServer, id)?;
        }
        for secret in after.iter().filter(|entry| !before.contains(entry)) {
            secrets.add_usage(secret, SecretConsumerKind::McpServer, id)?;
        }
        Ok(())
    }

    pub fn create(&self, input: &CreateMcpServerInput) -> Result<McpServerDto, AppError> {
        zvs_contract::Validate::validate(input)?;
        let command = input.command();
        let url = input.url();
        validate_shape(input.transport, command.as_deref(), url.as_deref())?;
        let now = self.now();
        let namespace = self.unique_namespace(
            &input
                .namespace()
                .map_or_else(|| derive_namespace(input.name()), str::to_owned),
            None,
        )?;
        let enabled = input.enabled();
        let row = self.database().with_connection(|connection| {
            Ok(Repositories::new(connection)
                .mcp_servers
                .insert(&McpServerRow {
                    id: id_text(),
                    name: input.name().to_owned(),
                    namespace,
                    transport: input.transport,
                    command,
                    args: input.args(),
                    env: input.env(),
                    url,
                    headers: input.headers(),
                    enabled,
                    startup: input.startup(),
                    status: if enabled {
                        McpServerStatus::Stopped
                    } else {
                        McpServerStatus::Disabled
                    },
                    protocol_version: None,
                    server_name: None,
                    server_version: None,
                    instructions: None,
                    last_error: None,
                    last_connected_at: None,
                    created_at: now,
                    updated_at: now,
                })?)
        })?;
        self.sync_secret_usage(&row.id, &[], &row.secret_ids())?;
        self.log(
            LogLevel::Info,
            "Added an MCP server",
            &[
                ("serverId", json!(row.id)),
                ("transport", json!(row.transport)),
            ],
        );
        server_dto(&row, CapabilityCounts::default(), false)
    }

    pub async fn update(&self, input: &UpdateMcpServerInput) -> Result<McpServerDto, AppError> {
        zvs_contract::Validate::validate(input)?;
        let existing = self.require(&input.id.to_string())?;
        let transport = input.transport.unwrap_or(existing.transport);
        let command = input.command().unwrap_or_else(|| existing.command.clone());
        let url = input.url().unwrap_or_else(|| existing.url.clone());
        validate_shape(transport, command.as_deref(), url.as_deref())?;
        let namespace = input
            .namespace()
            .map(|namespace| self.unique_namespace(namespace, Some(&existing.id)))
            .transpose()?;
        self.stop(&existing.id).await;
        let enabled = input.enabled.unwrap_or(existing.enabled);
        let row = self.patch(
            &existing.id,
            &McpServerPatch {
                name: input.name().map(str::to_owned),
                namespace,
                transport: input.transport,
                command: input.command(),
                args: input.args.clone(),
                env: input.env(),
                url: input.url(),
                headers: input.headers(),
                enabled: input.enabled,
                startup: input.startup,
                status: Some(if enabled {
                    McpServerStatus::Stopped
                } else {
                    McpServerStatus::Disabled
                }),
                last_error: Some(None),
                updated_at: Some(self.now()),
                ..McpServerPatch::default()
            },
        )?;
        self.sync_secret_usage(&row.id, &existing.secret_ids(), &row.secret_ids())?;
        if row.url != existing.url || row.transport != existing.transport {
            self.forget_grant(&row.id)?;
        }
        if !row.enabled {
            self.withdraw(&row)?;
        }
        self.dto(&row)
    }

    pub async fn remove(&self, id: &str) -> Result<(), AppError> {
        let row = self.require(id)?;
        self.abort_sign_in(&row.id);
        self.stop(&row.id).await;
        self.withdraw(&row)?;
        self.sync_secret_usage(&row.id, &row.secret_ids(), &[])?;
        self.database().with_connection(|connection| {
            Ok(Repositories::new(connection).mcp_servers.remove(&row.id)?)
        })?;
        self.supervisors().remove(&row.id);
        self.log(
            LogLevel::Info,
            "Removed an MCP server",
            &[("serverId", json!(row.id))],
        );
        Ok(())
    }

    pub async fn start_auto_start(&self) -> Result<(), AppError> {
        let rows = self.database().with_connection(|connection| {
            Ok(Repositories::new(connection)
                .mcp_servers
                .list_auto_start()?)
        })?;
        join_all(rows.into_iter().map(|row| async move {
            if let Err(error) = self.start(&row.id).await {
                self.log(
                    LogLevel::Warn,
                    "An MCP server configured for launch did not start",
                    &[("serverId", json!(row.id)), ("raw", json!(error.message))],
                );
            }
        }))
        .await;
        Ok(())
    }

    pub async fn start(&self, id: &str) -> Result<(), AppError> {
        let row = self.require(id)?;
        if !row.enabled {
            return Err(AppError::new(AppErrorCode::Conflict, SERVER_DISABLED)
                .with_detail("serverId", row.id));
        }
        let starting = self.with_supervisor(&row.id, |supervisor| {
            supervisor.intentional = false;
            if let Some(retry) = supervisor.retry.take() {
                retry.abort();
            }
            if supervisor
                .client
                .as_ref()
                .is_some_and(|client| client.connected())
            {
                return None;
            }
            if let Some(starting) = &supervisor.starting {
                return Some(starting.clone());
            }
            let service = self.clone();
            let id = row.id.clone();
            let task = async move {
                let outcome = service.launch(&id).await;
                service.with_supervisor(&id, |supervisor| supervisor.starting = None);
                outcome
            };
            let handle = match &self.inner.options.runtime {
                Some(runtime) => runtime.spawn(task),
                None => tokio::spawn(task),
            };
            let shared: Starting = async move {
                handle.await.unwrap_or_else(|_| {
                    Err(AppError::new(
                        AppErrorCode::Unknown,
                        "Запуск MCP-сервера прервался",
                    ))
                })
            }
            .boxed()
            .shared();
            supervisor.starting = Some(shared.clone());
            Some(shared)
        });
        match starting {
            None => Ok(()),
            Some(starting) => starting.await,
        }
    }

    pub async fn stop(&self, id: &str) {
        let starting = {
            let mut supervisors = self.supervisors();
            let Some(supervisor) = supervisors.get_mut(id) else {
                return;
            };
            supervisor.intentional = true;
            if let Some(retry) = supervisor.retry.take() {
                retry.abort();
            }
            supervisor.starting.clone()
        };
        if let Some(starting) = starting {
            let _ = starting.await;
        }
        let client = self.with_supervisor(id, |supervisor| {
            supervisor.started_at = None;
            supervisor.client.take()
        });
        if let Some(client) = client {
            client.close().await;
        }
        if let Ok(Some(row)) = self.find(id) {
            let _ = self.set_status(
                &row.id,
                if row.enabled {
                    McpServerStatus::Stopped
                } else {
                    McpServerStatus::Disabled
                },
            );
        }
    }

    pub async fn restart(&self, id: &str) -> Result<(), AppError> {
        self.stop(id).await;
        self.start(id).await
    }

    pub async fn test(&self, id: &str) -> Result<McpTestResultDto, AppError> {
        let row = self.require(id)?;
        self.stop(&row.id).await;
        if let Err(error) = self.start(&row.id).await {
            return Ok(McpTestResultDto {
                ok: false,
                latency_ms: 0,
                protocol_version: None,
                server_name: None,
                server_version: None,
                tool_count: 0,
                resource_count: 0,
                prompt_count: 0,
                error: Some(js_slice(&error.message, 2000).to_owned()),
            });
        }
        let connected = self.require(&row.id)?;
        let counts = self.counts(&row.id)?;
        let latency_ms = self.with_supervisor(&row.id, |supervisor| {
            supervisor.calls.back().map_or(0, |call| call.ms)
        });
        Ok(McpTestResultDto {
            ok: true,
            latency_ms,
            protocol_version: connected.protocol_version,
            server_name: connected.server_name,
            server_version: connected.server_version,
            tool_count: counts.tool,
            resource_count: counts.resource,
            prompt_count: counts.prompt,
            error: None,
        })
    }

    pub async fn dispose(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);
        let ids: Vec<String> = self.supervisors().keys().cloned().collect();
        for id in &ids {
            self.abort_sign_in(id);
        }
        join_all(ids.iter().map(|id| self.stop(id))).await;
        self.supervisors().clear();
    }

    pub async fn call(
        &self,
        target: &McpCallTarget,
        arguments: Map<String, Value>,
        cancel: &CancellationToken,
    ) -> Result<McpCallResultDto, AppError> {
        if self.inner.closed.load(Ordering::SeqCst) {
            return Err(AppError::new(AppErrorCode::Conflict, SERVICE_STOPPED));
        }
        if cancel.is_cancelled() {
            return Err(AppError::new(AppErrorCode::RunCancelled, CANCELLED));
        }
        self.start(&target.server_id).await?;
        let client = self
            .with_supervisor(&target.server_id, |supervisor| supervisor.client.clone())
            .filter(|client| client.connected())
            .ok_or_else(|| {
                AppError::new(AppErrorCode::McpUnavailable, NOT_CONNECTED)
                    .with_detail("serverId", target.server_id.clone())
            })?;
        let started_at = self.now();
        let timeout = target.timeout.unwrap_or(self.inner.options.call_timeout);
        let outcome = tokio::select! {
            result = self.invoke(&client, target, &arguments, timeout) => result,
            () = cancel.cancelled() => Err(AppError::new(AppErrorCode::RunCancelled, CANCELLED)),
        };
        let outcome = match outcome {
            Err(error) if is_auth_required(&error) && self.can_refresh(&target.server_id) => {
                self.fresh_grant(&target.server_id, true).await?;
                self.restart(&target.server_id).await?;
                let client = self
                    .with_supervisor(&target.server_id, |supervisor| supervisor.client.clone())
                    .filter(|client| client.connected())
                    .ok_or_else(|| {
                        AppError::new(AppErrorCode::McpUnavailable, NOT_CONNECTED)
                            .with_detail("serverId", target.server_id.clone())
                    })?;
                tokio::select! {
                    result = self.invoke(&client, target, &arguments, timeout) => result,
                    () = cancel.cancelled() => Err(AppError::new(AppErrorCode::RunCancelled, CANCELLED)),
                }
            }
            outcome => outcome,
        };
        let elapsed = self.now() - started_at;
        match outcome {
            Ok(value) => {
                let dto = call_result(&value);
                self.record_call(&target.server_id, elapsed, dto.is_error);
                self.append(
                    &target.server_id,
                    if dto.is_error {
                        McpLogLevel::Warn
                    } else {
                        McpLogLevel::Info
                    },
                    &format!(
                        "tools/call {} {} {}ms",
                        target.tool,
                        if dto.is_error { "error" } else { "ok" },
                        elapsed.max(0)
                    ),
                );
                Ok(dto)
            }
            Err(error) => {
                self.record_call(&target.server_id, elapsed, true);
                self.append(
                    &target.server_id,
                    McpLogLevel::Error,
                    &format!("tools/call {} — {}", target.tool, error.message),
                );
                if is_auth_required(&error) {
                    self.patch(
                        &target.server_id,
                        &McpServerPatch {
                            last_error: Some(Some(AUTH_REQUIRED.to_owned())),
                            updated_at: Some(self.now()),
                            ..McpServerPatch::default()
                        },
                    )?;
                }
                Err(error)
            }
        }
    }

    async fn invoke(
        &self,
        client: &McpClient,
        target: &McpCallTarget,
        arguments: &Map<String, Value>,
        timeout: Duration,
    ) -> Result<Value, AppError> {
        if !target.read_resource {
            return client
                .call_tool(&target.tool, arguments.clone(), timeout)
                .await;
        }
        let uri = arguments
            .get("uri")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|uri| !uri.is_empty())
            .ok_or_else(|| invalid("uri", "required"))?;
        let known: Vec<String> = self
            .capabilities(&target.server_id, Some(McpCapabilityKind::Resource))?
            .into_iter()
            .filter_map(|capability| capability.uri)
            .collect();
        if !known.iter().any(|candidate| candidate == uri) {
            return Err(AppError::new(AppErrorCode::NotFound, RESOURCE_NOT_FOUND)
                .with_detail("uri", uri)
                .with_detail("available", known));
        }
        client.read_resource(uri, timeout).await
    }

    pub async fn call_by_tool_key(
        &self,
        key: &str,
        arguments: Map<String, Value>,
        cancel: &CancellationToken,
    ) -> Result<McpCallResultDto, AppError> {
        let capability = self
            .database()
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .mcp_servers
                    .find_capability_by_tool_key(key)?)
            })?
            .ok_or_else(|| {
                AppError::new(AppErrorCode::NotFound, "Инструмент MCP не найден")
                    .with_detail("key", key)
            })?;
        self.call(
            &McpCallTarget {
                server_id: capability.server_id,
                tool: capability.name,
                timeout: None,
                read_resource: false,
            },
            arguments,
            cancel,
        )
        .await
    }

    pub fn health(&self, id: &str) -> Result<McpHealthDto, AppError> {
        let row = self.require(id)?;
        let now = self.now();
        let window = now - DAY_MS;
        let (started_at, restarts, restarting, recent, log) =
            self.with_supervisor(&row.id, |supervisor| {
                (
                    supervisor.started_at,
                    supervisor.restarts,
                    supervisor.retry.is_some(),
                    supervisor
                        .calls
                        .iter()
                        .filter(|call| call.at >= window)
                        .copied()
                        .collect::<Vec<_>>(),
                    supervisor.log.iter().cloned().collect::<Vec<_>>(),
                )
            });
        let mut durations: Vec<u64> = recent.iter().map(|call| call.ms).collect();
        durations.sort_unstable();
        Ok(McpHealthDto {
            server_id: McpServerId::parse(&row.id).ok_or_else(malformed)?,
            status: row.status,
            uptime_ms: started_at.map(|at| u64::try_from(now - at).unwrap_or(0)),
            restarts,
            restarting,
            calls: recent.len() as u64,
            errors: recent.iter().filter(|call| call.failed).count() as u64,
            median_latency_ms: (!durations.is_empty()).then(|| median(&durations).round() as u64),
            recent_log: log,
        })
    }

    fn resolve(
        &self,
        values: &BTreeMap<String, McpValue>,
    ) -> Result<BTreeMap<String, String>, AppError> {
        values
            .iter()
            .map(|(name, value)| match value {
                McpValue::Literal { value } => Ok((name.clone(), value.clone())),
                McpValue::Secret { secret_id } => self
                    .inner
                    .options
                    .secrets
                    .resolve(&secret_id.to_string())
                    .map(|plaintext| (name.clone(), plaintext.expose().to_owned()))
                    .map_err(|_| {
                        AppError::new(
                            AppErrorCode::SecretMissing,
                            format!("Не удалось получить значение для {name}"),
                        )
                        .with_detail("name", name.clone())
                    }),
            })
            .collect()
    }

    async fn endpoint(
        &self,
        row: &McpServerRow,
        force_refresh: bool,
    ) -> Result<McpEndpoint, AppError> {
        let env = self.resolve(&row.env)?;
        let mut headers = self.resolve(&row.headers)?;
        if row.transport == McpTransport::Http
            && !has_authorization(&headers)
            && let Some(grant) = self.fresh_grant(&row.id, force_refresh).await?
        {
            headers.insert(
                "Authorization".to_owned(),
                format!("Bearer {}", grant.access_token),
            );
        }
        match row.transport {
            McpTransport::Stdio => {
                let command = row
                    .command
                    .clone()
                    .filter(|command| !command.is_empty())
                    .ok_or_else(|| {
                        AppError::new(
                            AppErrorCode::ValidationFailed,
                            "У stdio-сервера не задана команда",
                        )
                    })?;
                let launched = self
                    .inner
                    .options
                    .toolchain
                    .as_ref()
                    .and_then(|toolchain| toolchain.launch_command(&command, &row.args));
                let mut spec = match launched {
                    Some(Ok(spec)) => spec,
                    Some(Err(error)) => {
                        return Err(AppError {
                            code: AppErrorCode::McpUnavailable,
                            ..error
                        });
                    }
                    None => LaunchSpec {
                        command,
                        args: row.args.clone(),
                        env: self
                            .inner
                            .options
                            .toolchain_dir
                            .as_deref()
                            .map(isolated_toolchain_env)
                            .unwrap_or_default(),
                        cwd: None,
                    },
                };
                spec.env.extend(env);
                Ok(McpEndpoint::Stdio {
                    spec,
                    spawn: self.inner.options.spawn.clone(),
                })
            }
            McpTransport::Http => {
                let url = row
                    .url
                    .clone()
                    .filter(|url| !url.is_empty())
                    .ok_or_else(|| {
                        AppError::new(
                            AppErrorCode::ValidationFailed,
                            "У http-сервера не задан адрес",
                        )
                    })?;
                Ok(McpEndpoint::Http {
                    url,
                    headers,
                    client: self.inner.options.http.clone(),
                })
            }
        }
    }

    fn oauth_cipher(&self, id: &str) -> Result<Option<Vec<u8>>, AppError> {
        self.database().with_connection(|connection| {
            Ok(Repositories::new(connection).mcp_servers.oauth_cipher(id)?)
        })
    }

    fn grant(&self, id: &str) -> Result<Option<OAuthGrant>, AppError> {
        let Some(cipher) = self.oauth_cipher(id)? else {
            return Ok(None);
        };
        let plaintext = self.inner.options.secrets.open(&cipher)?;
        serde_json::from_str(plaintext.expose())
            .map(Some)
            .map_err(|_| AppError::new(AppErrorCode::SecretDecryptFailed, OAUTH_CORRUPT))
    }

    fn save_grant(&self, id: &str, grant: &OAuthGrant) -> Result<(), AppError> {
        let json = serde_json::to_string(grant)
            .map_err(|cause| AppError::new(AppErrorCode::Unknown, cause.to_string()))?;
        let cipher = self.inner.options.secrets.seal(&Plaintext::new(json))?;
        let now = self.now();
        self.database().with_connection(|connection| {
            Ok(Repositories::new(connection)
                .mcp_servers
                .save_oauth(id, &cipher, now)?)
        })
    }

    fn forget_grant(&self, id: &str) -> Result<(), AppError> {
        self.database().with_connection(|connection| {
            Ok(Repositories::new(connection).mcp_servers.remove_oauth(id)?)
        })
    }

    fn can_refresh(&self, id: &str) -> bool {
        self.grant(id)
            .ok()
            .flatten()
            .is_some_and(|grant| grant.refresh_token.is_some())
    }

    async fn fresh_grant(&self, id: &str, force: bool) -> Result<Option<OAuthGrant>, AppError> {
        let grant = match self.grant(id) {
            Ok(grant) => grant,
            Err(_) => {
                self.forget_grant(id)?;
                return Err(oauth::auth_required(None));
            }
        };
        let Some(grant) = grant else {
            return Ok(None);
        };
        if !force && !grant.expires_soon(self.now()) {
            return Ok(Some(grant));
        }
        if grant.refresh_token.is_none() {
            return Err(oauth::auth_required(None));
        }
        match oauth::refresh(&self.inner.options.http, &grant, self.now()).await {
            Ok(refreshed) => {
                self.save_grant(id, &refreshed)?;
                self.append(id, McpLogLevel::Info, "oauth token refreshed");
                Ok(Some(refreshed))
            }
            Err(error) if is_auth_required(&error) => {
                self.append(id, McpLogLevel::Warn, &error.message);
                Err(oauth::auth_required(None))
            }
            Err(error) => Err(error),
        }
    }

    fn abort_sign_in(&self, id: &str) {
        if let Some(token) = self.with_supervisor(id, |supervisor| supervisor.sign_in.take()) {
            token.cancel();
        }
    }

    pub fn cancel_sign_in(&self, id: &str) -> Result<McpServerDto, AppError> {
        let row = self.require(id)?;
        self.abort_sign_in(&row.id);
        self.dto(&row)
    }

    pub async fn sign_out(&self, id: &str) -> Result<McpServerDto, AppError> {
        let row = self.require(id)?;
        self.abort_sign_in(&row.id);
        self.stop(&row.id).await;
        self.forget_grant(&row.id)?;
        self.withdraw(&row)?;
        if row.last_error.as_deref() == Some(AUTH_REQUIRED) {
            self.patch(
                &row.id,
                &McpServerPatch {
                    last_error: Some(None),
                    updated_at: Some(self.now()),
                    ..McpServerPatch::default()
                },
            )?;
        }
        self.append(&row.id, McpLogLevel::Info, "oauth signed out");
        self.server(&row.id)
    }

    pub async fn sign_in(&self, id: &str) -> Result<McpServerDto, AppError> {
        let row = self.require(id)?;
        if row.transport != McpTransport::Http {
            return Err(AppError::new(
                AppErrorCode::ValidationFailed,
                OAUTH_HTTP_ONLY,
            ));
        }
        if !row.enabled {
            return Err(AppError::new(AppErrorCode::Conflict, SERVER_DISABLED)
                .with_detail("serverId", row.id));
        }
        let url = row
            .url
            .clone()
            .filter(|url| !url.is_empty())
            .ok_or_else(|| {
                AppError::new(
                    AppErrorCode::ValidationFailed,
                    "У http-сервера не задан адрес",
                )
            })?;
        let cancel = CancellationToken::new();
        let previous = self.with_supervisor(&row.id, |supervisor| {
            supervisor.sign_in.replace(cancel.clone())
        });
        if let Some(previous) = previous {
            previous.cancel();
        }
        let outcome = self.authorize(&row.id, &url, &cancel).await;
        if !cancel.is_cancelled() {
            self.with_supervisor(&row.id, |supervisor| supervisor.sign_in = None);
        }
        let grant = match outcome {
            Ok(grant) => grant,
            Err(error) => {
                self.append(&row.id, McpLogLevel::Error, &error.message);
                return Err(error);
            }
        };
        self.save_grant(&row.id, &grant)?;
        self.append(&row.id, McpLogLevel::Info, "oauth signed in");
        self.log(
            LogLevel::Info,
            "Signed in to an MCP server",
            &[("serverId", json!(row.id)), ("issuer", json!(grant.issuer))],
        );
        self.stop(&row.id).await;
        let _ = self.start(&row.id).await;
        self.server(&row.id)
    }

    async fn authorize(
        &self,
        id: &str,
        url: &str,
        cancel: &CancellationToken,
    ) -> Result<OAuthGrant, AppError> {
        let http = &self.inner.options.http;
        let challenge = oauth::probe_challenge(http, url).await;
        let discovery = oauth::discover(http, url, challenge.as_deref()).await?;
        let previous = self.grant(id).ok().flatten().filter(|grant| {
            grant.issuer == discovery.issuer && grant.resource == discovery.resource
        });
        let loopback =
            Loopback::bind(previous.as_ref().and_then(OAuthGrant::redirect_port)).await?;
        let client = match previous
            .as_ref()
            .filter(|grant| grant.redirect_uri == loopback.redirect_uri)
        {
            Some(grant) => ClientRegistration {
                client_id: grant.client_id.clone(),
                client_secret: grant.client_secret.clone(),
            },
            None => {
                let endpoint = discovery
                    .server
                    .registration_endpoint
                    .clone()
                    .ok_or_else(|| {
                        AppError::new(AppErrorCode::McpUnavailable, OAUTH_NO_REGISTRATION)
                    })?;
                oauth::register(
                    http,
                    &endpoint,
                    &loopback.redirect_uri,
                    &self.inner.options.client_name,
                    discovery.scope.as_deref(),
                )
                .await?
            }
        };
        let pkce = Pkce::generate();
        let state = oauth::random_state();
        let redirect_uri = loopback.redirect_uri.clone();
        let address =
            oauth::authorize_url(&discovery, &client.client_id, &redirect_uri, &state, &pkce)?;
        let opener = self.inner.options.opener.clone().ok_or_else(|| {
            AppError::new(
                AppErrorCode::NotImplemented,
                crate::platform::external::OPEN_FAILED,
            )
        })?;
        opener(&address)?;
        self.append(
            id,
            McpLogLevel::Info,
            &format!("oauth sign-in opened in the browser · {}", discovery.issuer),
        );
        let code = loopback
            .wait_for_code(&state, self.inner.options.sign_in_timeout, cancel)
            .await?;
        oauth::exchange_code(
            http,
            CodeExchange {
                discovery: &discovery,
                client: &client,
                redirect_uri: &redirect_uri,
                code: &code,
                verifier: &pkce.verifier,
                now: self.now(),
            },
        )
        .await
    }

    fn hooks(&self, id: &str, generation: u64) -> TransportHooks {
        let weak: Weak<Inner> = Arc::downgrade(&self.inner);
        let log_weak = weak.clone();
        let log_id = id.to_owned();
        let malformed_weak = weak.clone();
        let malformed_id = id.to_owned();
        let closed_id = id.to_owned();
        TransportHooks::new(
            Arc::new(move |level, line| {
                if let Some(inner) = log_weak.upgrade() {
                    Self { inner }.append(&log_id, level, line);
                }
            }),
            Arc::new(move |payload| {
                if let Some(inner) = malformed_weak.upgrade() {
                    Self { inner }.log(
                        LogLevel::Error,
                        "An MCP server sent a malformed message",
                        &[("serverId", json!(malformed_id)), ("raw", json!(payload))],
                    );
                }
            }),
            Arc::new(move |reason| {
                if let Some(inner) = weak.upgrade() {
                    Self { inner }.died(&closed_id, generation, reason);
                }
            }),
        )
    }

    async fn launch(&self, id: &str) -> Result<(), AppError> {
        let row = self.require(id)?;
        self.set_status(id, McpServerStatus::Starting)?;
        let generation = self.with_supervisor(id, |supervisor| {
            supervisor.generation += 1;
            supervisor.generation
        });
        let options = McpClientOptions {
            client_version: self.inner.options.client_version.clone(),
            request_timeout: self.inner.options.request_timeout,
            clock: self.inner.options.clock.clone(),
        };
        let connected = async {
            let endpoint = self.endpoint(&row, false).await?;
            let attempt = McpClient::connect(endpoint, &options, self.hooks(id, generation)).await;
            let (client, handshake) = match attempt {
                Err(error) if is_auth_required(&error) && self.can_refresh(id) => {
                    let endpoint = self.endpoint(&row, true).await?;
                    McpClient::connect(endpoint, &options, self.hooks(id, generation)).await?
                }
                attempt => attempt?,
            };
            let client = Arc::new(client);
            self.with_supervisor(id, |supervisor| {
                supervisor.client = Some(Arc::clone(&client));
            });
            match client.discover(&handshake.capabilities).await {
                Ok(discovery) => Ok((client, handshake, discovery)),
                Err(error) => {
                    client.close().await;
                    Err(error)
                }
            }
        }
        .await;
        let (client, handshake, discovery) = match connected {
            Ok(connected) => connected,
            Err(error) => {
                self.with_supervisor(id, |supervisor| {
                    supervisor.client = None;
                    supervisor.failures += 1;
                });
                let message = if is_auth_required(&error) {
                    AUTH_REQUIRED.to_owned()
                } else {
                    error.message.clone()
                };
                self.patch(
                    id,
                    &McpServerPatch {
                        status: Some(McpServerStatus::Failed),
                        last_error: Some(Some(js_slice(&message, 2000).to_owned())),
                        updated_at: Some(self.now()),
                        ..McpServerPatch::default()
                    },
                )?;
                self.withdraw(&self.require(id)?)?;
                self.append(id, McpLogLevel::Error, &message);
                self.log(
                    LogLevel::Warn,
                    "An MCP server did not come up",
                    &[("serverId", json!(id)), ("raw", json!(message))],
                );
                return Err(error);
            }
        };
        let now = self.now();
        self.with_supervisor(id, |supervisor| {
            supervisor.failures = 0;
            supervisor.started_at = Some(now);
            supervisor.client = Some(Arc::clone(&client));
        });
        self.record_call(
            id,
            i64::try_from(handshake.latency_ms).unwrap_or(i64::MAX),
            false,
        );
        self.append(
            id,
            McpLogLevel::Info,
            &format!("initialize protocol {}", handshake.protocol_version),
        );
        self.append(
            id,
            McpLogLevel::Info,
            &format!(
                "tools/list {} tools · {} resources",
                discovery.tools.len(),
                discovery.resources.len()
            ),
        );
        let connected = self.patch(
            id,
            &McpServerPatch {
                status: Some(McpServerStatus::Connected),
                protocol_version: Some(Some(handshake.protocol_version.clone())),
                server_name: Some(
                    Some(handshake.server_name.clone()).filter(|name| !name.is_empty()),
                ),
                server_version: Some(
                    Some(handshake.server_version.clone()).filter(|version| !version.is_empty()),
                ),
                instructions: Some(
                    handshake
                        .instructions
                        .as_deref()
                        .map(|text| js_slice(text, MCP_INSTRUCTIONS_MAX).to_owned()),
                ),
                last_error: Some(None),
                last_connected_at: Some(Some(now)),
                updated_at: Some(now),
                ..McpServerPatch::default()
            },
        )?;
        self.adopt(&connected, &discovery)?;
        self.log(
            LogLevel::Info,
            "An MCP server is connected",
            &[
                ("serverId", json!(id)),
                ("protocolVersion", json!(handshake.protocol_version)),
                ("tools", json!(discovery.tools.len())),
            ],
        );
        Ok(())
    }

    fn caller(&self) -> McpToolCaller {
        let weak = Arc::downgrade(&self.inner);
        Arc::new(move |target, arguments, cancel| {
            let weak = weak.clone();
            async move {
                let Some(inner) = weak.upgrade() else {
                    return Err(AppError::new(AppErrorCode::Conflict, SERVICE_STOPPED));
                };
                Self { inner }.call(&target, arguments, &cancel).await
            }
            .boxed()
        })
    }

    fn adopt(&self, row: &McpServerRow, discovery: &McpDiscovery) -> Result<(), AppError> {
        let now = self.now();
        let mut kept: Vec<String> = Vec::new();
        let mut live: Vec<String> = Vec::new();
        let upsert = |capability: McpCapabilityRow| {
            self.database().with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .mcp_servers
                    .upsert_capability(&capability)?)
            })
        };
        for tool in &discovery.tools {
            let key = tool_key(&row.namespace, &tool.name);
            let read_only = infer_read_only(tool);
            let destructive = infer_destructive(tool);
            let schema = tool.input_schema.clone().unwrap_or(Value::Null);
            let tool_title = title(tool.title.as_deref());
            kept.push(
                upsert(McpCapabilityRow {
                    id: id_text(),
                    server_id: row.id.clone(),
                    kind: McpCapabilityKind::Tool,
                    name: tool.name.clone(),
                    title: tool_title.clone(),
                    description: description(tool.description.as_deref()),
                    tool_key: Some(key.clone()),
                    uri: None,
                    mime_type: None,
                    schema: schema.clone(),
                    read_only,
                    destructive,
                    discovered_at: now,
                })?
                .id,
            );
            live.push(key.clone());
            self.inner.options.tools.register(&ToolDescriptor {
                source_ref: Some(row.id.clone()),
                display_name: Some(tool_title.unwrap_or_else(|| key.clone())),
                description: description(tool.description.as_deref()),
                input_schema: schema,
                read_only,
                destructive,
                ..ToolDescriptor::new(key.clone(), ToolSource::Mcp)
            })?;
            register_mcp_tool_node(
                &self.inner.options.nodes,
                McpNodeTarget {
                    key,
                    server_id: row.id.clone(),
                    tool: tool.name.clone(),
                    read_only,
                    timeout: None,
                    read_resource: false,
                },
                self.caller(),
            )?;
        }
        for resource in &discovery.resources {
            let name = resource
                .name
                .clone()
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| resource.uri.clone());
            kept.push(
                upsert(McpCapabilityRow {
                    id: id_text(),
                    server_id: row.id.clone(),
                    kind: McpCapabilityKind::Resource,
                    name: js_slice(&name, 200).to_owned(),
                    title: title(resource.title.as_deref()),
                    description: description(resource.description.as_deref()),
                    tool_key: None,
                    uri: Some(resource.uri.clone()),
                    mime_type: resource.mime_type.clone(),
                    schema: Value::Null,
                    read_only: true,
                    destructive: false,
                    discovered_at: now,
                })?
                .id,
            );
        }
        for prompt in &discovery.prompts {
            kept.push(
                upsert(McpCapabilityRow {
                    id: id_text(),
                    server_id: row.id.clone(),
                    kind: McpCapabilityKind::Prompt,
                    name: prompt.name.clone(),
                    title: title(prompt.title.as_deref()),
                    description: description(prompt.description.as_deref()),
                    tool_key: None,
                    uri: None,
                    mime_type: None,
                    schema: Value::Array(prompt.arguments.clone().unwrap_or_default()),
                    read_only: true,
                    destructive: false,
                    discovered_at: now,
                })?
                .id,
            );
        }
        let reader = tool_key(&row.namespace, RESOURCE_READER);
        if !live.contains(&reader) {
            if discovery.resources.is_empty() {
                self.inner.options.tools.deregister(&reader)?;
                self.inner.options.nodes.unregister(&reader);
            } else {
                self.register_reader(row, &reader, discovery)?;
            }
        }
        let stale: Vec<String> = self
            .database()
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .mcp_servers
                    .capabilities_of_kind(&row.id, McpCapabilityKind::Tool)?)
            })?
            .into_iter()
            .filter_map(|capability| capability.tool_key)
            .filter(|key| !live.contains(key))
            .collect();
        for key in stale {
            self.inner.options.tools.deregister(&key)?;
            self.inner.options.nodes.unregister(&key);
        }
        self.database().with_connection(|connection| {
            Ok(Repositories::new(connection)
                .mcp_servers
                .prune_capabilities(&row.id, &kept)?)
        })
    }

    fn register_reader(
        &self,
        row: &McpServerRow,
        key: &str,
        discovery: &McpDiscovery,
    ) -> Result<(), AppError> {
        let server = row.server_name.clone().unwrap_or_else(|| row.name.clone());
        let listing: Vec<String> = discovery
            .resources
            .iter()
            .map(|resource| {
                let about = resource
                    .description
                    .as_deref()
                    .or(resource.title.as_deref())
                    .or(resource.name.as_deref())
                    .map(str::trim)
                    .filter(|about| !about.is_empty());
                match about {
                    Some(about) => format!("{} — {about}", resource.uri),
                    None => resource.uri.clone(),
                }
            })
            .collect();
        let uris: Vec<&str> = discovery
            .resources
            .iter()
            .map(|resource| resource.uri.as_str())
            .collect();
        self.inner.options.tools.register(&ToolDescriptor {
            source_ref: Some(row.id.clone()),
            display_name: Some(format!("{server}: чтение ресурсов")),
            description: description(Some(&format!(
                "Read a resource of the {server} MCP server, such as instructions the server asks you to read before using its tools. Resources: {}",
                listing.join("; ")
            ))),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "uri": {
                        "type": "string",
                        "enum": uris,
                        "description": "The resource URI to read.",
                    },
                },
                "required": ["uri"],
            }),
            read_only: true,
            destructive: false,
            ..ToolDescriptor::new(key.to_owned(), ToolSource::Mcp)
        })?;
        register_mcp_tool_node(
            &self.inner.options.nodes,
            McpNodeTarget {
                key: key.to_owned(),
                server_id: row.id.clone(),
                tool: RESOURCE_READER.to_owned(),
                read_only: true,
                timeout: None,
                read_resource: true,
            },
            self.caller(),
        )
    }

    fn withdraw(&self, row: &McpServerRow) -> Result<(), AppError> {
        for tool in self
            .inner
            .options
            .tools
            .deregister_source(ToolSource::Mcp, Some(&row.id))?
        {
            self.inner.options.nodes.unregister(&tool.key);
        }
        Ok(())
    }

    fn died(&self, id: &str, generation: u64, reason: &str) {
        let proceed = self.with_supervisor(id, |supervisor| {
            if supervisor.generation != generation || supervisor.starting.is_some() {
                return false;
            }
            supervisor.client = None;
            supervisor.started_at = None;
            !supervisor.intentional
        });
        if !proceed || self.inner.closed.load(Ordering::SeqCst) {
            return;
        }
        let Ok(Some(row)) = self.find(id) else {
            return;
        };
        if !row.enabled {
            return;
        }
        let _ = self.patch(
            id,
            &McpServerPatch {
                status: Some(McpServerStatus::Failed),
                last_error: Some(Some(js_slice(reason, 2000).to_owned())),
                updated_at: Some(self.now()),
                ..McpServerPatch::default()
            },
        );
        let _ = self.withdraw(&row);
        self.append(id, McpLogLevel::Error, reason);
        self.log(
            LogLevel::Warn,
            "An MCP server died",
            &[("serverId", json!(id)), ("raw", json!(reason))],
        );
        let max_restarts = self.inner.options.max_restarts;
        let failures = self.with_supervisor(id, |supervisor| {
            supervisor.failures += 1;
            supervisor.failures
        });
        if failures > max_restarts {
            self.append(
                id,
                McpLogLevel::Error,
                &format!("Перезапуск прекращён после {max_restarts} попыток"),
            );
            return;
        }
        let exponent = u32::try_from(failures.saturating_sub(1))
            .unwrap_or(u32::MAX)
            .min(30);
        let delay = self
            .inner
            .options
            .backoff
            .saturating_mul(2_u32.saturating_pow(exponent))
            .min(self.inner.options.max_backoff);
        self.append(
            id,
            McpLogLevel::Info,
            &format!("перезапуск через {} мс", delay.as_millis()),
        );
        let service = self.clone();
        let retry_id = id.to_owned();
        let task = async move {
            tokio::time::sleep(delay).await;
            service.with_supervisor(&retry_id, |supervisor| supervisor.retry = None);
            let _ = service.start(&retry_id).await;
        };
        let handle = match &self.inner.options.runtime {
            Some(runtime) => runtime.spawn(task),
            None => match Handle::try_current() {
                Ok(runtime) => runtime.spawn(task),
                Err(_) => return,
            },
        };
        self.with_supervisor(id, |supervisor| {
            supervisor.restarts += 1;
            supervisor.retry = Some(handle.abort_handle());
        });
    }

    fn record_call(&self, id: &str, ms: Timestamp, failed: bool) {
        let at = self.now();
        self.with_supervisor(id, |supervisor| {
            supervisor.calls.push_back(CallSample {
                at,
                ms: u64::try_from(ms.max(0)).unwrap_or(0),
                failed,
            });
            while supervisor.calls.len() > CALL_SAMPLES {
                supervisor.calls.pop_front();
            }
        });
    }

    fn append(&self, id: &str, level: McpLogLevel, line: &str) {
        let at = self.now();
        self.with_supervisor(id, |supervisor| {
            supervisor.log.push_front(McpLogLineDto {
                at,
                level,
                line: js_slice(line, 1000).to_owned(),
            });
            supervisor.log.truncate(LOG_LINES);
        });
    }

    fn set_status(&self, id: &str, status: McpServerStatus) -> Result<(), AppError> {
        let Some(row) = self.find(id)? else {
            return Ok(());
        };
        if row.status == status {
            return Ok(());
        }
        self.patch(
            id,
            &McpServerPatch {
                status: Some(status),
                updated_at: Some(self.now()),
                ..McpServerPatch::default()
            },
        )?;
        Ok(())
    }
}

impl McpServers for McpService {
    fn usable(&self) -> HashSet<String> {
        let live: HashSet<String> = self
            .supervisors()
            .iter()
            .filter(|(_, supervisor)| {
                supervisor
                    .client
                    .as_ref()
                    .is_some_and(|client| client.connected())
            })
            .map(|(id, _)| id.clone())
            .collect();
        if live.is_empty() {
            return HashSet::new();
        }
        let rows = self
            .database()
            .with_connection(|connection| Ok(Repositories::new(connection).mcp_servers.list()?))
            .unwrap_or_default();
        rows.into_iter()
            .filter(|row| {
                row.enabled
                    && row.status == McpServerStatus::Connected
                    && row.last_error.as_deref() != Some(AUTH_REQUIRED)
                    && live.contains(&row.id)
            })
            .map(|row| row.id)
            .collect()
    }
}

#[cfg(test)]
mod tests;
