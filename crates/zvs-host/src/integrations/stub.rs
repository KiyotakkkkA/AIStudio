use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub delay: Duration,
}

impl Reply {
    pub fn json(value: &Value) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type".to_owned(), "application/json".to_owned())],
            body: value.to_string(),
            delay: Duration::ZERO,
        }
    }

    #[must_use]
    pub const fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    #[must_use]
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    #[must_use]
    pub const fn delayed(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

#[derive(Debug, Clone)]
pub struct Call {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Call {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(existing, _)| existing.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

#[derive(Default)]
struct StubState {
    queued: Mutex<HashMap<String, VecDeque<Reply>>>,
    steady: Mutex<HashMap<String, Reply>>,
    calls: Mutex<Vec<Call>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct StubServer {
    address: SocketAddr,
    state: Arc<StubState>,
    task: JoinHandle<()>,
}

impl Drop for StubServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl StubServer {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a local port");
        let address = listener.local_addr().expect("an address");
        let state = Arc::new(StubState::default());
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

    pub fn origin(&self) -> String {
        format!("http://{}", self.address)
    }

    pub fn steady(&self, path: &str, reply: Reply) -> &Self {
        lock(&self.state.steady).insert(path.to_owned(), reply);
        self
    }

    pub fn queue(&self, path: &str, reply: Reply) -> &Self {
        lock(&self.state.queued)
            .entry(path.to_owned())
            .or_default()
            .push_back(reply);
        self
    }

    pub fn calls(&self) -> Vec<Call> {
        lock(&self.state.calls).clone()
    }

    pub fn calls_to(&self, path: &str) -> Vec<Call> {
        self.calls()
            .into_iter()
            .filter(|call| call.path == path)
            .collect()
    }
}

async fn handle(State(state): State<Arc<StubState>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let body = axum::body::to_bytes(body, usize::MAX)
        .await
        .map(|bytes| bytes.to_vec())
        .unwrap_or_default();
    let path = parts.uri.path().to_owned();
    lock(&state.calls).push(Call {
        method: parts.method.as_str().to_owned(),
        path: path.clone(),
        query: parts.uri.query().unwrap_or_default().to_owned(),
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
    let queued = lock(&state.queued)
        .get_mut(&path)
        .and_then(VecDeque::pop_front);
    let reply = queued.or_else(|| lock(&state.steady).get(&path).cloned());
    let Some(reply) = reply else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from(format!("No stub reply for {path}")))
            .expect("a response");
    };
    if !reply.delay.is_zero() {
        tokio::time::sleep(reply.delay).await;
    }
    let mut response = Response::builder().status(reply.status);
    for (name, value) in &reply.headers {
        response = response.header(name, value);
    }
    response.body(Body::from(reply.body)).expect("a response")
}
