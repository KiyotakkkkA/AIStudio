use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppErrorCode, CreateMcpServerInput, McpCapabilityKind, McpOAuthState, McpServerStatus,
    McpStartup, McpTransport, McpValue, RunStatus, SecretScope, ToolListFilter, ToolSource,
    ToolTier, UpdateMcpServerInput,
};

use super::*;
use crate::drivers::mcp::LaunchSpec;
use crate::platform::events::EventBus;
use crate::services::crypto::{CryptoService, Plaintext};
use crate::services::{RunService, RunServiceOptions, SecretCreateInput};
use crate::testing::{
    FakeCrypto, MemoryLogger, TemporaryDatabase, TemporaryDirectory, temporary_database,
    temporary_directory,
};
use crate::tools::ToolRegistryOptions;

fn fake_server() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("test")
        .join("helpers")
        .join("fakeMcpServer.mjs")
        .to_string_lossy()
        .into_owned()
}

struct Host {
    database: TemporaryDatabase,
    secrets: Arc<SecretService>,
    tools: ToolRegistry,
    nodes: Arc<NodeRegistry>,
    logger: Arc<MemoryLogger>,
    mcp: McpService,
}

fn options(host: &Host) -> McpServiceOptions {
    McpServiceOptions {
        logger: host.logger.clone(),
        request_timeout: Duration::from_secs(5),
        call_timeout: Duration::from_secs(5),
        backoff: Duration::from_millis(10),
        max_backoff: Duration::from_millis(20),
        ..McpServiceOptions::new(
            host.database.shared(),
            host.tools.clone(),
            Arc::clone(&host.nodes),
            Arc::clone(&host.secrets),
            reqwest::Client::new(),
        )
    }
}

fn host() -> Host {
    let database = temporary_database();
    let logger = MemoryLogger::shared();
    let secrets = Arc::new(SecretService::new(
        database.shared(),
        Arc::new(CryptoService::new(FakeCrypto::shared())),
        Arc::new(crate::platform::clock::SystemClock),
        logger.clone(),
    ));
    let tools = ToolRegistry::new(ToolRegistryOptions {
        logger: logger.clone(),
        ..ToolRegistryOptions::new(database.shared())
    });
    let nodes = Arc::new(NodeRegistry::new());
    let placeholder = McpService::new(McpServiceOptions::new(
        database.shared(),
        tools.clone(),
        Arc::clone(&nodes),
        Arc::clone(&secrets),
        reqwest::Client::new(),
    ));
    let mut host = Host {
        database,
        secrets,
        tools,
        nodes,
        logger,
        mcp: placeholder,
    };
    host.mcp = McpService::new(options(&host));
    host
}

fn stdio(name: &str, env: &[(&str, McpValue)], args: &[&str]) -> CreateMcpServerInput {
    let mut arguments = vec![fake_server()];
    arguments.extend(args.iter().map(|arg| (*arg).to_owned()));
    CreateMcpServerInput {
        command: Some(Some("node".to_owned())),
        args: Some(arguments),
        env: Some(
            env.iter()
                .map(|(name, value)| ((*name).to_owned(), value.clone()))
                .collect(),
        ),
        ..CreateMcpServerInput::new(name, McpTransport::Stdio)
    }
}

impl Host {
    fn create(&self, input: &CreateMcpServerInput) -> String {
        self.mcp.create(input).expect("creates").id.to_string()
    }

    fn stdio_server(&self, name: &str) -> String {
        self.create(&stdio(name, &[], &[]))
    }

    fn available_mcp(&self) -> Vec<String> {
        self.tools
            .list(&ToolListFilter {
                source: Some(ToolSource::Mcp),
                include_unavailable: Some(false),
                ..ToolListFilter::default()
            })
            .expect("lists")
            .into_iter()
            .map(|tool| tool.key)
            .collect()
    }

    fn status(&self, id: &str) -> McpServerStatus {
        self.mcp.server(id).expect("a server").status
    }

    async fn call(
        &self,
        id: &str,
        tool: &str,
        arguments: Value,
    ) -> Result<McpCallResultDto, AppError> {
        self.call_with(id, tool, arguments, None).await
    }

    async fn call_with(
        &self,
        id: &str,
        tool: &str,
        arguments: Value,
        timeout: Option<Duration>,
    ) -> Result<McpCallResultDto, AppError> {
        self.mcp
            .call(
                &McpCallTarget {
                    server_id: id.to_owned(),
                    tool: tool.to_owned(),
                    timeout,
                    read_resource: false,
                },
                arguments.as_object().cloned().unwrap_or_default(),
                &CancellationToken::new(),
            )
            .await
    }

    fn secret(&self, name: &str, value: &str) -> String {
        self.secrets
            .create(SecretCreateInput {
                value: Some(Plaintext::new(value)),
                ..SecretCreateInput::new("custom", name, SecretScope::Personal)
            })
            .expect("a secret")
            .id
    }
}

async fn eventually(what: &str, timeout: Duration, mut check: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(check(), "{what} did not happen in time");
}

fn secret_value(id: &str) -> McpValue {
    McpValue::secret(zvs_contract::SecretId::parse(id).expect("an id"))
}

#[test]
fn a_name_becomes_a_namespace_and_a_tool_name_becomes_a_key() {
    assert_eq!(derive_namespace("qdrant-local"), "qdrantLocal");
    assert_eq!(derive_namespace("GitHub MCP"), "githubMcp");
    assert_eq!(derive_namespace("2024 server"), "server");
    assert_eq!(derive_namespace("!!!"), "server");
    assert_eq!(
        tool_key("qdrant", "deleteCollection"),
        "qdrant.deleteCollection"
    );
    assert_eq!(
        tool_key("qdrant", "list collections!"),
        "qdrant.list-collections"
    );
    assert_eq!(tool_key("qdrant", "%%%"), "qdrant.call");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_server_with_a_colliding_name_gets_its_own_namespace() {
    let host = host();
    assert_eq!(
        host.mcp
            .create(&stdio("qdrant", &[], &[]))
            .expect("creates")
            .namespace,
        "qdrant"
    );
    assert_eq!(
        host.mcp
            .create(&stdio("Qdrant!", &[], &[]))
            .expect("creates")
            .namespace,
        "qdrant2"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stdio_server_hands_back_a_handshake_its_tools_resources_and_prompts() {
    let host = host();
    let id = host.stdio_server("qdrant");
    let result = host.mcp.test(&id).await.expect("tests");
    assert!(result.ok, "{result:?}");
    assert_eq!(result.protocol_version.as_deref(), Some("2025-06-18"));
    assert_eq!(result.server_name.as_deref(), Some("fake-qdrant"));
    assert_eq!(result.server_version.as_deref(), Some("0.8.1"));
    assert_eq!(
        (
            result.tool_count,
            result.resource_count,
            result.prompt_count
        ),
        (6, 1, 1)
    );
    assert_eq!(result.error, None);
    let detail = host.mcp.get(&id).expect("a detail");
    assert_eq!(detail.server.status, McpServerStatus::Connected);
    assert_eq!(
        detail.server.protocol_version.as_deref(),
        Some("2025-06-18")
    );
    let prompts: Vec<&str> = detail
        .capabilities
        .iter()
        .filter(|row| row.kind == McpCapabilityKind::Prompt)
        .map(|row| row.name.as_str())
        .collect();
    assert_eq!(prompts, ["summarise"]);
    assert_eq!(
        detail
            .capabilities
            .iter()
            .find(|row| row.kind == McpCapabilityKind::Resource)
            .and_then(|row| row.uri.as_deref()),
        Some("qdrant://collections")
    );
    host.mcp.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn discovery_registers_tools_with_their_schemas_and_a_safe_default_tier() {
    let host = host();
    let id = host.stdio_server("qdrant");
    host.mcp.start(&id).await.expect("starts");
    let registered: BTreeMap<String, zvs_contract::ToolDto> = host
        .tools
        .list(&ToolListFilter {
            source: Some(ToolSource::Mcp),
            ..ToolListFilter::default()
        })
        .expect("lists")
        .into_iter()
        .map(|tool| (tool.key.clone(), tool))
        .collect();
    assert_eq!(
        registered.keys().cloned().collect::<Vec<_>>(),
        [
            "qdrant.crash",
            "qdrant.deleteCollection",
            "qdrant.hang",
            "qdrant.read_resource",
            "qdrant.search",
            "qdrant.upsert",
            "qdrant.whoami"
        ]
    );
    let search = &registered["qdrant.search"];
    assert_eq!((search.tier, search.read_only), (ToolTier::Auto, true));
    let upsert = &registered["qdrant.upsert"];
    assert_eq!((upsert.tier, upsert.destructive), (ToolTier::Ask, false));
    let delete = &registered["qdrant.deleteCollection"];
    assert_eq!((delete.tier, delete.destructive), (ToolTier::Off, true));
    assert_eq!(search.input_schema["type"], json!("object"));
    assert_eq!(search.source_ref.as_deref(), Some(id.as_str()));
    assert!(
        host.nodes
            .list()
            .iter()
            .any(|node| node.node_type() == "qdrant.search")
    );
    host.mcp.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_discovered_tool_is_callable_and_its_result_is_normalised() {
    let host = host();
    let id = host.stdio_server("qdrant");
    host.mcp.start(&id).await.expect("starts");
    let result = host
        .call(&id, "search", json!({ "query": "вопрос" }))
        .await
        .expect("calls");
    assert!(!result.is_error);
    assert_eq!(result.text, r#"search:{"query":"вопрос"}"#);
    let by_key = host
        .mcp
        .call_by_tool_key(
            "qdrant.search",
            json!({ "query": "ещё" })
                .as_object()
                .cloned()
                .unwrap_or_default(),
            &CancellationToken::new(),
        )
        .await
        .expect("calls");
    assert_eq!(by_key.text, r#"search:{"query":"ещё"}"#);
    host.mcp.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn secret_env_values_are_resolved_at_launch_and_never_persisted_or_logged() {
    let host = host();
    let secret = host.secret("Qdrant — local cluster", "sk-qdrant-0123456789abcdef");
    let id = host.create(&stdio(
        "qdrant",
        &[
            ("QDRANT_API_KEY", secret_value(&secret)),
            (
                "QDRANT_COLLECTION",
                McpValue::literal("product-knowledge-base"),
            ),
        ],
        &[],
    ));
    host.mcp.start(&id).await.expect("starts");
    let result = host.call(&id, "whoami", json!({})).await.expect("calls");
    assert_eq!(
        serde_json::from_str::<Value>(&result.text).expect("json"),
        json!({
            "QDRANT_API_KEY": "sk-qdrant-0123456789abcdef",
            "QDRANT_COLLECTION": "product-knowledge-base",
        })
    );
    let row = host.mcp.find(&id).expect("reads").expect("a row");
    assert_eq!(row.env["QDRANT_API_KEY"], secret_value(&secret));
    assert_eq!(
        row.env["QDRANT_COLLECTION"],
        McpValue::literal("product-knowledge-base")
    );
    let get = serde_json::to_string(&host.mcp.get(&id).expect("a detail")).expect("json");
    assert!(!get.contains("sk-qdrant"));
    assert!(!format!("{:?}", host.logger.entries()).contains("sk-qdrant"));
    let health = serde_json::to_string(&host.mcp.health(&id).expect("health")).expect("json");
    assert!(!health.contains("sk-qdrant"));
    let usage = host.secrets.usage(&secret).expect("usage");
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].id, id);
    host.mcp.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_that_never_starts_fails_with_an_error_naming_the_command() {
    let host = host();
    let id = host.create(&CreateMcpServerInput {
        command: Some(Some("definitely-not-a-real-binary-zvs".to_owned())),
        args: Some(vec!["--serve".to_owned()]),
        ..CreateMcpServerInput::new("missing", McpTransport::Stdio)
    });
    let error = host.mcp.start(&id).await.expect_err("fails");
    assert_eq!(error.code, AppErrorCode::McpUnavailable);
    let row = host.mcp.server(&id).expect("a server");
    assert_eq!(row.status, McpServerStatus::Failed);
    assert!(
        row.last_error
            .as_deref()
            .is_some_and(|error| error.contains("definitely-not-a-real-binary-zvs"))
    );
    assert!(host.available_mcp().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hung_call_times_out_without_taking_the_connection_with_it() {
    let host = host();
    let id = host.stdio_server("qdrant");
    host.mcp.start(&id).await.expect("starts");
    let error = host
        .call_with(&id, "hang", json!({}), Some(Duration::from_millis(120)))
        .await
        .expect_err("times out");
    assert_eq!(error.code, AppErrorCode::McpTimeout);
    assert!(
        !host
            .call(&id, "search", json!({}))
            .await
            .expect("calls")
            .is_error
    );
    let health = host.mcp.health(&id).expect("health");
    assert_eq!(health.errors, 1);
    assert!(
        health
            .recent_log
            .iter()
            .any(|line| line.line.contains("hang"))
    );
    host.mcp.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_malformed_response_is_normalised_to_unknown_and_the_payload_stays_host_side() {
    let host = host();
    let id = host.create(&stdio(
        "broken",
        &[("FAKE_MCP_MALFORMED", McpValue::literal("1"))],
        &[],
    ));
    let error = host.mcp.start(&id).await.expect_err("fails");
    assert_eq!(error.code, AppErrorCode::Unknown, "{error:?}");
    assert_eq!(host.status(&id), McpServerStatus::Failed);
    let reported = host.logger.matching("malformed");
    assert!(
        reported.iter().any(|entry| entry.fields["raw"]
            .as_str()
            .is_some_and(|raw| raw.contains("тело не по схеме"))),
        "{reported:?}"
    );
    assert!(
        !host
            .mcp
            .health(&id)
            .expect("health")
            .recent_log
            .iter()
            .any(|line| line.line.contains("тело не по схеме"))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_that_speaks_an_unsupported_protocol_version_is_refused() {
    let host = host();
    let id = host.create(&stdio(
        "ancient",
        &[("FAKE_MCP_PROTOCOL", McpValue::literal("1999-01-01"))],
        &[],
    ));
    let error = host.mcp.start(&id).await.expect_err("fails");
    assert_eq!(error.code, AppErrorCode::McpUnavailable);
    assert!(
        host.mcp
            .server(&id)
            .expect("a server")
            .last_error
            .is_some_and(|error| error.contains("1999-01-01"))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_that_dies_marks_its_tools_unavailable_fails_in_flight_and_restarts() {
    let host = host();
    let id = host.stdio_server("qdrant");
    host.mcp.start(&id).await.expect("starts");
    assert_eq!(host.available_mcp().len(), 7);
    let error = host.call(&id, "crash", json!({})).await.expect_err("fails");
    assert_eq!(error.code, AppErrorCode::McpUnavailable, "{error:?}");
    eventually("the restart", Duration::from_secs(10), || {
        host.status(&id) == McpServerStatus::Connected && host.available_mcp().len() == 7
    })
    .await;
    assert!(host.mcp.health(&id).expect("health").restarts >= 1);
    assert!(
        !host
            .call(&id, "search", json!({}))
            .await
            .expect("calls")
            .is_error
    );
    host.mcp.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stopping_is_not_a_death_and_never_schedules_a_restart() {
    let host = host();
    let id = host.stdio_server("qdrant");
    host.mcp.start(&id).await.expect("starts");
    host.mcp.stop(&id).await;
    assert_eq!(host.status(&id), McpServerStatus::Stopped);
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(host.status(&id), McpServerStatus::Stopped);
    assert_eq!(host.mcp.health(&id).expect("health").restarts, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_health_rail_reports_uptime_calls_errors_and_a_median_latency() {
    let host = host();
    let id = host.stdio_server("qdrant");
    host.mcp.start(&id).await.expect("starts");
    host.call(&id, "search", json!({})).await.expect("calls");
    host.call(&id, "search", json!({})).await.expect("calls");
    let health = host.mcp.health(&id).expect("health");
    assert_eq!(health.status, McpServerStatus::Connected);
    assert!(health.uptime_ms.is_some());
    assert!(health.calls >= 3);
    assert_eq!(health.errors, 0);
    assert!(health.median_latency_ms.is_some());
    assert!(
        health
            .recent_log
            .last()
            .is_some_and(|line| line.line.contains("initialize protocol 2025-06-18"))
    );
    host.mcp.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn removing_a_server_withdraws_its_tools_and_its_nodes() {
    let host = host();
    let id = host.stdio_server("qdrant");
    host.mcp.start(&id).await.expect("starts");
    host.mcp.remove(&id).await.expect("removes");
    assert!(host.available_mcp().is_empty());
    assert!(
        !host
            .nodes
            .list()
            .iter()
            .any(|node| node.node_type() == "qdrant.search")
    );
    assert_eq!(
        host.mcp.get(&id).expect_err("gone").code,
        AppErrorCode::NotFound
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disabling_a_server_takes_its_tools_out_of_reach() {
    let host = host();
    let id = host.stdio_server("qdrant");
    host.mcp.start(&id).await.expect("starts");
    let update = UpdateMcpServerInput {
        enabled: Some(false),
        ..UpdateMcpServerInput::new(zvs_contract::McpServerId::parse(&id).expect("an id"))
    };
    host.mcp.update(&update).await.expect("updates");
    assert_eq!(host.status(&id), McpServerStatus::Disabled);
    assert!(host.available_mcp().is_empty());
    assert_eq!(
        host.mcp.start(&id).await.expect_err("refused").code,
        AppErrorCode::Conflict
    );
}

#[derive(Clone, Default)]
struct HttpFixture {
    calls: Arc<Mutex<Vec<(Value, HeaderMap)>>>,
    reject: bool,
    bearer: Option<&'static str>,
}

async fn http_handler(
    State(fixture): State<HttpFixture>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if fixture.reject {
        return (StatusCode::UNAUTHORIZED, "no").into_response();
    }
    if let Some(token) = fixture.bearer {
        let presented = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if presented != format!("Bearer {token}") {
            return (
                StatusCode::UNAUTHORIZED,
                [(
                    "www-authenticate",
                    "Bearer realm=\"mcp\", resource_metadata=\"http://127.0.0.1:1/.well-known/oauth-protected-resource\"",
                )],
                "{}",
            )
                .into_response();
        }
    }
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    fixture
        .calls
        .lock()
        .expect("calls")
        .push((body.clone(), headers));
    let Some(id) = body.get("id").cloned() else {
        return StatusCode::ACCEPTED.into_response();
    };
    let result = match body["method"].as_str() {
        Some("initialize") => json!({
            "protocolVersion": "2025-06-18",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "github", "version": "2.0.0" },
        }),
        Some("tools/list") => json!({
            "tools": [{ "name": "listIssues", "description": "Задачи", "inputSchema": { "type": "object" } }]
        }),
        _ => json!({}),
    };
    (
        StatusCode::OK,
        [
            ("content-type", "application/json"),
            ("mcp-session-id", "session-1"),
        ],
        json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string(),
    )
        .into_response()
}

async fn serve(fixture: HttpFixture) -> String {
    let app = Router::new()
        .route(
            "/mcp",
            post(http_handler)
                .get(|| async { StatusCode::METHOD_NOT_ALLOWED })
                .delete(|| async { StatusCode::OK }),
        )
        .with_state(fixture);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binds");
    let address = listener.local_addr().expect("an address");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{address}/mcp")
}

fn http_server(url: &str, headers: &[(&str, McpValue)]) -> CreateMcpServerInput {
    CreateMcpServerInput {
        url: Some(Some(url.to_owned())),
        headers: Some(
            headers
                .iter()
                .map(|(name, value)| ((*name).to_owned(), value.clone()))
                .collect(),
        ),
        ..CreateMcpServerInput::new("github", McpTransport::Http)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_http_server_handshakes_and_discovers_over_the_wire() {
    let host = host();
    let fixture = HttpFixture::default();
    let url = serve(fixture.clone()).await;
    let secret = host.secret("GitHub token", "ghp_0123456789abcdefghijklmnopqrstuvwx");
    let id = host.create(&http_server(
        &url,
        &[("Authorization", secret_value(&secret))],
    ));
    host.mcp.start(&id).await.expect("starts");
    let server = host.mcp.server(&id).expect("a server");
    assert_eq!(server.status, McpServerStatus::Connected);
    assert_eq!(server.server_name.as_deref(), Some("github"));
    assert_eq!(host.available_mcp(), ["github.listIssues"]);
    let calls = fixture.calls.lock().expect("calls").clone();
    assert_eq!(
        calls[0]
            .1
            .get("authorization")
            .and_then(|value| value.to_str().ok()),
        Some("ghp_0123456789abcdefghijklmnopqrstuvwx")
    );
    assert_eq!(
        calls
            .last()
            .and_then(|(_, headers)| headers.get("mcp-session-id"))
            .and_then(|value| value.to_str().ok()),
        Some("session-1")
    );
    assert!(!format!("{:?}", host.logger.entries()).contains("ghp_0123456789"));
    host.mcp.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_http_server_that_rejects_the_token_fails_with_a_clear_error() {
    let host = host();
    let url = serve(HttpFixture {
        reject: true,
        ..HttpFixture::default()
    })
    .await;
    let id = host.create(&http_server(&url, &[]));
    let error = host.mcp.start(&id).await.expect_err("fails");
    assert_eq!(error.code, AppErrorCode::McpUnavailable);
    assert!(
        host.mcp
            .server(&id)
            .expect("a server")
            .last_error
            .is_some_and(|error| error.contains("401")),
        "{error:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_oauth_server_asks_for_sign_in_and_then_accepts_the_stored_token() {
    let host = host();
    let fixture = HttpFixture {
        bearer: Some("access-1"),
        ..HttpFixture::default()
    };
    let url = serve(fixture.clone()).await;
    let id = host.create(&http_server(&url, &[]));
    let error = host.mcp.start(&id).await.expect_err("needs a sign-in");
    assert!(is_auth_required(&error));
    let server = host.mcp.server(&id).expect("a server");
    assert_eq!(server.oauth, McpOAuthState::Required);
    assert_eq!(server.last_error.as_deref(), Some(AUTH_REQUIRED));
    host.mcp
        .save_grant(
            &id,
            &OAuthGrant {
                issuer: "https://id.example".to_owned(),
                token_endpoint: "https://id.example/token".to_owned(),
                client_id: "client-1".to_owned(),
                client_secret: None,
                redirect_uri: "http://127.0.0.1:53682/callback".to_owned(),
                resource: url.clone(),
                scope: None,
                access_token: "access-1".to_owned(),
                refresh_token: None,
                expires_at: None,
            },
        )
        .expect("saves");
    host.mcp.start(&id).await.expect("starts");
    let server = host.mcp.server(&id).expect("a server");
    assert_eq!(server.status, McpServerStatus::Connected);
    assert_eq!(server.oauth, McpOAuthState::SignedIn);
    assert_eq!(host.available_mcp(), ["github.listIssues"]);
    assert!(!format!("{:?}", host.logger.entries()).contains("access-1"));
    let signed_out = host.mcp.sign_out(&id).await.expect("signs out");
    assert_eq!(signed_out.oauth, McpOAuthState::None);
    assert!(host.available_mcp().is_empty());
    host.mcp.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn signing_in_to_a_stdio_server_is_refused() {
    let host = host();
    let id = host.stdio_server("qdrant");
    let error = host.mcp.sign_in(&id).await.expect_err("refused");
    assert_eq!(error.code, AppErrorCode::ValidationFailed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_calls_an_mcp_tool_only_within_the_registered_ceiling() {
    let host = host();
    let id = host.stdio_server("qdrant");
    host.mcp.start(&id).await.expect("starts");
    let runs = RunService::new(RunServiceOptions {
        approval_timeout: Duration::from_millis(200),
        tools: Some(host.tools.clone()),
        ..RunServiceOptions::new(
            host.database.shared(),
            EventBus::default(),
            Arc::clone(&host.nodes),
        )
    })
    .expect("a run service");
    let start = |node_type: &str, input: Value| {
        runs.start(
            zvs_contract::parse(json!({
                "kind": "scenario",
                "subjectId": "s1",
                "graph": { "nodes": [{ "id": "a", "type": node_type, "input": input }] },
            }))
            .expect("a start input"),
        )
        .expect("starts")
    };
    let allowed = start("qdrant.search", json!({ "query": "вопрос" }));
    runs.wait(&allowed.id).await;
    assert_eq!(
        runs.get(&allowed.id).expect("a run").status,
        RunStatus::Succeeded
    );
    let denied = start("qdrant.deleteCollection", json!({}));
    runs.wait(&denied.id).await;
    assert_eq!(
        runs.get(&denied.id)
            .expect("a run")
            .outcome
            .and_then(|outcome| outcome.code),
        Some(AppErrorCode::PermissionDenied)
    );
    let search = host
        .tools
        .find_by_key("qdrant.search")
        .expect("reads")
        .expect("a tool");
    assert_eq!(
        host.tools
            .usage(&search.id.to_string())
            .expect("usage")
            .call_count,
        1
    );
    runs.dispose().await;
    host.mcp.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_call_made_while_the_server_is_down_brings_it_back_rather_than_failing() {
    let host = host();
    let id = host.stdio_server("qdrant");
    host.mcp.start(&id).await.expect("starts");
    host.mcp.stop(&id).await;
    let result = host
        .call(&id, "search", json!({ "query": "x" }))
        .await
        .expect("calls");
    assert!(!result.is_error);
    assert_eq!(host.status(&id), McpServerStatus::Connected);
    host.mcp.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tier_set_through_the_registry_survives_a_reconnect_for_an_mcp_tool() {
    let host = host();
    let id = host.stdio_server("qdrant");
    host.mcp.test(&id).await.expect("tests");
    let key = host
        .mcp
        .get(&id)
        .expect("a detail")
        .capabilities
        .into_iter()
        .find(|row| row.name == "search")
        .and_then(|row| row.tool_key)
        .expect("a key");
    let tool = host
        .tools
        .find_by_key(&key)
        .expect("reads")
        .expect("a tool");
    assert_eq!(tool.source_ref.as_deref(), Some(id.as_str()));
    let tool_id = tool.id.to_string();
    host.tools
        .set_tier(&tool_id, ToolTier::Off, "user")
        .expect("sets");
    host.tools.set_enabled(&tool_id, false).expect("disables");
    host.mcp.test(&id).await.expect("tests");
    let after = host.tools.get(&tool_id).expect("a tool").tool;
    assert_eq!(
        (after.tier, after.enabled, after.available),
        (ToolTier::Off, false, true)
    );
    host.mcp.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn health_says_a_crashed_server_is_waiting_to_restart() {
    let mut host = host();
    host.mcp = McpService::new(McpServiceOptions {
        backoff: Duration::from_secs(5),
        max_backoff: Duration::from_secs(5),
        ..options(&host)
    });
    let id = host.stdio_server("qdrant");
    host.mcp.start(&id).await.expect("starts");
    assert!(host.call(&id, "crash", json!({})).await.is_err());
    eventually("the scheduled restart", Duration::from_secs(5), || {
        host.mcp
            .health(&id)
            .is_ok_and(|health| health.status == McpServerStatus::Failed && health.restarting)
    })
    .await;
    host.mcp.stop(&id).await;
    assert!(!host.mcp.health(&id).expect("health").restarting);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_command_that_cannot_start_fails_once_and_is_not_restarted() {
    let host = host();
    let id = host.create(&CreateMcpServerInput {
        command: Some(Some("zvs-definitely-not-a-command".to_owned())),
        ..CreateMcpServerInput::new("ghost", McpTransport::Stdio)
    });
    let result = host.mcp.test(&id).await.expect("tests");
    assert!(!result.ok);
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("ENOENT")),
        "{result:?}"
    );
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(host.status(&id), McpServerStatus::Failed);
    let health = host.mcp.health(&id).expect("health");
    assert_eq!((health.restarts, health.restarting), (0, false));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stdio_servers_get_the_isolated_toolchain_env_and_a_servers_own_env_wins() {
    let mut host = host();
    let seen: Arc<Mutex<Vec<BTreeMap<String, String>>>> = Arc::default();
    let recorder = Arc::clone(&seen);
    let toolchain = PathBuf::from("C:").join("data").join("mcp-toolchain");
    host.mcp = McpService::new(McpServiceOptions {
        toolchain_dir: Some(toolchain.clone()),
        spawn: Some(Arc::new(move |spec: &LaunchSpec| {
            recorder.lock().expect("seen").push(spec.env.clone());
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "spawn ENOENT",
            ))
        })),
        ..options(&host)
    });
    let id = host.create(&CreateMcpServerInput {
        command: Some(Some("npx".to_owned())),
        env: Some(BTreeMap::from([(
            "UV_CACHE_DIR".to_owned(),
            McpValue::literal("D:/mine"),
        )])),
        ..CreateMcpServerInput::new("npx", McpTransport::Stdio)
    });
    host.mcp.test(&id).await.expect("tests");
    let seen = seen.lock().expect("seen").clone();
    assert_eq!(
        seen[0]["npm_config_cache"],
        toolchain.join("npm-cache").to_string_lossy()
    );
    assert_eq!(seen[0]["UV_CACHE_DIR"], "D:/mine");
}

struct FakeLauncher;

impl CommandLauncher for FakeLauncher {
    fn launch_command(
        &self,
        command: &str,
        args: &[String],
    ) -> Option<Result<LaunchSpec, AppError>> {
        match command {
            "npx" => Some(Ok(LaunchSpec {
                command: "C:/toolchain/node/22.11.0/npx.cmd".to_owned(),
                args: args.to_vec(),
                env: BTreeMap::from([
                    ("PATH".to_owned(), "C:/toolchain/node/22.11.0".to_owned()),
                    (
                        "UV_CACHE_DIR".to_owned(),
                        "C:/toolchain/uv/cache".to_owned(),
                    ),
                ]),
                cwd: None,
            })),
            "uvx" => Some(Err(AppError::new(
                AppErrorCode::NotFound,
                crate::toolchain::service::SETTINGS_HINT,
            )
            .with_detail("runtime", "uv"))),
            _ => None,
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runtime_commands_launch_through_the_toolchain_and_a_missing_one_never_spawns() {
    let mut host = host();
    let seen: Arc<Mutex<Vec<LaunchSpec>>> = Arc::default();
    let recorder = Arc::clone(&seen);
    host.mcp = McpService::new(McpServiceOptions {
        toolchain_dir: Some(PathBuf::from("C:").join("data").join("mcp-toolchain")),
        toolchain: Some(Arc::new(FakeLauncher)),
        spawn: Some(Arc::new(move |spec: &LaunchSpec| {
            recorder.lock().expect("seen").push(spec.clone());
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "spawn ENOENT",
            ))
        })),
        ..options(&host)
    });
    let npx = host.create(&CreateMcpServerInput {
        command: Some(Some("npx".to_owned())),
        args: Some(vec!["-y".to_owned(), "server-memory".to_owned()]),
        env: Some(BTreeMap::from([(
            "UV_CACHE_DIR".to_owned(),
            McpValue::literal("D:/mine"),
        )])),
        ..CreateMcpServerInput::new("npx", McpTransport::Stdio)
    });
    host.mcp.test(&npx).await.expect("tests");
    let spawned = seen.lock().expect("seen").clone();
    assert_eq!(spawned.len(), 1);
    assert_eq!(spawned[0].command, "C:/toolchain/node/22.11.0/npx.cmd");
    assert_eq!(spawned[0].args, ["-y", "server-memory"]);
    assert_eq!(spawned[0].env["PATH"], "C:/toolchain/node/22.11.0");
    assert_eq!(spawned[0].env["UV_CACHE_DIR"], "D:/mine");

    let uvx = host.create(&CreateMcpServerInput {
        command: Some(Some("uvx".to_owned())),
        ..CreateMcpServerInput::new("uvx", McpTransport::Stdio)
    });
    let result = host.mcp.test(&uvx).await.expect("tests");
    assert!(!result.ok);
    assert_eq!(
        result.error.as_deref(),
        Some(crate::toolchain::service::SETTINGS_HINT)
    );
    assert_eq!(seen.lock().expect("seen").len(), 1);

    let other = host.create(&CreateMcpServerInput {
        command: Some(Some("python".to_owned())),
        ..CreateMcpServerInput::new("python", McpTransport::Stdio)
    });
    host.mcp.test(&other).await.expect("tests");
    let spawned = seen.lock().expect("seen").clone();
    assert_eq!(spawned[1].command, "python");
    assert!(spawned[1].env["npm_config_cache"].contains("mcp-toolchain"));
    assert!(!spawned[1].env.contains_key("PATH"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn servers_configured_for_launch_come_up_and_one_bad_server_never_blocks_the_rest() {
    let host = host();
    let good = host.create(&CreateMcpServerInput {
        startup: Some(McpStartup::Launch),
        ..stdio("qdrant", &[], &[])
    });
    let bad = host.create(&CreateMcpServerInput {
        command: Some(Some("zvs-definitely-not-a-command".to_owned())),
        startup: Some(McpStartup::Launch),
        ..CreateMcpServerInput::new("ghost", McpTransport::Stdio)
    });
    let manual = host.stdio_server("manual");
    host.mcp.start_auto_start().await.expect("starts");
    assert_eq!(host.status(&good), McpServerStatus::Connected);
    assert_eq!(host.status(&bad), McpServerStatus::Failed);
    assert_eq!(host.status(&manual), McpServerStatus::Stopped);
    assert!(
        !host
            .logger
            .matching("configured for launch did not start")
            .is_empty()
    );
    host.mcp.dispose().await;
}

fn pid_file(directory: &TemporaryDirectory) -> PathBuf {
    directory.join("server.pid")
}

async fn read_pid(path: &std::path::Path) -> u32 {
    for _ in 0..500 {
        if let Ok(text) = std::fs::read_to_string(path)
            && let Ok(pid) = text.trim().parse()
        {
            return pid;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the fake server never wrote its pid");
}

fn alive(pid: u32) -> bool {
    let mut system = sysinfo::System::new();
    system.refresh_processes(
        sysinfo::ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]),
        true,
    );
    system.process(sysinfo::Pid::from_u32(pid)).is_some()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disposing_the_service_leaves_no_server_process_behind() {
    let host = host();
    let directory = temporary_directory("mcp-pid-");
    let id = host.create(&stdio(
        "qdrant",
        &[(
            "FAKE_MCP_PID_FILE",
            McpValue::literal(pid_file(&directory).to_string_lossy()),
        )],
        &[],
    ));
    host.mcp.start(&id).await.expect("starts");
    let pid = read_pid(&pid_file(&directory)).await;
    assert!(alive(pid));
    host.mcp.dispose().await;
    eventually("the server to exit", Duration::from_secs(5), || !alive(pid)).await;
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cmd_shim_with_spaces_in_its_path_launches_and_its_whole_tree_is_stopped() {
    let host = host();
    let directory = temporary_directory("mcp shim ");
    let shim = directory.join("fake server.cmd");
    std::fs::write(&shim, format!("@node \"{}\" %*\r\n", fake_server())).expect("writes");
    let id = host.create(&CreateMcpServerInput {
        command: Some(Some(shim.to_string_lossy().into_owned())),
        args: Some(vec!["--label".to_owned(), "a b&c".to_owned()]),
        env: Some(BTreeMap::from([(
            "FAKE_MCP_PID_FILE".to_owned(),
            McpValue::literal(pid_file(&directory).to_string_lossy()),
        )])),
        ..CreateMcpServerInput::new("shim", McpTransport::Stdio)
    });
    let result = host.mcp.test(&id).await.expect("tests");
    assert!(result.ok, "{result:?}");
    assert_eq!(result.tool_count, 6);
    let pid = read_pid(&pid_file(&directory)).await;
    assert!(alive(pid));
    host.mcp.stop(&id).await;
    eventually(
        "node behind the shim to exit",
        Duration::from_secs(5),
        || !alive(pid),
    )
    .await;
}

fn reader_target(id: &str) -> McpCallTarget {
    McpCallTarget {
        server_id: id.to_owned(),
        tool: RESOURCE_READER.to_owned(),
        timeout: None,
        read_resource: true,
    }
}

fn uri_arguments(uri: &str) -> Map<String, Value> {
    json!({ "uri": uri })
        .as_object()
        .cloned()
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn server_instructions_are_kept_and_resources_are_read_through_the_reader_tool() {
    let host = host();
    let id = host.stdio_server("qdrant");
    host.mcp.start(&id).await.expect("starts");
    assert_eq!(
        host.mcp
            .server(&id)
            .expect("a server")
            .instructions
            .as_deref(),
        Some("Read qdrant://collections before searching.")
    );
    let reader = host
        .tools
        .list(&ToolListFilter::default())
        .expect("lists")
        .into_iter()
        .find(|tool| tool.key == "qdrant.read_resource")
        .expect("the reader tool");
    assert!(reader.read_only);
    assert_eq!(
        reader.input_schema["properties"]["uri"]["enum"],
        json!(["qdrant://collections"])
    );
    let read = host
        .mcp
        .call(
            &reader_target(&id),
            uri_arguments("qdrant://collections"),
            &CancellationToken::new(),
        )
        .await
        .expect("reads");
    assert_eq!(read.text, "kb, docs");
    let unknown = host
        .mcp
        .call(
            &reader_target(&id),
            uri_arguments("file:///etc/passwd"),
            &CancellationToken::new(),
        )
        .await
        .expect_err("refused");
    assert_eq!(unknown.message, RESOURCE_NOT_FOUND);
    host.mcp.dispose().await;
}
