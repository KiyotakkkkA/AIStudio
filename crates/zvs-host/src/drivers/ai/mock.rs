use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use futures::StreamExt;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use zvs_contract::AppError;

use super::capabilities::AdapterCapabilities;
use super::http::{HttpClient, HttpClientOptions, ProxyMode};
use super::ports::{
    ChatMessage, DeltaStream, DiscoveredModel, FinishReason, GenerateRequest, GenerateResult,
    TextDelta, TextGenerationDriver, ToolCall, ToolSpec,
};
use super::transport::SharedTransport;
use super::transport::{ApiTransport, ApiTransportOptions, SecretResolver};
use crate::services::crypto::Plaintext;

pub struct CountingKey {
    value: &'static str,
    calls: AtomicUsize,
}

impl CountingKey {
    pub fn shared(value: &'static str) -> Arc<Self> {
        Arc::new(Self {
            value,
            calls: AtomicUsize::new(0),
        })
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl SecretResolver for CountingKey {
    fn resolve(&self, _id: &str) -> Result<Plaintext, AppError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Plaintext::new(self.value))
    }
}

pub fn direct_http() -> HttpClient {
    HttpClient::with_options(HttpClientOptions {
        proxy: ProxyMode::Off,
        ..HttpClientOptions::default()
    })
    .expect("a client")
}

pub fn transport_for(base_url: &str) -> SharedTransport {
    Arc::new(ApiTransport::new(ApiTransportOptions::new(
        base_url,
        direct_http(),
    )))
}

pub fn resolving_http(server: &MockServer, host: &str) -> HttpClient {
    HttpClient::with_options(HttpClientOptions {
        proxy: ProxyMode::Off,
        resolve: vec![(host.to_owned(), server.address())],
        ..HttpClientOptions::default()
    })
    .expect("a client")
}

pub fn openrouter_transport(server: &MockServer) -> SharedTransport {
    Arc::new(ApiTransport::new(ApiTransportOptions::new(
        format!("http://openrouter.ai:{}/api/v1", server.address().port()),
        resolving_http(server, "openrouter.ai"),
    )))
}

pub fn sse_frames(deltas: &[&str]) -> Vec<String> {
    deltas
        .iter()
        .map(|delta| {
            format!(
                "data: {}\n\n",
                json!({ "choices": [{ "index": 0, "delta": { "content": delta } }] })
            )
        })
        .chain([
            format!(
                "data: {}\n\n",
                json!({ "choices": [{ "index": 0, "delta": {}, "finish_reason": "stop" }] })
            ),
            "data: [DONE]\n\n".to_owned(),
        ])
        .collect()
}

pub fn fixture_text(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/fixtures/providers")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

pub fn fixture_frames(name: &str) -> Vec<String> {
    fixture_text(name)
        .split("\n\n")
        .filter(|frame| !frame.trim().is_empty())
        .map(|frame| format!("{frame}\n\n"))
        .collect()
}

pub fn search_tool() -> ToolSpec {
    ToolSpec {
        name: "docs__search".to_owned(),
        description: "Search the docs".to_owned(),
        parameters: json!({
            "type": "object",
            "properties": { "query": { "type": "string" } },
            "required": ["query"],
        }),
    }
}

pub fn tool_request() -> GenerateRequest {
    GenerateRequest {
        system: Some("Be brief.".to_owned()),
        tools: vec![search_tool()],
        ..GenerateRequest::new(
            "openai/gpt-4o-mini",
            vec![
                ChatMessage::user("How do I rotate a token?"),
                ChatMessage::assistant("").with_tool_calls(vec![ToolCall::new(
                    "call_1",
                    "docs__search",
                    r#"{"query":"rotate"}"#,
                )]),
                ChatMessage::tool("call_1", "Use the CLI."),
            ],
        )
    }
}

#[derive(Debug, Clone)]
pub enum ReplyBody {
    Bytes(Vec<u8>),
    Chunks {
        chunks: Vec<Vec<u8>>,
        gap: Duration,
        hold: bool,
    },
}

#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: ReplyBody,
    pub delay: Duration,
}

impl Reply {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body: ReplyBody::Bytes(text.into().into_bytes()),
            delay: Duration::ZERO,
        }
    }

    pub fn json(value: Value) -> Self {
        Self::text(value.to_string()).with_header("content-type", "application/json")
    }

    pub fn chunks<S: Into<String>>(chunks: impl IntoIterator<Item = S>) -> Self {
        Self {
            body: ReplyBody::Chunks {
                chunks: chunks
                    .into_iter()
                    .map(|chunk| chunk.into().into_bytes())
                    .collect(),
                gap: Duration::ZERO,
                hold: false,
            },
            ..Self::text("")
        }
        .with_header("content-type", "text/event-stream")
    }

    #[must_use]
    pub const fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    #[must_use]
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.retain(|(existing, _)| existing != name);
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    #[must_use]
    pub const fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    #[must_use]
    pub fn with_gap(mut self, delay: Duration) -> Self {
        if let ReplyBody::Chunks { gap, .. } = &mut self.body {
            *gap = delay;
        }
        self
    }

    #[must_use]
    pub fn holding(mut self) -> Self {
        if let ReplyBody::Chunks { hold, .. } = &mut self.body {
            *hold = true;
        }
        self
    }
}

#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub host: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<String> {
        self.headers
            .iter()
            .find(|(existing, _)| existing.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.clone())
    }

    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
}

#[derive(Default)]
struct MockState {
    routes: Mutex<HashMap<String, Reply>>,
    fallback: Mutex<Option<Reply>>,
    calls: Mutex<Vec<Recorded>>,
    streams_dropped: AtomicUsize,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

struct DropCounter(Arc<MockState>);

impl Drop for DropCounter {
    fn drop(&mut self) {
        self.0.streams_dropped.fetch_add(1, Ordering::SeqCst);
    }
}

pub struct MockServer {
    address: SocketAddr,
    state: Arc<MockState>,
    task: JoinHandle<()>,
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl MockServer {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a local port");
        let address = listener.local_addr().expect("an address");
        let state = Arc::new(MockState::default());
        let router = Router::new()
            .fallback(handle)
            .with_state(Arc::clone(&state));
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Self {
            address,
            state,
            task,
        }
    }

    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn origin(&self) -> String {
        format!("http://{}", self.address)
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.origin())
    }

    pub fn reply(&self, path: &str, reply: Reply) -> &Self {
        lock(&self.state.routes).insert(path.to_owned(), reply);
        self
    }

    pub fn fallback(&self, reply: Reply) {
        *lock(&self.state.fallback) = Some(reply);
    }

    pub fn calls(&self) -> Vec<Recorded> {
        lock(&self.state.calls).clone()
    }

    pub fn paths(&self) -> Vec<String> {
        self.calls().into_iter().map(|call| call.path).collect()
    }

    pub fn last(&self) -> Recorded {
        self.calls().pop().expect("a recorded call")
    }

    pub fn last_body(&self) -> Value {
        self.last().json()
    }

    pub fn streams_dropped(&self) -> usize {
        self.state.streams_dropped.load(Ordering::SeqCst)
    }

    pub async fn wait_for_dropped_stream(&self) -> bool {
        for _ in 0..200 {
            if self.streams_dropped() > 0 {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }
}

async fn handle(State(state): State<Arc<MockState>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let body = axum::body::to_bytes(body, usize::MAX)
        .await
        .map(|bytes| bytes.to_vec())
        .unwrap_or_default();
    let host = parts.uri.host().map(str::to_owned).or_else(|| {
        parts
            .headers
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.split(':').next().unwrap_or(value).to_owned())
    });
    let path = parts.uri.path().to_owned();
    lock(&state.calls).push(Recorded {
        method: parts.method.as_str().to_owned(),
        path: path.clone(),
        query: parts.uri.query().map(str::to_owned),
        host,
        headers: parts
            .headers
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_owned(),
                    String::from_utf8_lossy(value.as_bytes()).into_owned(),
                )
            })
            .collect(),
        body,
    });
    let reply = lock(&state.routes)
        .get(&path)
        .cloned()
        .or_else(|| lock(&state.fallback).clone());
    let Some(reply) = reply else {
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(format!("No scripted reply for {path}")))
            .expect("a response");
    };
    if !reply.delay.is_zero() {
        tokio::time::sleep(reply.delay).await;
    }
    let mut response = Response::builder().status(reply.status);
    for (name, value) in &reply.headers {
        response = response.header(name, value);
    }
    let body = match reply.body {
        ReplyBody::Bytes(bytes) => Body::from(bytes),
        ReplyBody::Chunks { chunks, gap, hold } => {
            let counter = DropCounter(Arc::clone(&state));
            let chunks = futures::stream::iter(chunks).then(move |chunk| async move {
                if !gap.is_zero() {
                    tokio::time::sleep(gap).await;
                }
                Ok::<_, Infallible>(Bytes::from(chunk))
            });
            let chunks = if hold {
                chunks.chain(futures::stream::pending()).boxed()
            } else {
                chunks.boxed()
            };
            Body::from_stream(chunks.map(move |chunk| {
                let _ = &counter;
                chunk
            }))
        }
    };
    response.body(body).expect("a response")
}

pub struct FakeTextDriver {
    deltas: Vec<String>,
    text: String,
    fail_after: Option<(usize, AppError)>,
    hold_after: Option<usize>,
    requests: Mutex<Vec<GenerateRequest>>,
    holding: Arc<tokio::sync::Notify>,
    emitted: Arc<AtomicUsize>,
}

impl FakeTextDriver {
    pub fn with_deltas(deltas: &[&str]) -> Self {
        Self {
            deltas: deltas.iter().map(|delta| (*delta).to_owned()).collect(),
            text: deltas.concat(),
            fail_after: None,
            hold_after: None,
            requests: Mutex::new(Vec::new()),
            holding: Arc::new(tokio::sync::Notify::new()),
            emitted: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn with_text(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            ..Self::with_deltas(&[])
        }
    }

    #[must_use]
    pub fn failing_after(mut self, count: usize, error: AppError) -> Self {
        self.fail_after = Some((count, error));
        self
    }

    #[must_use]
    pub const fn holding_after(mut self, count: usize) -> Self {
        self.hold_after = Some(count);
        self
    }

    pub fn requests(&self) -> Vec<GenerateRequest> {
        lock(&self.requests).clone()
    }

    pub fn emitted(&self) -> usize {
        self.emitted.load(Ordering::SeqCst)
    }

    pub async fn when_holding(&self) {
        self.holding.notified().await;
    }
}

#[async_trait::async_trait]
impl TextGenerationDriver for FakeTextDriver {
    fn capabilities(&self) -> AdapterCapabilities {
        crate::drivers::ai::adapters::OPENAI_COMPATIBLE_CAPABILITIES
    }

    async fn generate(
        &self,
        request: &GenerateRequest,
        _cancel: &CancellationToken,
    ) -> Result<GenerateResult, AppError> {
        lock(&self.requests).push(request.clone());
        Ok(GenerateResult {
            model: request.model.clone(),
            text: self.text.clone(),
            reasoning: None,
            finish_reason: FinishReason::Stop,
            usage: None,
            tool_calls: Vec::new(),
        })
    }

    fn stream(&self, request: &GenerateRequest, cancel: &CancellationToken) -> DeltaStream {
        lock(&self.requests).push(request.clone());
        let deltas = self.deltas.clone();
        let fail_after = self.fail_after.clone();
        let hold_after = self.hold_after;
        let holding = Arc::clone(&self.holding);
        let emitted = Arc::clone(&self.emitted);
        let cancel = cancel.clone();
        futures::stream::unfold(Some(0_usize), move |index| {
            let deltas = deltas.clone();
            let fail_after = fail_after.clone();
            let holding = Arc::clone(&holding);
            let emitted = Arc::clone(&emitted);
            let cancel = cancel.clone();
            async move {
                let index = index?;
                if let Some((_, error)) = fail_after.filter(|(count, _)| *count == index) {
                    return Some((Err(error), None));
                }
                if hold_after == Some(index) {
                    holding.notify_one();
                    cancel.cancelled().await;
                }
                if cancel.is_cancelled() {
                    return Some((Err(super::errors::cancelled()), None));
                }
                let delta = deltas.get(index)?.clone();
                emitted.fetch_add(1, Ordering::SeqCst);
                Some((Ok(TextDelta::text(delta)), Some(index + 1)))
            }
        })
        .boxed()
    }

    async fn list_models(
        &self,
        _cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        Ok(vec![DiscoveredModel::named("m")])
    }
}
