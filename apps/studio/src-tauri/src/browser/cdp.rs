use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tauri::Webview;
use tokio::sync::{mpsc, oneshot};

use crate::platform::site;

pub const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(10);
pub const BINDING_CALLED: &str = "Runtime.bindingCalled";

pub type CdpReply = Box<dyn FnOnce(Result<String, String>) + Send>;
pub type CdpEventSink = Arc<dyn Fn(String) + Send + Sync>;

pub trait CdpTransport: Send + Sync {
    fn call(&self, method: &str, params: String, reply: CdpReply);

    fn listen(&self, event: &str, sink: CdpEventSink) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CdpError {
    Protocol { code: i64, message: String },
    Exception(String),
    Transport(String),
    Decode(String),
    Timeout(String),
    Closed,
}

impl fmt::Display for CdpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protocol { code, message } => write!(formatter, "CDP error {code}: {message}"),
            Self::Exception(text) => write!(formatter, "page exception: {text}"),
            Self::Transport(text) => write!(formatter, "CDP transport failed: {text}"),
            Self::Decode(text) => write!(formatter, "CDP reply did not decode: {text}"),
            Self::Timeout(method) => write!(formatter, "CDP call {method} timed out"),
            Self::Closed => formatter.write_str("CDP client is closed"),
        }
    }
}

impl std::error::Error for CdpError {}

#[derive(Debug, Deserialize)]
struct ProtocolError {
    code: i64,
    message: String,
}

fn transport_failure(failure: String) -> CdpError {
    serde_json::from_str::<ProtocolError>(&failure)
        .or_else(|_| {
            serde_json::from_str::<Value>(&failure)
                .ok()
                .and_then(|value| value.get("error").cloned())
                .ok_or(())
                .and_then(|error| serde_json::from_value::<ProtocolError>(error).map_err(|_| ()))
        })
        .map_or(CdpError::Transport(failure), |error| CdpError::Protocol {
            code: error.code,
            message: error.message,
        })
}

type Pending = HashMap<u64, oneshot::Sender<Result<Value, CdpError>>>;
type Subscribers = HashMap<String, Vec<mpsc::UnboundedSender<Value>>>;

struct Shared {
    pending: Mutex<Pending>,
    subscribers: Mutex<Subscribers>,
    closed: AtomicBool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    fn settle(&self, id: u64, outcome: Result<Value, CdpError>) {
        if let Some(sender) = lock(&self.pending).remove(&id) {
            let _ = sender.send(outcome);
        }
    }

    fn deliver(&self, event: &str, payload: &str) {
        let Ok(value) = serde_json::from_str::<Value>(payload) else {
            return;
        };
        let mut subscribers = lock(&self.subscribers);
        if let Some(senders) = subscribers.get_mut(event) {
            senders.retain(|sender| sender.send(value.clone()).is_ok());
        }
    }
}

pub struct CdpEvents(mpsc::UnboundedReceiver<Value>);

impl CdpEvents {
    pub async fn next(&mut self) -> Option<Value> {
        self.0.recv().await
    }

    pub async fn next_as<T: DeserializeOwned>(&mut self) -> Option<Result<T, CdpError>> {
        let value = self.next().await?;
        Some(serde_json::from_value(value).map_err(|error| CdpError::Decode(error.to_string())))
    }

    pub fn try_next(&mut self) -> Option<Value> {
        self.0.try_recv().ok()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteObject {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub subtype: Option<String>,
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub object_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionDetails {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub exception: Option<RemoteObject>,
}

impl ExceptionDetails {
    fn summary(&self) -> String {
        self.exception
            .as_ref()
            .and_then(|exception| exception.description.clone())
            .unwrap_or_else(|| self.text.clone())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvaluationResult {
    result: RemoteObject,
    #[serde(default)]
    exception_details: Option<ExceptionDetails>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingCalled {
    pub name: String,
    pub payload: String,
    pub execution_context_id: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Evaluate {
    pub expression: String,
    pub context_id: Option<i64>,
    pub await_promise: bool,
    pub return_by_value: bool,
}

impl Evaluate {
    pub fn value(expression: impl Into<String>) -> Self {
        Self {
            expression: expression.into(),
            context_id: None,
            await_promise: false,
            return_by_value: true,
        }
    }

    #[must_use]
    pub const fn in_context(mut self, context_id: i64) -> Self {
        self.context_id = Some(context_id);
        self
    }

    #[must_use]
    pub const fn awaiting(mut self) -> Self {
        self.await_promise = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallFunctionOn {
    pub declaration: &'static str,
    pub context_id: i64,
    pub arguments: Vec<Value>,
    pub await_promise: bool,
}

impl CallFunctionOn {
    pub fn new(declaration: &'static str, context_id: i64, arguments: Vec<Value>) -> Self {
        Self {
            declaration,
            context_id,
            arguments,
            await_promise: false,
        }
    }

    #[must_use]
    pub const fn awaiting(mut self) -> Self {
        self.await_promise = true;
        self
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrameTreeReply {
    frame_tree: FrameTree,
}

#[derive(Debug, Deserialize)]
struct FrameTree {
    frame: Frame,
}

#[derive(Debug, Deserialize)]
struct Frame {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IsolatedWorld {
    execution_context_id: i64,
}

#[derive(Clone)]
pub struct CdpClient {
    transport: Arc<dyn CdpTransport>,
    shared: Arc<Shared>,
    ids: Arc<AtomicU64>,
    timeout: Duration,
}

impl CdpClient {
    pub fn new(transport: Arc<dyn CdpTransport>) -> Self {
        Self::with_timeout(transport, DEFAULT_CALL_TIMEOUT)
    }

    pub fn with_timeout(transport: Arc<dyn CdpTransport>, timeout: Duration) -> Self {
        Self {
            transport,
            shared: Arc::new(Shared {
                pending: Mutex::default(),
                subscribers: Mutex::default(),
                closed: AtomicBool::new(false),
            }),
            ids: Arc::new(AtomicU64::new(1)),
            timeout,
        }
    }

    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::SeqCst)
    }

    pub fn pending(&self) -> usize {
        lock(&self.shared.pending).len()
    }

    pub fn close(&self) {
        self.shared.closed.store(true, Ordering::SeqCst);
        let pending = std::mem::take(&mut *lock(&self.shared.pending));
        for (_, sender) in pending {
            let _ = sender.send(Err(CdpError::Closed));
        }
        lock(&self.shared.subscribers).clear();
    }

    pub async fn send(&self, method: &str, params: Value) -> Result<Value, CdpError> {
        if self.is_closed() {
            return Err(CdpError::Closed);
        }
        let id = self.ids.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        lock(&self.shared.pending).insert(id, sender);
        let shared: Weak<Shared> = Arc::downgrade(&self.shared);
        self.transport.call(
            method,
            params.to_string(),
            Box::new(move |outcome| {
                if let Some(shared) = shared.upgrade() {
                    let decoded = outcome.map_err(transport_failure).and_then(|text| {
                        if text.is_empty() {
                            Ok(Value::Object(serde_json::Map::new()))
                        } else {
                            serde_json::from_str(&text)
                                .map_err(|error| CdpError::Decode(error.to_string()))
                        }
                    });
                    shared.settle(id, decoded);
                }
            }),
        );
        match tokio::time::timeout(self.timeout, receiver).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(_)) => Err(CdpError::Closed),
            Err(_) => {
                lock(&self.shared.pending).remove(&id);
                Err(CdpError::Timeout(method.to_owned()))
            }
        }
    }

    pub fn subscribe(&self, event: &str) -> Result<CdpEvents, CdpError> {
        if self.is_closed() {
            return Err(CdpError::Closed);
        }
        let (sender, receiver) = mpsc::unbounded_channel();
        let first = {
            let mut subscribers = lock(&self.shared.subscribers);
            let senders = subscribers.entry(event.to_owned()).or_default();
            senders.push(sender);
            senders.len() == 1
        };
        if first {
            let shared = Arc::downgrade(&self.shared);
            let name = event.to_owned();
            let listened = self.transport.listen(
                event,
                Arc::new(move |payload| {
                    if let Some(shared) = shared.upgrade() {
                        shared.deliver(&name, &payload);
                    }
                }),
            );
            if let Err(error) = listened {
                lock(&self.shared.subscribers).remove(event);
                return Err(CdpError::Transport(error));
            }
        }
        Ok(CdpEvents(receiver))
    }

    pub async fn runtime_enable(&self) -> Result<(), CdpError> {
        self.send("Runtime.enable", json!({})).await.map(drop)
    }

    pub async fn add_binding(
        &self,
        name: &str,
        context_name: Option<&str>,
    ) -> Result<(), CdpError> {
        let mut params = json!({ "name": name });
        if let Some(context_name) = context_name {
            params["executionContextName"] = json!(context_name);
        }
        self.send("Runtime.addBinding", params).await.map(drop)
    }

    pub fn binding_calls(&self) -> Result<CdpEvents, CdpError> {
        self.subscribe(BINDING_CALLED)
    }

    pub async fn evaluate(&self, evaluate: Evaluate) -> Result<RemoteObject, CdpError> {
        let mut params = json!({
            "expression": evaluate.expression,
            "awaitPromise": evaluate.await_promise,
            "returnByValue": evaluate.return_by_value,
        });
        if let Some(context_id) = evaluate.context_id {
            params["contextId"] = json!(context_id);
        }
        evaluation(self.send("Runtime.evaluate", params).await?)
    }

    pub async fn call_function_on(&self, call: CallFunctionOn) -> Result<RemoteObject, CdpError> {
        let arguments = call
            .arguments
            .into_iter()
            .map(|value| json!({ "value": value }))
            .collect::<Vec<_>>();
        let params = json!({
            "functionDeclaration": call.declaration,
            "executionContextId": call.context_id,
            "arguments": arguments,
            "awaitPromise": call.await_promise,
            "returnByValue": true,
        });
        evaluation(self.send("Runtime.callFunctionOn", params).await?)
    }

    pub async fn main_frame(&self) -> Result<String, CdpError> {
        let reply: FrameTreeReply = decode(self.send("Page.getFrameTree", json!({})).await?)?;
        Ok(reply.frame_tree.frame.id)
    }

    pub async fn create_isolated_world(
        &self,
        frame_id: &str,
        world_name: &str,
    ) -> Result<i64, CdpError> {
        let reply: IsolatedWorld = decode(
            self.send(
                "Page.createIsolatedWorld",
                json!({ "frameId": frame_id, "worldName": world_name }),
            )
            .await?,
        )?;
        Ok(reply.execution_context_id)
    }
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, CdpError> {
    serde_json::from_value(value).map_err(|error| CdpError::Decode(error.to_string()))
}

fn evaluation(value: Value) -> Result<RemoteObject, CdpError> {
    let reply: EvaluationResult = decode(value)?;
    match reply.exception_details {
        Some(details) => Err(CdpError::Exception(details.summary())),
        None => Ok(reply.result),
    }
}

pub struct WebviewCdp(Webview);

impl WebviewCdp {
    pub const fn new(webview: Webview) -> Self {
        Self(webview)
    }

    pub fn client(webview: Webview, timeout: Duration) -> CdpClient {
        CdpClient::with_timeout(Arc::new(Self(webview)), timeout)
    }
}

impl CdpTransport for WebviewCdp {
    fn call(&self, method: &str, params: String, reply: CdpReply) {
        let method = method.to_owned();
        let reply = Arc::new(Mutex::new(Some(reply)));
        let fallback = Arc::clone(&reply);
        let posted = self.0.with_webview(move |platform| {
            if let Some(reply) = lock(&reply).take() {
                site::call_devtools(&platform, &method, &params, reply);
            }
        });
        if let Err(error) = posted
            && let Some(reply) = lock(&fallback).take()
        {
            reply(Err(error.to_string()));
        }
    }

    fn listen(&self, event: &str, sink: CdpEventSink) -> Result<(), String> {
        let event = event.to_owned();
        self.0
            .with_webview(move |platform| {
                let _ = site::listen_devtools(&platform, &event, sink);
            })
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
pub mod testing {
    use super::*;

    pub type Handler = Box<dyn Fn(&str, &Value) -> Option<Result<Value, String>> + Send + Sync>;

    #[derive(Default)]
    pub struct FakeCdp {
        pub calls: Mutex<Vec<(String, Value)>>,
        pub held: Mutex<Vec<(String, CdpReply)>>,
        pub sinks: Mutex<HashMap<String, CdpEventSink>>,
        pub listens: Mutex<Vec<String>>,
        pub handler: Mutex<Option<Handler>>,
        pub refuse_listen: AtomicBool,
    }

    impl FakeCdp {
        pub fn shared() -> Arc<Self> {
            Arc::new(Self::default())
        }

        pub fn handle(
            &self,
            handler: impl Fn(&str, &Value) -> Option<Result<Value, String>> + Send + Sync + 'static,
        ) {
            *lock(&self.handler) = Some(Box::new(handler));
        }

        pub fn emit(&self, event: &str, payload: &Value) {
            let sink = lock(&self.sinks).get(event).cloned();
            if let Some(sink) = sink {
                sink(payload.to_string());
            }
        }

        pub fn methods(&self) -> Vec<String> {
            lock(&self.calls)
                .iter()
                .map(|(method, _)| method.clone())
                .collect()
        }

        pub fn release(&self, method: &str, outcome: Result<String, String>) {
            let held = {
                let mut held = lock(&self.held);
                let index = held.iter().position(|(name, _)| name == method);
                index.map(|index| held.remove(index))
            };
            if let Some((_, reply)) = held {
                reply(outcome);
            }
        }
    }

    impl CdpTransport for FakeCdp {
        fn call(&self, method: &str, params: String, reply: CdpReply) {
            let params: Value = serde_json::from_str(&params).expect("params are JSON");
            lock(&self.calls).push((method.to_owned(), params.clone()));
            let answer = lock(&self.handler)
                .as_ref()
                .and_then(|handler| handler(method, &params));
            match answer {
                Some(outcome) => reply(outcome.map(|value| value.to_string())),
                None => lock(&self.held).push((method.to_owned(), reply)),
            }
        }

        fn listen(&self, event: &str, sink: CdpEventSink) -> Result<(), String> {
            if self.refuse_listen.load(Ordering::SeqCst) {
                return Err("listener refused".to_owned());
            }
            lock(&self.listens).push(event.to_owned());
            lock(&self.sinks).insert(event.to_owned(), sink);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::FakeCdp;
    use super::*;

    fn client(fake: &Arc<FakeCdp>) -> CdpClient {
        CdpClient::with_timeout(fake.clone(), Duration::from_secs(5))
    }

    #[tokio::test]
    async fn a_command_resolves_with_its_own_reply() {
        let fake = FakeCdp::shared();
        let cdp = client(&fake);
        let first = tokio::spawn({
            let cdp = cdp.clone();
            async move { cdp.send("Runtime.enable", json!({})).await }
        });
        let second = tokio::spawn({
            let cdp = cdp.clone();
            async move { cdp.send("Page.getFrameTree", json!({})).await }
        });
        while cdp.pending() < 2 {
            tokio::task::yield_now().await;
        }
        fake.release(
            "Page.getFrameTree",
            Ok(r#"{"frameTree":{"frame":{"id":"F"}}}"#.to_owned()),
        );
        fake.release("Runtime.enable", Ok(String::new()));
        assert_eq!(first.await.expect("joins").expect("enabled"), json!({}));
        assert_eq!(
            second.await.expect("joins").expect("tree")["frameTree"]["frame"]["id"],
            "F"
        );
        assert_eq!(cdp.pending(), 0);
    }

    #[tokio::test]
    async fn protocol_errors_exceptions_and_garbage_are_told_apart() {
        let fake = FakeCdp::shared();
        fake.handle(|method, params| match method {
            "Runtime.addBinding" => Some(Err(r#"{"code":-32000,"message":"bad name"}"#.to_owned())),
            "Runtime.enable" => Some(Err("0x80004005".to_owned())),
            "Runtime.evaluate" if params["expression"] == "throw 1" => Some(Ok(json!({
                "result": { "type": "object" },
                "exceptionDetails": { "text": "Uncaught", "exception": { "type": "number", "description": "1" } }
            }))),
            "Runtime.evaluate" => Some(Ok(json!({ "result": { "type": "number", "value": 2 } }))),
            "Page.getFrameTree" => Some(Ok(json!({ "nope": true }))),
            _ => None,
        });
        let cdp = client(&fake);
        assert_eq!(
            cdp.add_binding("x", None).await.expect_err("refused"),
            CdpError::Protocol {
                code: -32000,
                message: "bad name".to_owned()
            }
        );
        assert_eq!(
            cdp.runtime_enable().await.expect_err("refused"),
            CdpError::Transport("0x80004005".to_owned())
        );
        assert_eq!(
            cdp.evaluate(Evaluate::value("throw 1"))
                .await
                .expect_err("throws"),
            CdpError::Exception("1".to_owned())
        );
        assert_eq!(
            cdp.evaluate(Evaluate::value("1 + 1").in_context(7))
                .await
                .expect("evaluates")
                .value,
            Some(json!(2))
        );
        assert!(matches!(
            cdp.main_frame().await.expect_err("garbage"),
            CdpError::Decode(_)
        ));
        let (_, evaluate) = lock(&fake.calls)
            .iter()
            .rfind(|(method, _)| method == "Runtime.evaluate")
            .cloned()
            .expect("evaluated");
        assert_eq!(evaluate["contextId"], 7);
        assert_eq!(evaluate["returnByValue"], true);
    }

    #[tokio::test]
    async fn arguments_travel_as_values_and_never_as_source() {
        let fake = FakeCdp::shared();
        fake.handle(|method, _| match method {
            "Runtime.callFunctionOn" => Some(Ok(
                json!({ "result": { "type": "boolean", "value": true } }),
            )),
            "Page.getFrameTree" => Some(Ok(json!({ "frameTree": { "frame": { "id": "MAIN" } } }))),
            "Page.createIsolatedWorld" => Some(Ok(json!({ "executionContextId": 42 }))),
            "Runtime.addBinding" => Some(Ok(json!({}))),
            _ => None,
        });
        let cdp = client(&fake);
        let frame = cdp.main_frame().await.expect("frame");
        let world = cdp
            .create_isolated_world(&frame, "zvs")
            .await
            .expect("world");
        cdp.add_binding("send", Some("zvs")).await.expect("binding");
        let hostile = "\"); alert(1); (\"";
        let result = cdp
            .call_function_on(
                CallFunctionOn::new(
                    "function (request) { return true; }",
                    world,
                    vec![json!({ "url": hostile })],
                )
                .awaiting(),
            )
            .await
            .expect("calls");
        assert_eq!(result.value, Some(json!(true)));
        let calls = lock(&fake.calls).clone();
        let (_, call) = calls
            .iter()
            .find(|(method, _)| method == "Runtime.callFunctionOn")
            .expect("called");
        assert_eq!(
            call["functionDeclaration"],
            "function (request) { return true; }"
        );
        assert_eq!(call["arguments"][0]["value"]["url"], hostile);
        assert_eq!(call["executionContextId"], 42);
        assert_eq!(call["awaitPromise"], true);
        let (_, binding) = calls
            .iter()
            .find(|(method, _)| method == "Runtime.addBinding")
            .expect("bound");
        assert_eq!(binding["executionContextName"], "zvs");
        let (_, world) = calls
            .iter()
            .find(|(method, _)| method == "Page.createIsolatedWorld")
            .expect("world");
        assert_eq!(world["frameId"], "MAIN");
    }

    #[tokio::test]
    async fn events_fan_out_to_every_subscriber_and_are_listened_to_once() {
        let fake = FakeCdp::shared();
        let cdp = client(&fake);
        let mut first = cdp.binding_calls().expect("subscribes");
        let mut second = cdp.subscribe(BINDING_CALLED).expect("subscribes");
        assert_eq!(*lock(&fake.listens), vec![BINDING_CALLED.to_owned()]);
        fake.emit(
            BINDING_CALLED,
            &json!({ "name": "send", "payload": "{}", "executionContextId": 3 }),
        );
        let called: BindingCalled = first.next_as().await.expect("event").expect("decodes");
        assert_eq!(
            called,
            BindingCalled {
                name: "send".to_owned(),
                payload: "{}".to_owned(),
                execution_context_id: 3
            }
        );
        assert_eq!(second.next().await.expect("event")["executionContextId"], 3);
        drop(first);
        fake.emit(
            BINDING_CALLED,
            &json!({ "name": "send", "payload": "1", "executionContextId": 3 }),
        );
        assert_eq!(second.try_next().expect("still delivered")["payload"], "1");

        let refusing = FakeCdp::shared();
        refusing.refuse_listen.store(true, Ordering::SeqCst);
        let cdp = client(&refusing);
        assert!(matches!(
            cdp.subscribe("Page.loadEventFired"),
            Err(CdpError::Transport(_))
        ));
    }

    #[tokio::test]
    async fn closing_fails_every_pending_call_and_refuses_new_ones() {
        let fake = FakeCdp::shared();
        let cdp = client(&fake);
        let waiting = {
            let cdp = cdp.clone();
            tokio::spawn(async move { cdp.send("Runtime.enable", json!({})).await })
        };
        while cdp.pending() == 0 {
            tokio::task::yield_now().await;
        }
        let mut events = cdp.binding_calls().expect("subscribes");
        cdp.close();
        assert_eq!(waiting.await.expect("joins"), Err(CdpError::Closed));
        assert_eq!(events.next().await, None);
        assert_eq!(
            cdp.send("Runtime.enable", json!({})).await,
            Err(CdpError::Closed)
        );
        assert!(matches!(cdp.binding_calls(), Err(CdpError::Closed)));
        fake.release("Runtime.enable", Ok(String::new()));
    }

    #[tokio::test(start_paused = true)]
    async fn an_unanswered_call_times_out_and_is_forgotten() {
        let fake = FakeCdp::shared();
        let cdp = CdpClient::with_timeout(fake.clone(), Duration::from_millis(50));
        assert_eq!(
            cdp.send("Runtime.evaluate", json!({})).await,
            Err(CdpError::Timeout("Runtime.evaluate".to_owned()))
        );
        assert_eq!(cdp.pending(), 0);
        fake.release("Runtime.evaluate", Ok("{}".to_owned()));
    }
}
