pub mod routes;
mod webview;

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures::{StreamExt, stream};
use serde_json::{Value, json};
use tokio::sync::{OwnedMutexGuard, mpsc};
use tokio_util::sync::CancellationToken;
use url::Url;
use zvs_contract::{AppError, AppErrorCode};
use zvs_host::browser::BROWSER_PARTITION;
use zvs_host::drivers::ai::errors::{
    Headers, UNREACHABLE_MESSAGE, browser_verification, cancelled, request_failed, session_expired,
    timed_out,
};
use zvs_host::drivers::ai::{
    AccountToken, ByteStream, SessionGateway, SessionRequest, SessionResponse,
};
use zvs_host::platform::ids::new_id;
use zvs_host::platform::logger::{LogLevel, SharedLogger, silent_logger};
use zvs_qwen_web::{FrontendVersion, PagePlan, PageTransport};

use self::routes::{PageMessage, Routes, SharedRoutes};
use crate::browser::bearer::{BearerTokens, account_origin, origin_of};

pub use self::webview::WebviewPages;

pub const PAGE_PATH: &str = "/robots.txt";
pub const BINDING: &str = "__zvsGatewaySend";
pub const WORLD: &str = "zvs-account-gateway";

pub static PAGE_SCRIPT: LazyLock<&'static str> = LazyLock::new(|| {
    let source = include_str!("page.js").trim();
    source.strip_prefix("export default ").unwrap_or(source)
});

const FORBIDDEN_HEADERS: [&str; 21] = [
    "accept-charset",
    "accept-encoding",
    "access-control-request-headers",
    "access-control-request-method",
    "connection",
    "content-length",
    "cookie",
    "cookie2",
    "date",
    "dnt",
    "expect",
    "host",
    "keep-alive",
    "origin",
    "referer",
    "set-cookie",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "user-agent",
];
const HIDDEN_RESPONSE_HEADERS: [&str; 2] = ["set-cookie", "set-cookie2"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatewayOptions {
    pub ready_timeout: Duration,
    pub head_timeout: Duration,
    pub idle_timeout: Duration,
    pub chunk_size: usize,
    pub window: u32,
    pub ack_every: u32,
    /// A watched request still without a response after this long is shown to the user,
    /// since the vendor may be waiting for a verification nobody can see.
    pub reveal_after: Duration,
    pub stream_reveal_after: Duration,
    /// How long a shown verification may take before the request fails.
    pub challenge_timeout: Duration,
    pub renew_reuse: Duration,
    pub pace_gap: Duration,
}

impl Default for GatewayOptions {
    fn default() -> Self {
        Self {
            ready_timeout: Duration::from_secs(30),
            head_timeout: Duration::from_secs(120),
            idle_timeout: Duration::from_secs(300),
            chunk_size: 32 * 1024,
            window: 8,
            ack_every: 4,
            reveal_after: Duration::from_secs(10),
            stream_reveal_after: Duration::from_secs(30),
            challenge_timeout: Duration::from_secs(300),
            renew_reuse: Duration::from_secs(5),
            pace_gap: Duration::from_millis(1500),
        }
    }
}

#[async_trait]
pub trait GatewayPage: Send + Sync {
    async fn command(&self, command: Value) -> Result<Value, String>;

    fn healthy(&self) -> bool;

    /// Show the page so a person can complete a verification the vendor asks for.
    fn reveal(&self) {}

    /// Hide the page again once the request it was shown for has an answer.
    fn conceal(&self) {}

    fn close(&self);
}

pub type SharedPage = Arc<dyn GatewayPage>;

#[async_trait]
pub trait PageFactory: Send + Sync {
    async fn open(
        &self,
        origin: &str,
        page: &Url,
        routes: SharedRoutes,
    ) -> Result<SharedPage, String>;

    fn user_agent(&self) -> Option<String>;

    fn dispose(&self) {}
}

#[derive(Clone)]
struct Live {
    page: SharedPage,
    routes: SharedRoutes,
}

type Slot = Arc<tokio::sync::Mutex<Option<Live>>>;
type Pace = Arc<tokio::sync::Mutex<Option<tokio::time::Instant>>>;

struct Turn(OwnedMutexGuard<Option<tokio::time::Instant>>);

impl Drop for Turn {
    fn drop(&mut self) {
        *self.0 = Some(tokio::time::Instant::now());
    }
}
type Renewal = Arc<tokio::sync::Mutex<Option<(tokio::time::Instant, Result<bool, AppError>)>>>;

pub struct AccountGateway {
    factory: Arc<dyn PageFactory>,
    tokens: BearerTokens,
    options: GatewayOptions,
    logger: SharedLogger,
    slots: Mutex<HashMap<String, Slot>>,
    renewals: Mutex<HashMap<String, Renewal>>,
    paces: Mutex<HashMap<String, Pace>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn unreachable() -> AppError {
    AppError::new(AppErrorCode::ProviderUnreachable, UNREACHABLE_MESSAGE)
}

fn refused_origin() -> AppError {
    request_failed().with_detail("reason", "origin-not-allowed")
}

fn protocol_violation() -> AppError {
    request_failed().with_detail("reason", "gateway-protocol")
}

fn seconds(duration: Duration) -> f64 {
    duration.as_secs_f64()
}

pub const APPLICATION_ORIGINS: [&str; 2] = [
    zvs_qwen_web::protocol::ORIGIN,
    zvs_zai_web::protocol::ORIGIN,
];

pub fn application_page(page: &Url) -> bool {
    page.path() == "/"
        && page.query().is_none()
        && APPLICATION_ORIGINS.contains(&page.origin().ascii_serialization().as_str())
}

pub fn page_url(origin: &str) -> Option<Url> {
    let origin = Url::parse(origin).ok()?;
    let path = if APPLICATION_ORIGINS.contains(&origin.origin().ascii_serialization().as_str()) {
        "/"
    } else {
        PAGE_PATH
    };
    origin.join(path).ok()
}

/// Qwen requests go out the way its client sends them; other origins use plain fetch.
fn page_plan(origin: &str, url: &str) -> PagePlan {
    if origin == zvs_qwen_web::protocol::ORIGIN {
        zvs_qwen_web::page_plan(url)
    } else if origin == zvs_zai_web::protocol::ORIGIN {
        PagePlan {
            transport: PageTransport::Fetch,
            challenge_pattern: Some(zvs_zai_web::CHALLENGE_PATTERN),
            page_bearer_key: Some(zvs_zai_web::PAGE_BEARER_KEY),
            session_refresh: None,
            frontend_version: Some(FrontendVersion {
                header: zvs_zai_web::FE_VERSION_HEADER,
                script_pattern: zvs_zai_web::protocol::FE_VERSION_PATTERN,
            }),
            paced: zvs_zai_web::is_turn(url),
            timezone_header: false,
        }
    } else {
        PagePlan {
            transport: PageTransport::Fetch,
            challenge_pattern: None,
            page_bearer_key: None,
            session_refresh: None,
            frontend_version: None,
            paced: false,
            timezone_header: false,
        }
    }
}

fn captcha_command(origin: &str, url: &str) -> Value {
    if origin != zvs_zai_web::protocol::ORIGIN || !zvs_zai_web::needs_captcha(url) {
        return Value::Null;
    }
    let captcha = zvs_zai_web::CAPTCHA;
    json!({
        "script_url": captcha.script_url,
        "region": captcha.region,
        "prefix": captcha.prefix,
        "scene_id": captcha.scene_id,
        "mode": captcha.mode,
        "element_id": captcha.element_id,
        "button_id": captcha.button_id,
        "timeout_ms": captcha.timeout_ms,
        "field": captcha.field,
    })
}

fn bearer_query(origin: &str) -> Option<&'static str> {
    (origin == zvs_zai_web::protocol::ORIGIN).then_some(zvs_zai_web::BEARER_QUERY)
}

fn session_revoked(answer: &Value) -> bool {
    answer["success"] == false
        && answer["code"].as_str().is_some_and(|code| {
            zvs_qwen_web::SESSION_REFUSALS.contains(&code.to_ascii_lowercase().as_str())
        })
}

fn session_command(plan: PagePlan) -> Value {
    let (Some(key), Some(refresh)) = (plan.page_bearer_key, plan.session_refresh) else {
        return Value::Null;
    };
    json!({
        "url": refresh.url,
        "key": key,
        "expiry_key": refresh.expiry_key,
        "state_key": refresh.state_key,
        "channel": refresh.channel,
        "leeway_ms": refresh.leeway_ms,
        "headers": [
            ["accept", "application/json, text/plain, */*"],
            ["source", zvs_qwen_web::protocol::SOURCE],
            ["version", zvs_qwen_web::protocol::WEB_VERSION],
            ["x-request-id", new_id().to_string()],
            ["x-request-origin", zvs_qwen_web::protocol::REQUEST_ORIGIN],
        ],
    })
}

fn page_headers(request: &SessionRequest) -> Vec<(String, String)> {
    request
        .headers
        .iter()
        .filter(|(name, _)| {
            let name = name.to_ascii_lowercase();
            !FORBIDDEN_HEADERS.contains(&name.as_str())
                && !name.starts_with("proxy-")
                && !name.starts_with("sec-")
        })
        .cloned()
        .collect()
}

fn referrer(request: &SessionRequest, origin: &str) -> Option<String> {
    let referer = request.header("referer")?;
    (origin_of(referer).as_deref() == Some(origin)).then(|| referer.to_owned())
}

fn response_headers(pairs: Vec<(String, String)>) -> Headers {
    pairs
        .into_iter()
        .map(|(name, value)| (name.to_ascii_lowercase(), value))
        .filter(|(name, _)| !HIDDEN_RESPONSE_HEADERS.contains(&name.as_str()))
        .collect()
}

fn abort(page: &SharedPage, id: &str) {
    let page = Arc::clone(page);
    let command = json!({ "op": "abort", "id": id });
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        runtime.spawn(async move {
            let _ = page.command(command).await;
        });
    }
}

struct Exchange {
    page: SharedPage,
    routes: SharedRoutes,
    id: String,
    armed: bool,
    turn: Option<Turn>,
}

impl Exchange {
    fn abort(&mut self) {
        self.turn = None;
        if self.armed {
            self.armed = false;
            self.routes.unregister(&self.id);
            abort(&self.page, &self.id);
        }
    }

    fn settle(&mut self) {
        self.turn = None;
        self.armed = false;
        self.routes.unregister(&self.id);
    }
}

impl Drop for Exchange {
    fn drop(&mut self) {
        self.abort();
    }
}

struct Body {
    exchange: Exchange,
    receiver: mpsc::UnboundedReceiver<PageMessage>,
    cancel: CancellationToken,
    options: GatewayOptions,
    consumed: u32,
    finished: bool,
}

impl Body {
    fn fail(&mut self, error: AppError) -> Option<Result<Vec<u8>, AppError>> {
        self.finished = true;
        self.exchange.abort();
        Some(Err(error))
    }

    fn acknowledge(&mut self) {
        self.consumed += 1;
        if !self.consumed.is_multiple_of(self.options.ack_every.max(1)) {
            return;
        }
        let page = Arc::clone(&self.exchange.page);
        let command =
            json!({ "op": "ack", "id": self.exchange.id, "credit": self.options.ack_every.max(1) });
        tokio::spawn(async move {
            let _ = page.command(command).await;
        });
    }

    async fn next(&mut self) -> Option<Result<Vec<u8>, AppError>> {
        if self.finished {
            return None;
        }
        let message = loop {
            let message = tokio::select! {
                biased;
                () = self.cancel.cancelled() => return self.fail(cancelled()),
                () = tokio::time::sleep(self.options.idle_timeout) => {
                    return self.fail(timed_out(seconds(self.options.idle_timeout)));
                }
                message = self.receiver.recv() => message,
            };
            // A dialog that appears once the response is streaming does not hold it up.
            if message != Some(PageMessage::Challenge) {
                break message;
            }
        };
        match message {
            Some(PageMessage::Chunk { data }) => match STANDARD.decode(data.as_bytes()) {
                Ok(bytes) if bytes.len() <= self.options.chunk_size && !bytes.is_empty() => {
                    self.acknowledge();
                    Some(Ok(bytes))
                }
                _ => self.fail(protocol_violation()),
            },
            Some(PageMessage::End) => {
                self.finished = true;
                self.exchange.settle();
                None
            }
            Some(PageMessage::Error { .. }) => {
                self.exchange.settle();
                self.fail(unreachable())
            }
            Some(PageMessage::Head { .. } | PageMessage::Challenge) => {
                self.fail(protocol_violation())
            }
            None => {
                self.exchange.settle();
                self.fail(unreachable())
            }
        }
    }
}

impl AccountGateway {
    pub fn new(factory: Arc<dyn PageFactory>, tokens: BearerTokens) -> Self {
        Self::with_options(factory, tokens, GatewayOptions::default(), silent_logger())
    }

    pub fn with_options(
        factory: Arc<dyn PageFactory>,
        tokens: BearerTokens,
        options: GatewayOptions,
        logger: SharedLogger,
    ) -> Self {
        Self {
            factory,
            tokens,
            options,
            logger,
            slots: Mutex::default(),
            renewals: Mutex::default(),
            paces: Mutex::default(),
        }
    }

    pub async fn warm(&self, origin: &str) -> Result<(), AppError> {
        let origin = account_origin(origin).ok_or_else(refused_origin)?;
        let page = page_url(&origin).ok_or_else(refused_origin)?;
        self.live(&origin, &page).await.map(|_| ())
    }

    async fn pace(&self, origin: &str, cancel: &CancellationToken) -> Result<Turn, AppError> {
        let pace = Arc::clone(lock(&self.paces).entry(origin.to_owned()).or_default());
        let guard = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(cancelled()),
            guard = pace.lock_owned() => guard,
        };
        if let Some(last) = *guard {
            tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(cancelled()),
                () = tokio::time::sleep_until(last + self.options.pace_gap) => {}
            }
        }
        Ok(Turn(guard))
    }

    pub fn dispose(&self) {
        let slots = std::mem::take(&mut *lock(&self.slots));
        for slot in slots.into_values() {
            if let Ok(mut live) = slot.try_lock()
                && let Some(live) = live.take()
            {
                live.routes.fail_all();
                live.page.close();
            }
        }
        self.factory.dispose();
    }

    async fn live(&self, origin: &str, page: &Url) -> Result<Live, AppError> {
        let slot = Arc::clone(lock(&self.slots).entry(origin.to_owned()).or_default());
        let mut current = slot.lock().await;
        if let Some(live) = current.as_ref().filter(|live| live.page.healthy()) {
            return Ok(live.clone());
        }
        if let Some(stale) = current.take() {
            stale.routes.fail_all();
            stale.page.close();
            self.logger.log(
                LogLevel::Info,
                "browser",
                "Recreating the account gateway",
                &[("origin", json!(origin))],
            );
        }
        let routes = Routes::shared();
        let opened = tokio::time::timeout(
            self.options.ready_timeout,
            self.factory.open(origin, page, Arc::clone(&routes)),
        )
        .await;
        let page = match opened {
            Ok(Ok(page)) => page,
            Ok(Err(error)) => {
                self.logger.log(
                    LogLevel::Warn,
                    "browser",
                    "Could not open the account gateway",
                    &[("origin", json!(origin)), ("error", json!(error))],
                );
                return Err(unreachable());
            }
            Err(_) => {
                self.logger.log(
                    LogLevel::Warn,
                    "browser",
                    "The account gateway did not become ready",
                    &[("origin", json!(origin))],
                );
                return Err(timed_out(seconds(self.options.ready_timeout)));
            }
        };
        let live = Live { page, routes };
        *current = Some(live.clone());
        Ok(live)
    }

    async fn discard(&self, origin: &str, broken: &Live) {
        let slot = lock(&self.slots).get(origin).cloned();
        let Some(slot) = slot else {
            return;
        };
        let mut current = slot.lock().await;
        if current
            .as_ref()
            .is_some_and(|live| Arc::ptr_eq(&live.page, &broken.page))
        {
            *current = None;
        }
        broken.routes.fail_all();
        broken.page.close();
    }

    async fn start(
        &self,
        origin: &str,
        url: &str,
        request: &SessionRequest,
        plan: PagePlan,
        mut turn: Option<Turn>,
        cancel: &CancellationToken,
    ) -> Result<(Exchange, mpsc::UnboundedReceiver<PageMessage>), AppError> {
        let page = &page_url(origin).ok_or_else(refused_origin)?;
        let mut attempts = 0;
        loop {
            attempts += 1;
            let live = tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(cancelled()),
                live = self.live(origin, page) => live?,
            };
            let id = new_id().to_string();
            let Some(receiver) = live.routes.register(&id) else {
                self.discard(origin, &live).await;
                if attempts < 2 {
                    continue;
                }
                return Err(unreachable());
            };
            let exchange = Exchange {
                page: Arc::clone(&live.page),
                routes: Arc::clone(&live.routes),
                id: id.clone(),
                armed: true,
                turn: turn.take(),
            };
            let command = json!({
                "op": "start",
                "id": id,
                "url": url,
                "method": request.method,
                "headers": page_headers(request),
                "body": request.body,
                "referrer": referrer(request, origin),
                "page": page.as_str(),
                "app": application_page(page),
                "chunk": self.options.chunk_size,
                "window": self.options.window,
                "transport": plan.transport.as_str(),
                "challenge": plan.challenge_pattern,
                "bearer_key": plan.page_bearer_key,
                "session": session_command(plan),
                "version": plan.frontend_version.map(|version| json!({
                    "header": version.header,
                    "pattern": version.script_pattern,
                })),
                "timezone": plan.timezone_header,
                "captcha": captcha_command(origin, url),
                "bearer_query": bearer_query(origin),
            });
            let started = tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(cancelled()),
                started = live.page.command(command) => started,
            };
            match started {
                Ok(Value::Bool(true)) => return Ok((exchange, receiver)),
                Ok(_) | Err(_) => {
                    let mut exchange = exchange;
                    turn = exchange.turn.take();
                    exchange.settle();
                    self.logger.log(
                        LogLevel::Warn,
                        "browser",
                        "The account gateway refused a request",
                        &[("origin", json!(origin)), ("error", json!(started.err()))],
                    );
                    self.discard(origin, &live).await;
                    if attempts < 2 {
                        continue;
                    }
                    return Err(unreachable());
                }
            }
        }
    }
}

type Head = (u16, Vec<(String, String)>, Option<String>, bool);

impl AccountGateway {
    /// Wait for the response head. While a watched request waits, a verification dialog (or a
    /// silence long enough to suggest one) shows the page to the user; the request goes on once
    /// the vendor accepts it, and the page is hidden again.
    async fn head(
        &self,
        origin: &str,
        exchange: &mut Exchange,
        plan: PagePlan,
        receiver: &mut mpsc::UnboundedReceiver<PageMessage>,
        cancel: &CancellationToken,
    ) -> Result<Head, AppError> {
        let watched = plan.challenge_pattern.is_some();
        let started = tokio::time::Instant::now();
        let silent_after = match plan.transport {
            PageTransport::Xhr => self.options.reveal_after,
            PageTransport::Fetch => self.options.stream_reveal_after,
        };
        let reveal_at = started + silent_after;
        let mut deadline = started + self.options.head_timeout;
        let mut revealed = false;
        let outcome = loop {
            let reveal = async {
                if watched && !revealed {
                    tokio::time::sleep_until(reveal_at).await;
                } else {
                    std::future::pending::<()>().await;
                }
            };
            let message = tokio::select! {
                biased;
                () = cancel.cancelled() => break Err(cancelled()),
                () = tokio::time::sleep_until(deadline) => {
                    break Err(if revealed {
                        let mut details = zvs_contract::ErrorDetails::new();
                        details.insert("reason".to_owned(), json!("browser-verification"));
                        details.insert("origin".to_owned(), json!(origin));
                        browser_verification(details)
                    } else {
                        timed_out(seconds(self.options.head_timeout))
                    });
                }
                () = reveal => None,
                message = receiver.recv() => Some(message),
            };
            match message {
                None | Some(Some(PageMessage::Challenge)) => {
                    if !revealed {
                        revealed = true;
                        deadline = tokio::time::Instant::now() + self.options.challenge_timeout;
                        exchange.page.reveal();
                        self.logger.log(
                            LogLevel::Warn,
                            "browser",
                            "The account page is shown for a verification",
                            &[
                                ("origin", json!(origin)),
                                ("dialog", json!(message.is_some())),
                            ],
                        );
                    }
                }
                Some(Some(PageMessage::Head {
                    status,
                    headers,
                    final_url,
                    redirected,
                })) => break Ok((status, headers, final_url, redirected)),
                Some(Some(PageMessage::Error { .. }) | None) => {
                    exchange.settle();
                    break Err(unreachable());
                }
                Some(Some(PageMessage::Chunk { .. } | PageMessage::End)) => {
                    break Err(protocol_violation());
                }
            }
        };
        if revealed {
            exchange.page.conceal();
        }
        outcome
    }
}

#[async_trait]
impl SessionGateway for AccountGateway {
    fn partition(&self) -> &str {
        BROWSER_PARTITION
    }

    fn user_agent(&self) -> String {
        self.factory.user_agent().unwrap_or_default()
    }

    fn account_token(&self, url: &str) -> Option<AccountToken> {
        self.tokens.token(url)
    }

    async fn renew(&self, url: &str, cancel: &CancellationToken) -> Result<bool, AppError> {
        let origin = account_origin(url).ok_or_else(refused_origin)?;
        let plan = page_plan(&origin, url);
        let session = session_command(plan);
        if session.is_null() {
            return Ok(false);
        }
        let page = page_url(&origin).ok_or_else(refused_origin)?;
        let renewal = Arc::clone(lock(&self.renewals).entry(origin.clone()).or_default());
        let mut renewed = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(cancelled()),
            guard = renewal.lock() => guard,
        };
        if let Some((at, outcome)) = renewed.as_ref()
            && at.elapsed() < self.options.renew_reuse
        {
            return outcome.clone();
        }
        let live = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(cancelled()),
            live = self.live(&origin, &page) => live?,
        };
        let command = json!({ "op": "renew", "session": session });
        let answer = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(cancelled()),
            answer = live.page.command(command) => answer,
        };
        let answer = answer.unwrap_or(Value::Null);
        let renewed_now = answer["stored"] == true || answer["changed"] == true;
        let outcome = if session_revoked(&answer) {
            let mut details = zvs_contract::ErrorDetails::new();
            details.insert("origin".to_owned(), json!(origin));
            details.insert("reason".to_owned(), json!("refresh-revoked"));
            details.insert("vendorCode".to_owned(), answer["code"].clone());
            Err(session_expired(details))
        } else {
            Ok(renewed_now)
        };
        *renewed = Some((tokio::time::Instant::now(), outcome.clone()));
        self.logger.log(
            if renewed_now {
                LogLevel::Info
            } else {
                LogLevel::Warn
            },
            "browser",
            "The account client was asked to renew its session",
            &[
                ("origin", json!(origin)),
                ("status", answer["status"].clone()),
                ("success", answer["success"].clone()),
                ("vendorCode", answer["code"].clone()),
                ("details", answer["details"].clone()),
                ("fields", answer["fields"].clone()),
                ("stored", answer["stored"].clone()),
                ("bearerChanged", answer["changed"].clone()),
            ],
        );
        outcome
    }

    async fn fetch(
        &self,
        url: &str,
        request: SessionRequest,
        cancel: &CancellationToken,
    ) -> Result<SessionResponse, AppError> {
        let origin = account_origin(url).ok_or_else(refused_origin)?;
        page_url(&origin).ok_or_else(refused_origin)?;
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let plan = page_plan(&origin, url);
        let turn = if plan.paced {
            Some(self.pace(&origin, cancel).await?)
        } else {
            None
        };
        let (mut exchange, mut receiver) = self
            .start(&origin, url, &request, plan, turn, cancel)
            .await?;
        let (status, headers, final_url, redirected) = match self
            .head(&origin, &mut exchange, plan, &mut receiver, cancel)
            .await
        {
            Ok(head) => head,
            Err(error) => {
                exchange.abort();
                return Err(error);
            }
        };
        let body = Body {
            exchange,
            receiver,
            cancel: cancel.clone(),
            options: self.options,
            consumed: 0,
            finished: false,
        };
        let body: ByteStream = Box::pin(
            stream::unfold(body, |mut body| async move {
                body.next().await.map(|item| (item, body))
            })
            .fuse(),
        );
        Ok(
            SessionResponse::new(status, response_headers(headers), body)
                .with_response_url(final_url, redirected),
        )
    }
}

#[cfg(test)]
mod tests;
