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
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use url::Url;
use zvs_contract::{AppError, AppErrorCode};
use zvs_host::browser::BROWSER_PARTITION;
use zvs_host::drivers::ai::errors::{
    Headers, UNREACHABLE_MESSAGE, cancelled, request_failed, timed_out,
};
use zvs_host::drivers::ai::{
    AccountToken, ByteStream, SessionGateway, SessionRequest, SessionResponse,
};
use zvs_host::platform::ids::new_id;
use zvs_host::platform::logger::{LogLevel, SharedLogger, silent_logger};

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
        }
    }
}

#[async_trait]
pub trait GatewayPage: Send + Sync {
    async fn command(&self, command: Value) -> Result<Value, String>;

    fn healthy(&self) -> bool;

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

pub struct AccountGateway {
    factory: Arc<dyn PageFactory>,
    tokens: BearerTokens,
    options: GatewayOptions,
    logger: SharedLogger,
    slots: Mutex<HashMap<String, Slot>>,
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

pub fn page_url(origin: &str) -> Option<Url> {
    Url::parse(origin).ok()?.join(PAGE_PATH).ok()
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
}

impl Exchange {
    fn abort(&mut self) {
        if self.armed {
            self.armed = false;
            self.routes.unregister(&self.id);
            abort(&self.page, &self.id);
        }
    }

    fn settle(&mut self) {
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
        let message = tokio::select! {
            biased;
            () = self.cancel.cancelled() => return self.fail(cancelled()),
            () = tokio::time::sleep(self.options.idle_timeout) => {
                return self.fail(timed_out(seconds(self.options.idle_timeout)));
            }
            message = self.receiver.recv() => message,
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
            Some(PageMessage::Head { .. }) => self.fail(protocol_violation()),
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
        }
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
        page: &Url,
        url: &str,
        request: &SessionRequest,
        cancel: &CancellationToken,
    ) -> Result<(Exchange, mpsc::UnboundedReceiver<PageMessage>), AppError> {
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
                "chunk": self.options.chunk_size,
                "window": self.options.window,
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

    async fn fetch(
        &self,
        url: &str,
        request: SessionRequest,
        cancel: &CancellationToken,
    ) -> Result<SessionResponse, AppError> {
        let origin = account_origin(url).ok_or_else(refused_origin)?;
        let page = page_url(&origin).ok_or_else(refused_origin)?;
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let (mut exchange, mut receiver) =
            self.start(&origin, &page, url, &request, cancel).await?;
        let head = tokio::select! {
            biased;
            () = cancel.cancelled() => Err(cancelled()),
            () = tokio::time::sleep(self.options.head_timeout) => {
                Err(timed_out(seconds(self.options.head_timeout)))
            }
            message = receiver.recv() => match message {
                Some(PageMessage::Head { status, headers }) => Ok((status, headers)),
                Some(PageMessage::Error { .. }) | None => {
                    exchange.settle();
                    Err(unreachable())
                }
                Some(PageMessage::Chunk { .. } | PageMessage::End) => Err(protocol_violation()),
            },
        };
        let (status, headers) = match head {
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
        Ok(SessionResponse::new(
            status,
            response_headers(headers),
            body,
        ))
    }
}

#[cfg(test)]
mod tests;
