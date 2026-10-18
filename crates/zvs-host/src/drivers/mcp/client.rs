use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex as SyncMutex, PoisonError};
use std::time::Duration;

use reqwest::header::{HeaderName, HeaderValue};
use rmcp::model::{
    CallToolRequest, CallToolRequestParams, ClientCapabilities, ClientConfig, ClientRequest,
    Implementation, ListPromptsRequest, ListResourcesRequest, ListToolsRequest,
    PaginatedRequestParams, ProtocolVersion, ServerResult,
};
use rmcp::service::ClientInitializeError;
use rmcp::service::{PeerRequestOptions, RunningService, ServiceError};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::{Peer, RoleClient, serve_client};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use tokio::sync::Mutex;
use zvs_contract::validate::invalid;
use zvs_contract::{AppError, AppErrorCode, McpLogLevel};

use super::http::McpHttpClient;
use super::protocol::{
    CLIENT_NAME, CLIENT_PROTOCOL_VERSION, McpDiscovery, McpPromptDescriptor, McpResourceDescriptor,
    McpToolDescriptor, SUPPORTED_PROTOCOL_VERSIONS,
};
use super::stdio::{LaunchSpec, SpawnServer, StdioTransport};
use super::transport::{MALFORMED_SENTINEL, STOPPED, TransportHooks};
use crate::platform::clock::SharedClock;

pub const MAX_PAGES: usize = 50;

pub enum McpEndpoint {
    Stdio {
        spec: LaunchSpec,
        spawn: Option<SpawnServer>,
    },
    Http {
        url: String,
        headers: BTreeMap<String, String>,
        client: reqwest::Client,
    },
}

#[derive(Clone)]
pub struct McpClientOptions {
    pub client_version: String,
    pub request_timeout: Duration,
    pub clock: SharedClock,
}

#[derive(Debug, Clone, PartialEq)]
pub struct McpHandshake {
    pub protocol_version: String,
    pub server_name: String,
    pub server_version: String,
    pub capabilities: Map<String, Value>,
    pub latency_ms: u64,
}

pub type FailureSlot = Arc<SyncMutex<Option<String>>>;

pub struct McpClient {
    service: Mutex<Option<RunningService<RoleClient, ClientConfig>>>,
    peer: Peer<RoleClient>,
    hooks: TransportHooks,
    failures: FailureSlot,
    request_timeout: Duration,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page<T> {
    #[serde(
        default = "Vec::new",
        alias = "tools",
        alias = "resources",
        alias = "prompts"
    )]
    items: Vec<T>,
    #[serde(default)]
    next_cursor: Option<String>,
}

fn unexpected(method: &str) -> AppError {
    AppError::new(
        AppErrorCode::Unknown,
        format!("MCP-сервер ответил неожиданным телом на {method}"),
    )
    .with_detail("method", method)
}

fn interrupted(method: &str, reason: &str) -> AppError {
    AppError::new(
        AppErrorCode::McpUnavailable,
        format!("MCP-сервер прервал {method}"),
    )
    .with_detail("reason", reason)
}

fn timed_out(method: &str, timeout: Duration) -> AppError {
    let ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
    AppError::new(
        AppErrorCode::McpTimeout,
        format!("MCP-сервер не ответил на {method} за {ms} мс"),
    )
    .with_detail("method", method)
    .with_detail("timeoutMs", ms)
}

fn take_failure(failures: &FailureSlot) -> Option<String> {
    failures
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
}

fn report_value(hooks: &TransportHooks, value: &impl serde::Serialize) {
    let payload = serde_json::to_string(value).unwrap_or_default();
    hooks.report(&payload);
}

fn header_map(
    headers: &BTreeMap<String, String>,
) -> Result<HashMap<HeaderName, HeaderValue>, AppError> {
    headers
        .iter()
        .map(|(name, value)| {
            let header = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| invalid(&format!("headers.{name}"), "pattern"))?;
            let value = HeaderValue::from_str(value)
                .map_err(|_| invalid(&format!("headers.{name}"), "value"))?;
            Ok((header, value))
        })
        .collect()
}

fn page_of<T: DeserializeOwned>(
    result: &ServerResult,
    method: &str,
    hooks: &TransportHooks,
) -> Result<Page<T>, AppError> {
    let matches = matches!(
        (method, result),
        ("tools/list", ServerResult::ListToolsResult(_))
            | ("resources/list", ServerResult::ListResourcesResult(_))
            | ("prompts/list", ServerResult::ListPromptsResult(_))
    );
    let value = serde_json::to_value(result).unwrap_or(Value::Null);
    if !matches {
        report_value(hooks, &value);
        return Err(unexpected(method));
    }
    serde_json::from_value(value.clone()).map_err(|_| {
        report_value(hooks, &value);
        unexpected(method)
    })
}

impl McpClient {
    pub async fn connect(
        endpoint: McpEndpoint,
        options: &McpClientOptions,
        hooks: TransportHooks,
    ) -> Result<(Self, McpHandshake), AppError> {
        let config = ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new(CLIENT_NAME, options.client_version.clone()),
        )
        .with_protocol_version(ProtocolVersion::V_2025_06_18);
        let failures: FailureSlot = Arc::new(SyncMutex::new(None));
        let timeout = options.request_timeout;
        let started_at = options.clock.now();
        let served = match endpoint {
            McpEndpoint::Stdio { spec, spawn } => {
                let transport = StdioTransport::open(&spec, spawn.as_ref(), hooks.clone())?;
                tokio::time::timeout(timeout, serve_client(config, transport)).await
            }
            McpEndpoint::Http {
                url,
                headers,
                client,
            } => {
                let mut transport_config = StreamableHttpClientTransportConfig::with_uri(url);
                transport_config.custom_headers = header_map(&headers)?;
                transport_config.allow_stateless = true;
                let transport = StreamableHttpClientTransport::with_client(
                    McpHttpClient::new(client, hooks.clone(), Arc::clone(&failures)),
                    transport_config,
                );
                tokio::time::timeout(timeout, serve_client(config, transport)).await
            }
        };
        let service = match served {
            Err(_) => {
                hooks.close(STOPPED);
                return Err(timed_out("initialize", timeout));
            }
            Ok(Err(error)) => {
                hooks.close(STOPPED);
                return Err(Self::initialize_error(error, &hooks, &failures));
            }
            Ok(Ok(service)) => service,
        };
        let info = service.peer().peer_info();
        let protocol_version = info
            .as_ref()
            .map(|info| info.protocol_version.to_string())
            .unwrap_or_else(|| CLIENT_PROTOCOL_VERSION.to_owned());
        let client = Self {
            peer: service.peer().clone(),
            service: Mutex::new(Some(service)),
            hooks,
            failures,
            request_timeout: timeout,
        };
        if !SUPPORTED_PROTOCOL_VERSIONS.contains(&protocol_version.as_str()) {
            client.close().await;
            return Err(AppError::new(
                AppErrorCode::McpUnavailable,
                format!("Сервер говорит на неподдерживаемой версии протокола: {protocol_version}"),
            )
            .with_detail("protocolVersion", protocol_version));
        }
        let server = info.as_ref().and_then(|info| info.server_info.clone());
        let capabilities = info
            .as_ref()
            .and_then(|info| serde_json::to_value(&info.capabilities).ok())
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        let latency_ms = u64::try_from(options.clock.now() - started_at).unwrap_or(0);
        let handshake = McpHandshake {
            protocol_version,
            server_name: server
                .as_ref()
                .map(|server| server.name.clone())
                .unwrap_or_default(),
            server_version: server.map(|server| server.version).unwrap_or_default(),
            capabilities,
            latency_ms,
        };
        Ok((client, handshake))
    }

    fn initialize_error(
        error: ClientInitializeError,
        hooks: &TransportHooks,
        failures: &FailureSlot,
    ) -> AppError {
        if let Some(message) = take_failure(failures) {
            return AppError::new(AppErrorCode::McpUnavailable, message);
        }
        match error {
            ClientInitializeError::ExpectedInitResult(result) => {
                report_value(hooks, &result);
                unexpected("initialize")
            }
            ClientInitializeError::ExpectedInitResponse(message) => {
                report_value(hooks, &message);
                unexpected("initialize")
            }
            ClientInitializeError::JsonRpcError(data) if data.message == MALFORMED_SENTINEL => {
                unexpected("initialize")
            }
            ClientInitializeError::JsonRpcError(data) => {
                let message = data.message.trim();
                AppError::new(
                    AppErrorCode::Unknown,
                    if message.is_empty() {
                        "MCP-сервер отклонил initialize".to_owned()
                    } else {
                        message.to_owned()
                    },
                )
                .with_detail("method", "initialize")
                .with_detail("code", data.code.0)
            }
            ClientInitializeError::ConnectionClosed(_)
            | ClientInitializeError::TransportError { .. }
            | ClientInitializeError::Cancelled => interrupted(
                "initialize",
                &hooks.closed_reason().unwrap_or_else(|| STOPPED.to_owned()),
            ),
            other => AppError::new(AppErrorCode::Unknown, other.to_string())
                .with_detail("method", "initialize"),
        }
    }

    fn service_error(&self, error: ServiceError, method: &str) -> AppError {
        if let Some(message) = take_failure(&self.failures) {
            return AppError::new(AppErrorCode::McpUnavailable, message);
        }
        match error {
            ServiceError::Timeout { timeout } => timed_out(method, timeout),
            ServiceError::McpError(data) if data.message == MALFORMED_SENTINEL => {
                unexpected(method)
            }
            ServiceError::McpError(data) => {
                let message = data.message.trim();
                AppError::new(
                    AppErrorCode::Unknown,
                    if message.is_empty() {
                        format!("MCP-сервер отклонил {method}")
                    } else {
                        message.to_owned()
                    },
                )
                .with_detail("method", method)
                .with_detail("code", data.code.0)
            }
            ServiceError::UnexpectedResponse => unexpected(method),
            ServiceError::TransportClosed
            | ServiceError::TransportSend(_)
            | ServiceError::Cancelled { .. } => interrupted(
                method,
                &self
                    .hooks
                    .closed_reason()
                    .unwrap_or_else(|| STOPPED.to_owned()),
            ),
            other => AppError::new(AppErrorCode::Unknown, other.to_string())
                .with_detail("method", method),
        }
    }

    pub fn connected(&self) -> bool {
        !self.hooks.is_closed()
    }

    async fn request(
        &self,
        request: ClientRequest,
        method: &str,
        timeout: Duration,
    ) -> Result<ServerResult, AppError> {
        if let Some(reason) = self.hooks.closed_reason() {
            return Err(AppError::new(AppErrorCode::McpUnavailable, reason));
        }
        let mut options = PeerRequestOptions::no_options();
        options.timeout = Some(timeout);
        let handle = self
            .peer
            .send_request_with_option(request, options)
            .await
            .map_err(|error| self.service_error(error, method))?;
        handle
            .await_response()
            .await
            .map_err(|error| self.service_error(error, method))
    }

    async fn pages<T: DeserializeOwned>(
        &self,
        method: &str,
        build: impl Fn(PaginatedRequestParams) -> ClientRequest,
    ) -> Result<Vec<T>, AppError> {
        let mut items = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let params = PaginatedRequestParams::default().with_cursor(cursor.clone());
            let result = self
                .request(build(params), method, self.request_timeout)
                .await?;
            let page: Page<T> = page_of(&result, method, &self.hooks)?;
            items.extend(page.items);
            match page.next_cursor.filter(|next| !next.is_empty()) {
                Some(next) => cursor = Some(next),
                None => return Ok(items),
            }
        }
        self.hooks.log(
            McpLogLevel::Warn,
            &format!("{method}: список оборван на {MAX_PAGES} страницах"),
        );
        Ok(items)
    }

    pub async fn discover(
        &self,
        capabilities: &Map<String, Value>,
    ) -> Result<McpDiscovery, AppError> {
        let has = |name: &str| capabilities.is_empty() || capabilities.contains_key(name);
        let tools: Vec<McpToolDescriptor> = if has("tools") {
            self.pages("tools/list", |params| {
                ClientRequest::ListToolsRequest(ListToolsRequest::with_param(params))
            })
            .await?
        } else {
            Vec::new()
        };
        let resources: Vec<McpResourceDescriptor> = if has("resources") {
            self.pages("resources/list", |params| {
                ClientRequest::ListResourcesRequest(ListResourcesRequest::with_param(params))
            })
            .await?
        } else {
            Vec::new()
        };
        let prompts: Vec<McpPromptDescriptor> = if has("prompts") {
            self.pages("prompts/list", |params| {
                ClientRequest::ListPromptsRequest(ListPromptsRequest::with_param(params))
            })
            .await?
        } else {
            Vec::new()
        };
        Ok(McpDiscovery {
            tools,
            resources,
            prompts,
        })
    }

    pub async fn call_tool(
        &self,
        name: &str,
        arguments: Map<String, Value>,
        timeout: Duration,
    ) -> Result<Value, AppError> {
        let request = ClientRequest::CallToolRequest(CallToolRequest::new(
            CallToolRequestParams::new(name.to_owned()).with_arguments(arguments),
        ));
        match self.request(request, "tools/call", timeout).await? {
            ServerResult::CallToolResult(result) => {
                Ok(serde_json::to_value(result).unwrap_or(Value::Null))
            }
            ServerResult::EmptyResult(_) => Ok(json!({ "content": [], "isError": false })),
            other => {
                report_value(&self.hooks, &other);
                Err(unexpected("tools/call"))
            }
        }
    }

    pub async fn close(&self) {
        let service = self.service.lock().await.take();
        if let Some(mut service) = service {
            let _ = service.close().await;
        }
        self.hooks.close(STOPPED);
    }
}
