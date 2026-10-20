use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};

use futures::StreamExt;
use tokio::sync::Notify;
use zvs_host::drivers::ai::errors::{
    BROWSER_VERIFICATION_MESSAGE, CANCELLED_MESSAGE, TIMED_OUT_MESSAGE,
};

use super::*;
use crate::browser::bearer::Sighting;

const QWEN: &str = "https://chat.qwen.ai/api/v2/chat/completions";
const QWEN_NEW_CHAT: &str = "https://chat.qwen.ai/api/v2/chats/new";
const FIXTURE: &str = "http://127.0.0.1:4100/api/v1/auths/";

#[derive(Clone)]
enum Plan {
    Respond {
        status: u16,
        headers: Vec<(String, String)>,
        reads: Vec<Vec<u8>>,
        stall_after: Option<usize>,
    },
    NetworkError,
    FailAfter(Vec<Vec<u8>>),
    Refuse,
    Silent,
    /// Post a verification dialog first, then follow the inner plan once released.
    ChallengeThen(Box<Plan>),
}

fn respond(status: u16, reads: Vec<Vec<u8>>) -> Plan {
    Plan::Respond {
        status,
        headers: vec![("Content-Type".to_owned(), "text/plain".to_owned())],
        reads,
        stall_after: None,
    }
}

struct Flow {
    credit: AtomicI64,
    aborted: AtomicBool,
    wake: Notify,
}

struct FakePage {
    routes: SharedRoutes,
    healthy: AtomicBool,
    commands: Mutex<Vec<Value>>,
    plan: Mutex<Plan>,
    flows: Mutex<HashMap<String, Arc<Flow>>>,
    outstanding: AtomicI64,
    peak: AtomicI64,
    revealed: AtomicUsize,
    concealed: AtomicUsize,
    passed: Notify,
    renewal: Arc<Mutex<Value>>,
}

impl FakePage {
    fn commands(&self, op: &str) -> Vec<Value> {
        lock(&self.commands)
            .iter()
            .filter(|command| command["op"] == op)
            .cloned()
            .collect()
    }

    fn post(&self, message: &Value) {
        self.routes.deliver(&message.to_string());
    }

    async fn spend(&self, flow: &Flow) -> bool {
        loop {
            if flow.aborted.load(Ordering::SeqCst) {
                return false;
            }
            if flow.credit.load(Ordering::SeqCst) > 0 {
                flow.credit.fetch_sub(1, Ordering::SeqCst);
                let outstanding = self.outstanding.fetch_add(1, Ordering::SeqCst) + 1;
                self.peak.fetch_max(outstanding, Ordering::SeqCst);
                return true;
            }
            let woken = flow.wake.notified();
            if flow.aborted.load(Ordering::SeqCst) || flow.credit.load(Ordering::SeqCst) > 0 {
                continue;
            }
            woken.await;
        }
    }

    async fn run(self: Arc<Self>, command: Value, plan: Plan, flow: Arc<Flow>) {
        let id = command["id"].as_str().expect("an id").to_owned();
        let chunk = command["chunk"].as_u64().expect("a chunk size") as usize;
        let aborted = |page: &Self| {
            page.post(&json!({ "id": id, "kind": "error", "aborted": true, "message": "aborted" }));
        };
        let plan = match plan {
            Plan::ChallengeThen(inner) => {
                self.post(&json!({ "id": id, "kind": "challenge" }));
                self.passed.notified().await;
                *inner
            }
            plan => plan,
        };
        let (status, headers, reads, stall_after, fail) = match plan {
            Plan::Respond {
                status,
                headers,
                reads,
                stall_after,
            } => (status, headers, reads, stall_after, false),
            Plan::FailAfter(reads) => (200, Vec::new(), reads, None, true),
            Plan::NetworkError => {
                self.post(&json!({ "id": id, "kind": "error", "aborted": false, "message": "Failed to fetch" }));
                return;
            }
            Plan::Silent | Plan::Refuse | Plan::ChallengeThen(_) => return,
        };
        self.post(&json!({ "id": id, "kind": "head", "status": status, "headers": headers }));
        let mut sent = 0;
        for read in reads {
            for piece in read.chunks(chunk) {
                if stall_after == Some(sent) {
                    while !flow.aborted.load(Ordering::SeqCst) {
                        flow.wake.notified().await;
                    }
                }
                if !self.spend(&flow).await {
                    aborted(&self);
                    return;
                }
                sent += 1;
                self.post(&json!({ "id": id, "kind": "chunk", "data": STANDARD.encode(piece) }));
            }
        }
        if fail {
            self.post(&json!({ "id": id, "kind": "error", "aborted": false, "message": "network changed" }));
        } else {
            self.post(&json!({ "id": id, "kind": "end" }));
        }
    }
}

#[async_trait]
impl GatewayPage for Arc<FakePage> {
    async fn command(&self, command: Value) -> Result<Value, String> {
        lock(&self.commands).push(command.clone());
        if !self.healthy() {
            return Err("closed".to_owned());
        }
        let id = command["id"].as_str().unwrap_or_default().to_owned();
        match command["op"].as_str() {
            Some("start") => {
                let plan = lock(&self.plan).clone();
                if matches!(plan, Plan::Refuse) {
                    return Err("Cannot find context with specified id".to_owned());
                }
                let flow = Arc::new(Flow {
                    credit: AtomicI64::new(command["window"].as_i64().expect("a window")),
                    aborted: AtomicBool::new(false),
                    wake: Notify::new(),
                });
                lock(&self.flows).insert(id, Arc::clone(&flow));
                tokio::spawn(Arc::clone(self).run(command, plan, flow));
                Ok(json!(true))
            }
            Some("ack") => {
                let credit = command["credit"].as_i64().expect("credit");
                self.outstanding.fetch_sub(credit, Ordering::SeqCst);
                if let Some(flow) = lock(&self.flows).get(&id) {
                    flow.credit.fetch_add(credit, Ordering::SeqCst);
                    flow.wake.notify_waiters();
                }
                Ok(json!(true))
            }
            Some("renew") => {
                assert_eq!(command["session"]["key"], "token");
                Ok(lock(&self.renewal).clone())
            }
            Some("abort") => {
                if let Some(flow) = lock(&self.flows).get(&id) {
                    flow.aborted.store(true, Ordering::SeqCst);
                    flow.wake.notify_waiters();
                }
                Ok(json!(true))
            }
            _ => Err("unknown".to_owned()),
        }
    }

    fn healthy(&self) -> bool {
        self.healthy.load(Ordering::SeqCst)
    }

    fn reveal(&self) {
        self.revealed.fetch_add(1, Ordering::SeqCst);
    }

    fn conceal(&self) {
        self.concealed.fetch_add(1, Ordering::SeqCst);
    }

    fn close(&self) {
        self.healthy.store(false, Ordering::SeqCst);
        self.routes.fail_all();
    }
}

#[derive(Default)]
struct FakePages {
    plan: Mutex<Option<Plan>>,
    pages: Mutex<Vec<(String, Arc<FakePage>)>>,
    failures: AtomicUsize,
    hang: AtomicBool,
    disposed: AtomicBool,
    renewal: Arc<Mutex<Value>>,
}

impl FakePages {
    fn shared(plan: Plan) -> Arc<Self> {
        let pages = Self::default();
        *lock(&pages.plan) = Some(plan);
        Arc::new(pages)
    }

    fn pages(&self) -> Vec<Arc<FakePage>> {
        lock(&self.pages)
            .iter()
            .map(|(_, page)| Arc::clone(page))
            .collect()
    }

    fn page(&self, index: usize) -> Arc<FakePage> {
        self.pages()[index].clone()
    }

    fn origins(&self) -> Vec<String> {
        lock(&self.pages)
            .iter()
            .map(|(origin, _)| origin.clone())
            .collect()
    }

    fn set_plan(&self, plan: Plan) {
        *lock(&self.plan) = Some(plan.clone());
        for page in self.pages() {
            *lock(&page.plan) = plan.clone();
        }
    }
}

#[async_trait]
impl PageFactory for FakePages {
    async fn open(
        &self,
        origin: &str,
        page: &Url,
        routes: SharedRoutes,
    ) -> Result<SharedPage, String> {
        assert_eq!(Some(page), page_url(origin).as_ref());
        assert_eq!(page.origin().ascii_serialization(), origin);
        if self.hang.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        if self
            .failures
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            })
            .is_ok()
        {
            return Err("the page did not load".to_owned());
        }
        let fake = Arc::new(FakePage {
            routes,
            healthy: AtomicBool::new(true),
            commands: Mutex::default(),
            plan: Mutex::new(lock(&self.plan).clone().unwrap_or(Plan::Silent)),
            flows: Mutex::default(),
            outstanding: AtomicI64::new(0),
            peak: AtomicI64::new(0),
            revealed: AtomicUsize::new(0),
            concealed: AtomicUsize::new(0),
            passed: Notify::new(),
            renewal: Arc::clone(&self.renewal),
        });
        lock(&self.pages).push((origin.to_owned(), Arc::clone(&fake)));
        Ok(Arc::new(fake))
    }

    fn user_agent(&self) -> Option<String> {
        Some("Mozilla/5.0 Edg/141".to_owned())
    }

    fn dispose(&self) {
        self.disposed.store(true, Ordering::SeqCst);
    }
}

fn options() -> GatewayOptions {
    GatewayOptions {
        ready_timeout: Duration::from_secs(5),
        head_timeout: Duration::from_secs(20),
        idle_timeout: Duration::from_secs(30),
        chunk_size: 1024,
        window: 4,
        ack_every: 2,
        reveal_after: Duration::from_secs(60),
        challenge_timeout: Duration::from_secs(120),
        renew_reuse: Duration::from_secs(5),
    }
}

fn gateway(pages: &Arc<FakePages>) -> AccountGateway {
    gateway_with(pages, BearerTokens::default())
}

fn gateway_with(pages: &Arc<FakePages>, tokens: BearerTokens) -> AccountGateway {
    AccountGateway::with_options(pages.clone(), tokens, options(), silent_logger())
}

fn request() -> SessionRequest {
    SessionRequest::new("POST")
        .with_header("accept", "text/event-stream")
        .with_header("authorization", "Bearer tok")
        .with_header("Cookie", "session=stolen")
        .with_header("user-agent", "agent")
        .with_header("origin", "https://chat.qwen.ai")
        .with_header("referer", "https://chat.qwen.ai/")
        .with_header("Sec-Fetch-Mode", "cors")
        .with_header("content-type", "application/json")
        .with_body(r#"{"stream":true}"#)
}

fn sample(size: usize) -> Vec<u8> {
    (0..size).map(|index| (index * 31 % 251) as u8).collect()
}

#[tokio::test]
async fn a_large_body_is_reassembled_in_order_under_backpressure() {
    let body = sample(64 * 1024 + 17);
    let pages = FakePages::shared(respond(
        200,
        vec![body[..40_000].to_vec(), body[40_000..].to_vec()],
    ));
    let gateway = gateway(&pages);
    let response = gateway
        .fetch(QWEN, request(), &CancellationToken::new())
        .await
        .expect("responds");
    assert_eq!(response.status, 200);
    assert_eq!(response.headers["content-type"], "text/plain");
    let mut stream = response.body;
    let mut collected = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.expect("a chunk");
        assert!(chunk.len() <= 1024);
        collected.extend_from_slice(&chunk);
        tokio::task::yield_now().await;
    }
    assert_eq!(collected, body);
    let page = pages.page(0);
    assert!(page.peak.load(Ordering::SeqCst) <= 4);
    assert!(!page.commands("ack").is_empty());
    assert!(page.routes.is_empty());
    assert!(page.commands("abort").is_empty());
}

#[tokio::test]
async fn the_request_is_passed_as_data_with_browser_owned_headers_removed() {
    let pages = FakePages::shared(respond(200, vec![b"ok".to_vec()]));
    let gateway = gateway(&pages);
    let response = gateway
        .fetch(QWEN, request(), &CancellationToken::new())
        .await
        .expect("responds");
    assert_eq!(response.text().await.expect("reads"), "ok");
    let start = pages.page(0).commands("start").remove(0);
    assert_eq!(start["url"], QWEN);
    assert_eq!(start["method"], "POST");
    assert_eq!(start["body"], r#"{"stream":true}"#);
    assert_eq!(start["page"], "https://chat.qwen.ai/");
    assert_eq!(start["session"]["key"], "token");
    assert_eq!(start["session"]["leeway_ms"], 60_000);
    assert_eq!(start["referrer"], "https://chat.qwen.ai/");
    assert_eq!(
        start["headers"],
        json!([
            ["accept", "text/event-stream"],
            ["authorization", "Bearer tok"],
            ["content-type", "application/json"]
        ])
    );
    assert!(!start.to_string().contains("stolen"));
    let foreign = SessionRequest::new("GET").with_header("referer", "https://evil.example/");
    gateway
        .fetch(FIXTURE, foreign, &CancellationToken::new())
        .await
        .expect("responds");
    let start = pages.page(1).commands("start").remove(0);
    assert_eq!(start["referrer"], Value::Null);
    assert_eq!(start["session"], Value::Null);
    assert_eq!(start["body"], Value::Null);
}

#[tokio::test]
async fn one_gateway_page_serves_each_origin_and_other_origins_are_refused() {
    let pages = FakePages::shared(respond(200, vec![b"ok".to_vec()]));
    let gateway = gateway(&pages);
    let cancel = CancellationToken::new();
    for url in [QWEN, "https://chat.qwen.ai/api/v1/auths/", FIXTURE] {
        gateway
            .fetch(url, SessionRequest::new("GET"), &cancel)
            .await
            .expect("responds");
    }
    assert_eq!(
        pages.origins(),
        vec!["https://chat.qwen.ai", "http://127.0.0.1:4100"]
    );
    for refused in [
        "https://evil.example/api",
        "http://chat.qwen.ai/api",
        "https://chat.qwen.ai.evil.example/",
        "not a url",
    ] {
        let error = gateway
            .fetch(refused, SessionRequest::new("GET"), &cancel)
            .await
            .expect_err("refused");
        assert_eq!(error.code, AppErrorCode::Unknown, "{refused}");
        assert_eq!(
            error.details.as_ref().expect("details")["reason"],
            "origin-not-allowed"
        );
    }
    assert_eq!(pages.pages().len(), 2);
    gateway.dispose();
    assert!(pages.disposed.load(Ordering::SeqCst));
    assert!(pages.pages().iter().all(|page| !page.healthy()));
}

#[tokio::test]
async fn an_event_stream_arrives_incrementally() {
    let pages = FakePages::shared(Plan::Silent);
    let gateway = gateway(&pages);
    let cancel = CancellationToken::new();
    let fetch = gateway.fetch(QWEN, request(), &cancel);
    let posting = async {
        while pages.pages().is_empty() || pages.page(0).commands("start").is_empty() {
            tokio::task::yield_now().await;
        }
        let page = pages.page(0);
        let id = page.commands("start")[0]["id"].clone();
        page.post(&json!({ "id": id, "kind": "head", "status": 200, "headers": [["Content-Type", "text/event-stream"], ["Set-Cookie", "a=b"]] }));
        (page, id)
    };
    let (response, (page, id)) = tokio::join!(fetch, posting);
    let response = response.expect("responds");
    assert_eq!(response.headers["content-type"], "text/event-stream");
    assert!(!response.headers.contains_key("set-cookie"));
    let mut body = response.body;
    for event in ["data: one\n\n", "data: two\n\n"] {
        page.post(&json!({ "id": id, "kind": "chunk", "data": STANDARD.encode(event) }));
        let chunk = body.next().await.expect("a chunk").expect("data");
        assert_eq!(chunk, event.as_bytes());
    }
    page.post(&json!({ "id": id, "kind": "end" }));
    assert!(body.next().await.is_none());
    assert!(body.next().await.is_none());
}

#[tokio::test]
async fn a_failing_status_keeps_its_body() {
    let pages = FakePages::shared(respond(503, vec![br#"{"msg":"busy"}"#.to_vec()]));
    let response = gateway(&pages)
        .fetch(QWEN, request(), &CancellationToken::new())
        .await
        .expect("responds");
    assert_eq!(response.status, 503);
    assert!(!response.ok());
    assert_eq!(response.text().await.expect("reads"), r#"{"msg":"busy"}"#);
}

#[tokio::test]
async fn network_errors_before_and_during_the_body_are_unreachable() {
    let pages = FakePages::shared(Plan::NetworkError);
    let gateway = gateway(&pages);
    let error = gateway
        .fetch(QWEN, request(), &CancellationToken::new())
        .await
        .expect_err("fails");
    assert_eq!(error.code, AppErrorCode::ProviderUnreachable);
    assert_eq!(error.message, UNREACHABLE_MESSAGE);

    pages.set_plan(Plan::FailAfter(vec![b"partial".to_vec()]));
    let response = gateway
        .fetch(QWEN, request(), &CancellationToken::new())
        .await
        .expect("responds");
    let mut body = response.body;
    assert_eq!(body.next().await.expect("chunk").expect("data"), b"partial");
    assert_eq!(
        body.next()
            .await
            .expect("an error")
            .expect_err("fails")
            .code,
        AppErrorCode::ProviderUnreachable
    );
    assert!(body.next().await.is_none());
    assert_eq!(pages.pages().len(), 1);
}

#[tokio::test]
async fn cancelling_mid_stream_aborts_the_page_request() {
    let pages = FakePages::shared(Plan::Respond {
        status: 200,
        headers: Vec::new(),
        reads: vec![b"first".to_vec(), b"second".to_vec()],
        stall_after: Some(1),
    });
    let gateway = gateway(&pages);
    let cancel = CancellationToken::new();
    let response = gateway
        .fetch(QWEN, request(), &cancel)
        .await
        .expect("responds");
    let mut body = response.body;
    assert_eq!(body.next().await.expect("chunk").expect("data"), b"first");
    cancel.cancel();
    let error = body.next().await.expect("an error").expect_err("cancelled");
    assert_eq!(error.code, AppErrorCode::RunCancelled);
    assert_eq!(error.message, CANCELLED_MESSAGE);
    assert!(body.next().await.is_none());
    let page = pages.page(0);
    while page.commands("abort").is_empty() {
        tokio::task::yield_now().await;
    }
    assert!(page.routes.is_empty());
}

#[tokio::test]
async fn cancelling_before_the_head_aborts_and_a_cancelled_token_sends_nothing() {
    let pages = FakePages::shared(Plan::Silent);
    let gateway = gateway(&pages);
    let cancel = CancellationToken::new();
    let fetch = gateway.fetch(QWEN, request(), &cancel);
    let cancelling = async {
        while pages.pages().is_empty() || pages.page(0).commands("start").is_empty() {
            tokio::task::yield_now().await;
        }
        cancel.cancel();
    };
    let (result, ()) = tokio::join!(fetch, cancelling);
    assert_eq!(
        result.expect_err("cancelled").code,
        AppErrorCode::RunCancelled
    );
    let page = pages.page(0);
    while page.commands("abort").is_empty() {
        tokio::task::yield_now().await;
    }
    let before = lock(&page.commands).len();
    assert_eq!(
        gateway
            .fetch(QWEN, request(), &cancel)
            .await
            .expect_err("cancelled")
            .code,
        AppErrorCode::RunCancelled
    );
    assert_eq!(lock(&page.commands).len(), before);
}

#[tokio::test]
async fn dropping_the_body_aborts_the_page_request() {
    let pages = FakePages::shared(Plan::Respond {
        status: 200,
        headers: Vec::new(),
        reads: vec![b"first".to_vec(), b"second".to_vec()],
        stall_after: Some(1),
    });
    let gateway = gateway(&pages);
    let response = gateway
        .fetch(QWEN, request(), &CancellationToken::new())
        .await
        .expect("responds");
    drop(response);
    let page = pages.page(0);
    while page.commands("abort").is_empty() {
        tokio::task::yield_now().await;
    }
    assert!(page.routes.is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_silent_page_times_out_waiting_for_the_head() {
    let pages = FakePages::shared(Plan::Silent);
    let error = gateway(&pages)
        .fetch(QWEN, request(), &CancellationToken::new())
        .await
        .expect_err("times out");
    assert_eq!(error.code, AppErrorCode::ProviderUnreachable);
    assert_eq!(error.message, TIMED_OUT_MESSAGE);
    assert_eq!(error.details.expect("details")["timeoutSeconds"], 20);
    let page = pages.page(0);
    while page.commands("abort").is_empty() {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn a_stalled_body_times_out_after_the_idle_limit() {
    let pages = FakePages::shared(Plan::Respond {
        status: 200,
        headers: Vec::new(),
        reads: vec![b"first".to_vec(), b"second".to_vec()],
        stall_after: Some(1),
    });
    let response = gateway(&pages)
        .fetch(QWEN, request(), &CancellationToken::new())
        .await
        .expect("responds");
    let mut body = response.body;
    assert_eq!(body.next().await.expect("chunk").expect("data"), b"first");
    let error = body.next().await.expect("an error").expect_err("times out");
    assert_eq!(error.message, TIMED_OUT_MESSAGE);
    assert_eq!(error.details.expect("details")["timeoutSeconds"], 30);
}

#[tokio::test(start_paused = true)]
async fn a_page_that_never_loads_times_out() {
    let pages = FakePages::shared(Plan::Silent);
    pages.hang.store(true, Ordering::SeqCst);
    let error = gateway(&pages)
        .fetch(QWEN, request(), &CancellationToken::new())
        .await
        .expect_err("times out");
    assert_eq!(error.message, TIMED_OUT_MESSAGE);
    assert_eq!(error.details.expect("details")["timeoutSeconds"], 5);
}

#[tokio::test]
async fn a_crashed_page_fails_its_requests_and_is_recreated() {
    let pages = FakePages::shared(Plan::Respond {
        status: 200,
        headers: Vec::new(),
        reads: vec![b"first".to_vec(), b"second".to_vec()],
        stall_after: Some(1),
    });
    let gateway = gateway(&pages);
    let response = gateway
        .fetch(QWEN, request(), &CancellationToken::new())
        .await
        .expect("responds");
    let mut body = response.body;
    assert_eq!(body.next().await.expect("chunk").expect("data"), b"first");
    pages.page(0).close();
    assert_eq!(
        body.next()
            .await
            .expect("an error")
            .expect_err("fails")
            .code,
        AppErrorCode::ProviderUnreachable
    );
    pages.set_plan(respond(200, vec![b"fresh".to_vec()]));
    let response = gateway
        .fetch(QWEN, request(), &CancellationToken::new())
        .await
        .expect("responds");
    assert_eq!(response.text().await.expect("reads"), "fresh");
    assert_eq!(pages.pages().len(), 2);
}

#[tokio::test]
async fn a_page_that_lost_its_context_is_replaced_and_the_request_retried_once() {
    let pages = FakePages::shared(respond(200, vec![b"ok".to_vec()]));
    let gateway = gateway(&pages);
    gateway
        .fetch(QWEN, request(), &CancellationToken::new())
        .await
        .expect("responds");
    *lock(&pages.page(0).plan) = Plan::Refuse;
    let response = gateway
        .fetch(QWEN, request(), &CancellationToken::new())
        .await
        .expect("retried on a new page");
    assert_eq!(response.text().await.expect("reads"), "ok");
    assert_eq!(pages.pages().len(), 2);
    assert!(!pages.page(0).healthy());

    pages.set_plan(Plan::Refuse);
    let error = gateway
        .fetch(QWEN, request(), &CancellationToken::new())
        .await
        .expect_err("gives up");
    assert_eq!(error.code, AppErrorCode::ProviderUnreachable);
    assert_eq!(pages.pages().len(), 3);
}

#[tokio::test]
async fn a_page_that_fails_to_open_is_tried_again_on_the_next_request() {
    let pages = FakePages::shared(respond(200, vec![b"ok".to_vec()]));
    pages.failures.store(1, Ordering::SeqCst);
    let gateway = gateway(&pages);
    assert_eq!(
        gateway
            .fetch(QWEN, request(), &CancellationToken::new())
            .await
            .expect_err("fails")
            .code,
        AppErrorCode::ProviderUnreachable
    );
    assert!(
        gateway
            .fetch(QWEN, request(), &CancellationToken::new())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_forged_or_oversized_chunk_ends_the_stream() {
    let pages = FakePages::shared(Plan::Silent);
    let gateway = gateway(&pages);
    for data in [
        json!("%%%"),
        json!(STANDARD.encode(sample(2048))),
        json!(""),
    ] {
        let cancel = CancellationToken::new();
        let fetch = gateway.fetch(QWEN, request(), &cancel);
        let posting = async {
            loop {
                let started = pages
                    .pages()
                    .first()
                    .map(|page| page.commands("start"))
                    .unwrap_or_default();
                let page = pages.pages().first().cloned();
                if let (Some(page), Some(start)) = (page, started.last())
                    && page.routes.len() == 1
                {
                    let id = start["id"].clone();
                    page.post(&json!({ "id": id, "kind": "head", "status": 200, "headers": [] }));
                    return (page, id);
                }
                tokio::task::yield_now().await;
            }
        };
        let (response, (page, id)) = tokio::join!(fetch, posting);
        let mut body = response.expect("responds").body;
        page.post(&json!({ "id": id, "kind": "chunk", "data": data }));
        assert_eq!(
            body.next()
                .await
                .expect("an error")
                .expect_err("refused")
                .details
                .expect("details")["reason"],
            "gateway-protocol"
        );
        assert!(body.next().await.is_none());
    }
}

#[test]
fn identity_comes_from_the_profile_and_tokens_from_site_tabs() {
    let tokens = BearerTokens::default();
    let gateway = gateway_with(&FakePages::shared(Plan::Silent), tokens.clone());
    assert_eq!(gateway.partition(), BROWSER_PARTITION);
    assert_eq!(gateway.user_agent(), "Mozilla/5.0 Edg/141");
    assert_eq!(gateway.account_token(QWEN), None);
    tokens.observe(&Sighting {
        request_url: "https://chat.qwen.ai/api/v1/auths/".to_owned(),
        page_url: "https://chat.qwen.ai/".to_owned(),
        authorization: Some("Bearer live".to_owned()),
    });
    assert_eq!(
        gateway.account_token(QWEN),
        Some(AccountToken::bearer("live"))
    );
    assert_eq!(gateway.account_token(FIXTURE), None);
}

#[test]
fn the_page_script_is_one_function_expression() {
    assert!(PAGE_SCRIPT.starts_with("function gateway(command) {"));
    assert!(PAGE_SCRIPT.ends_with('}'));
    assert!(!PAGE_SCRIPT.contains("export"));
    assert!(PAGE_SCRIPT.contains(BINDING));
    assert_eq!(
        page_url("http://127.0.0.1:4100").map(String::from),
        Some("http://127.0.0.1:4100/robots.txt".to_owned())
    );
}

#[tokio::test]
async fn qwen_chats_are_created_over_xhr_with_a_challenge_watch() {
    let pages = FakePages::shared(respond(200, vec![b"{}".to_vec()]));
    let gateway = gateway(&pages);
    gateway
        .fetch(QWEN_NEW_CHAT, request(), &CancellationToken::new())
        .await
        .expect("responds");
    gateway
        .fetch(QWEN, request(), &CancellationToken::new())
        .await
        .expect("responds");
    let starts = pages.page(0).commands("start");
    assert_eq!(starts[0]["transport"], "xhr");
    assert_eq!(starts[1]["transport"], "fetch");
    assert_eq!(starts[0]["challenge"], zvs_qwen_web::CHALLENGE_PATTERN);
}

#[tokio::test]
async fn other_origins_use_fetch_without_a_challenge_watch() {
    let pages = FakePages::shared(respond(200, vec![b"{}".to_vec()]));
    gateway(&pages)
        .fetch(FIXTURE, request(), &CancellationToken::new())
        .await
        .expect("responds");
    let start = &pages.page(0).commands("start")[0];
    assert_eq!(start["transport"], "fetch");
    assert_eq!(start["challenge"], Value::Null);
}

#[tokio::test(start_paused = true)]
async fn a_verification_dialog_shows_the_page_until_the_vendor_answers() {
    let pages = FakePages::shared(Plan::ChallengeThen(Box::new(respond(
        200,
        vec![b"ok".to_vec()],
    ))));
    let gateway = Arc::new(gateway(&pages));
    let pending = tokio::spawn({
        let gateway = Arc::clone(&gateway);
        async move {
            gateway
                .fetch(QWEN_NEW_CHAT, request(), &CancellationToken::new())
                .await
        }
    });
    // Longer than the head timeout: a shown verification waits for the person.
    tokio::time::sleep(Duration::from_secs(90)).await;
    let page = pages.page(0);
    assert_eq!(page.revealed.load(Ordering::SeqCst), 1);
    assert_eq!(page.concealed.load(Ordering::SeqCst), 0);
    page.passed.notify_waiters();
    let response = pending.await.expect("joins").expect("responds");
    assert_eq!(response.status, 200);
    assert_eq!(page.concealed.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn an_unanswered_verification_fails_as_browser_verification() {
    let pages = FakePages::shared(Plan::ChallengeThen(Box::new(Plan::Silent)));
    let error = gateway(&pages)
        .fetch(QWEN_NEW_CHAT, request(), &CancellationToken::new())
        .await
        .expect_err("gives up");
    assert_eq!(error.message, BROWSER_VERIFICATION_MESSAGE);
    let page = pages.page(0);
    assert_eq!(page.revealed.load(Ordering::SeqCst), 1);
    assert_eq!(page.concealed.load(Ordering::SeqCst), 1);
    // The abort is sent from a spawned task.
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(page.commands("abort").len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_long_silence_on_a_watched_request_shows_the_page() {
    let pages = FakePages::shared(Plan::Silent);
    let gateway = AccountGateway::with_options(
        pages.clone(),
        BearerTokens::default(),
        GatewayOptions {
            reveal_after: Duration::from_secs(5),
            ..options()
        },
        silent_logger(),
    );
    let error = gateway
        .fetch(QWEN_NEW_CHAT, request(), &CancellationToken::new())
        .await
        .expect_err("gives up");
    assert_eq!(error.message, BROWSER_VERIFICATION_MESSAGE);
    assert_eq!(pages.page(0).revealed.load(Ordering::SeqCst), 1);
    let fixture = gateway
        .fetch(FIXTURE, request(), &CancellationToken::new())
        .await
        .expect_err("times out");
    assert_eq!(fixture.message, TIMED_OUT_MESSAGE);
    assert_eq!(pages.page(1).revealed.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn renewing_asks_the_client_page_to_refresh_its_session_and_reuses_a_recent_answer() {
    let pages = FakePages::shared(respond(200, vec![b"ok".to_vec()]));
    *lock(&pages.renewal) = json!({ "status": 200, "stored": true, "changed": true });
    let gateway = gateway(&pages);
    let cancel = CancellationToken::new();
    assert!(gateway.renew(QWEN, &cancel).await.expect("renews"));
    assert!(gateway.renew(QWEN, &cancel).await.expect("reuses"));
    assert_eq!(pages.pages().len(), 1);
    let renewals = pages.page(0).commands("renew");
    assert_eq!(renewals.len(), 1);
    assert_eq!(
        renewals[0]["session"]["url"],
        "https://auth.qwen.ai/api/v2/auths/refresh"
    );
    assert_eq!(
        renewals[0]["session"]["state_key"],
        "qwen_access_token_state"
    );
    assert!(!renewals[0].to_string().contains("authorization"));
    assert!(
        renewals[0]["session"]["headers"]
            .as_array()
            .expect("headers")
            .contains(&json!(["x-request-origin", "chat.qwen.ai"]))
    );
    assert!(pages.page(0).healthy());

    tokio::time::sleep(Duration::from_secs(6)).await;
    *lock(&pages.renewal) = json!({ "status": 401, "stored": false, "changed": false });
    assert!(!gateway.renew(QWEN, &cancel).await.expect("answers"));
    assert_eq!(pages.page(0).commands("renew").len(), 2);
    assert_eq!(pages.pages().len(), 1);
}

#[tokio::test]
async fn origins_without_a_client_session_refresh_are_not_renewed() {
    let pages = FakePages::shared(respond(200, vec![b"ok".to_vec()]));
    let gateway = gateway(&pages);
    let cancel = CancellationToken::new();
    assert!(!gateway.renew(FIXTURE, &cancel).await.expect("answers"));
    assert!(pages.pages().is_empty());
    assert!(
        gateway
            .renew("https://evil.example/", &cancel)
            .await
            .is_err()
    );
}
