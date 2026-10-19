use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{Value, json};
use tokio::time::{Instant, sleep_until};
use tokio_util::sync::{CancellationToken, DropGuard};
use url::Url;
use zvs_contract::{AppError, AuthMode};

use super::{
    ByteStream, RequestSpec, SessionRequest, SessionResponse, SharedSessionGateway, Transport,
    TransportResponse, join_url, normalise_timeout,
};
use crate::drivers::ai::errors::{
    HttpFailure, account_page_error, auth_envelope_code, cancelled, html_title, http_failure,
    is_html, request_failed, session_expired, timed_out,
};
use crate::drivers::ai::transport::AccountToken;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};

const FAR_FUTURE: Duration = Duration::from_secs(60 * 60 * 24 * 365 * 30);

#[async_trait]
pub trait AccountCredentials: Send + Sync {
    async fn current(&self, cancel: &CancellationToken) -> Result<Option<AccountToken>, AppError>;

    async fn refresh(&self, cancel: &CancellationToken) -> Result<Option<AccountToken>, AppError>;
}

pub type SharedAccountCredentials = Arc<dyn AccountCredentials>;

pub struct AccountTransportOptions {
    pub base_url: String,
    pub session: SharedSessionGateway,
    pub credentials: Option<SharedAccountCredentials>,
    pub timeout_seconds: Option<f64>,
    pub headers: Vec<(String, String)>,
    pub logger: SharedLogger,
}

impl AccountTransportOptions {
    pub fn new(base_url: impl Into<String>, session: SharedSessionGateway) -> Self {
        Self {
            base_url: base_url.into(),
            session,
            credentials: None,
            timeout_seconds: None,
            headers: Vec::new(),
            logger: silent_logger(),
        }
    }
}

pub struct AccountTransport {
    base_url: String,
    session: SharedSessionGateway,
    credentials: Option<SharedAccountCredentials>,
    timeout_seconds: f64,
    headers: Vec<(String, String)>,
    origin: String,
    logger: SharedLogger,
}

impl fmt::Debug for AccountTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AccountTransport")
            .field("base_url", &self.base_url)
            .field("partition", &self.session.partition())
            .field("credentials", &self.credentials.is_some())
            .field("timeout_seconds", &self.timeout_seconds)
            .finish_non_exhaustive()
    }
}

struct Attempt {
    response: SessionResponse,
    deadline: Instant,
    authorised: bool,
    scope: DropGuard,
}

#[derive(Clone, Copy)]
enum TokenMode {
    Current,
    Refresh,
}

struct Pump {
    body: ByteStream,
    cancel: CancellationToken,
    deadline: Instant,
    timeout_seconds: f64,
    finished: bool,
    _scope: DropGuard,
}

fn origin_of(base_url: &str) -> String {
    Url::parse(base_url)
        .map(|url| url.origin().ascii_serialization())
        .unwrap_or_default()
}

fn set_header(headers: &mut Vec<(String, String)>, name: &str, value: impl Into<String>) {
    let name = name.to_ascii_lowercase();
    let value = value.into();
    if let Some(existing) = headers.iter_mut().find(|(key, _)| *key == name) {
        existing.1 = value;
    } else {
        headers.push((name, value));
    }
}

impl AccountTransport {
    pub fn new(options: AccountTransportOptions) -> Self {
        Self {
            origin: origin_of(&options.base_url),
            base_url: options.base_url,
            session: options.session,
            credentials: options.credentials,
            timeout_seconds: normalise_timeout(options.timeout_seconds),
            headers: options.headers,
            logger: options.logger,
        }
    }

    pub const fn timeout_seconds(&self) -> f64 {
        self.timeout_seconds
    }

    fn deadline(&self) -> Instant {
        let now = Instant::now();
        Duration::try_from_secs_f64(self.timeout_seconds)
            .ok()
            .and_then(|timeout| now.checked_add(timeout))
            .unwrap_or_else(|| now + FAR_FUTURE)
    }

    fn abort_reason(&self, cancel: &CancellationToken) -> AppError {
        if cancel.is_cancelled() {
            cancelled()
        } else {
            timed_out(self.timeout_seconds)
        }
    }

    async fn token(
        &self,
        cancel: &CancellationToken,
        mode: TokenMode,
    ) -> Result<Option<AccountToken>, AppError> {
        let Some(credentials) = &self.credentials else {
            return Ok(None);
        };
        match mode {
            TokenMode::Current => credentials.current(cancel).await,
            TokenMode::Refresh => credentials.refresh(cancel).await,
        }
    }

    async fn attempt(
        &self,
        spec: &RequestSpec,
        accept: &str,
        cancel: &CancellationToken,
    ) -> Result<Attempt, AppError> {
        let current = self.token(cancel, TokenMode::Current).await?;
        let first = self.send(spec, accept, cancel, current).await?;
        if !first.authorised || !matches!(first.response.status, 401 | 403) {
            return Ok(first);
        }
        let status = first.response.status;
        drop(first);
        let Some(refreshed) = self.token(cancel, TokenMode::Refresh).await? else {
            let mut details = zvs_contract::ErrorDetails::new();
            details.insert("status".to_owned(), json!(status));
            return Err(session_expired(details));
        };
        self.send(spec, accept, cancel, Some(refreshed)).await
    }

    fn request_for(
        &self,
        spec: &RequestSpec,
        accept: &str,
        token: Option<&AccountToken>,
    ) -> Result<SessionRequest, AppError> {
        let mut headers = Vec::new();
        set_header(
            &mut headers,
            "accept",
            spec.accept.as_deref().unwrap_or(accept),
        );
        set_header(&mut headers, "user-agent", self.session.user_agent());
        set_header(&mut headers, "origin", self.origin.clone());
        set_header(&mut headers, "referer", format!("{}/", self.origin));
        for (name, value) in self.headers.iter().chain(&spec.headers) {
            set_header(&mut headers, name, value.clone());
        }
        if let Some(token) = token {
            set_header(&mut headers, "authorization", token.authorization());
        }
        if spec.body.is_some() {
            set_header(&mut headers, "content-type", "application/json");
        }
        let body = spec
            .body
            .as_ref()
            .map(|body| serde_json::to_string(body).map_err(|_| request_failed()))
            .transpose()?;
        Ok(SessionRequest {
            method: spec.method.as_str().to_owned(),
            headers,
            body,
        })
    }

    async fn send(
        &self,
        spec: &RequestSpec,
        accept: &str,
        cancel: &CancellationToken,
        token: Option<AccountToken>,
    ) -> Result<Attempt, AppError> {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let url = join_url(&self.base_url, &spec.path, &spec.query)?;
        let request = self.request_for(spec, accept, token.as_ref())?;
        let deadline = self.deadline();
        let scope = cancel.child_token();
        let outcome = tokio::select! {
            biased;
            () = cancel.cancelled() => Err(cancelled()),
            () = sleep_until(deadline) => Err(self.abort_reason(cancel)),
            fetched = self.session.fetch(url.as_str(), request, &scope) => fetched.map_err(|error| {
                if cancel.is_cancelled() {
                    cancelled()
                } else {
                    error
                }
            }),
        };
        let guard = scope.drop_guard();
        let mut response = outcome?;
        if is_html(&response.headers) || (300..400).contains(&response.status) {
            let mut prefix = Vec::new();
            while prefix.len() < 32 * 1024 {
                let next = tokio::select! {
                    biased;
                    () = cancel.cancelled() => return Err(cancelled()),
                    () = sleep_until(deadline) => return Err(self.abort_reason(cancel)),
                    chunk = response.body.next() => chunk,
                };
                let Some(chunk) = next else { break };
                let chunk = chunk?;
                let remaining = 32 * 1024 - prefix.len();
                prefix.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
            }
            let mut details = zvs_contract::ErrorDetails::new();
            details.insert("path".to_owned(), json!(url.path()));
            let page = String::from_utf8_lossy(&prefix);
            let title = html_title(&page);
            if let Some(title) = &title {
                details.insert("htmlTitle".to_owned(), json!(title));
            }
            if let Some(error) = account_page_error(
                response.status,
                &response.headers,
                &page,
                response.final_url.as_deref(),
                response.redirected,
                details,
            ) {
                self.logger.log(
                    LogLevel::Warn,
                    "ai",
                    "The account endpoint returned a page instead of an API response",
                    &[
                        ("status", json!(response.status)),
                        ("origin", json!(self.origin)),
                        ("path", json!(url.path())),
                        ("redirected", json!(response.redirected)),
                        (
                            "contentType",
                            json!(
                                response
                                    .headers
                                    .get("content-type")
                                    .cloned()
                                    .unwrap_or_default()
                            ),
                        ),
                        ("htmlTitle", json!(title)),
                        (
                            "finalPath",
                            error
                                .details
                                .as_ref()
                                .and_then(|details| details.get("finalPath"))
                                .cloned()
                                .unwrap_or(Value::Null),
                        ),
                        (
                            "reason",
                            error
                                .details
                                .as_ref()
                                .and_then(|details| details.get("reason"))
                                .cloned()
                                .unwrap_or(Value::Null),
                        ),
                    ],
                );
                return Err(error);
            }
        }
        Ok(Attempt {
            response,
            deadline,
            authorised: token.is_some(),
            scope: guard,
        })
    }

    async fn read(&self, attempt: Attempt, cancel: &CancellationToken) -> Result<String, AppError> {
        let status = attempt.response.status;
        let deadline = attempt.deadline;
        let _scope = attempt.scope;
        tokio::select! {
            biased;
            () = cancel.cancelled() => Err(cancelled()),
            () = sleep_until(deadline) => Err(self.abort_reason(cancel)),
            text = attempt.response.text() => text.map_err(|error| {
                if cancel.is_cancelled() {
                    return cancelled();
                }
                self.logger.log(
                    LogLevel::Debug,
                    "ai",
                    "Could not read the account response body",
                    &[("status", json!(status))],
                );
                error
            }),
        }
    }

    fn failure(
        status: u16,
        headers: crate::drivers::ai::errors::Headers,
        body: String,
    ) -> AppError {
        http_failure(HttpFailure {
            headers,
            body: Some(body),
            auth_mode: Some(AuthMode::Account),
            ..HttpFailure::status(status)
        })
    }
}

/// How far a request has gone in recovering from a bearer rejected inside a 2xx answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Recovery {
    Initial,
    Refreshed,
    CookieOnly,
}

impl Recovery {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::Refreshed => "refreshed-bearer",
            Self::CookieOnly => "cookie-only",
        }
    }
}

enum Settled {
    Text(u16, crate::drivers::ai::errors::Headers, String),
    Live(Attempt),
}

fn is_json(headers: &crate::drivers::ai::errors::Headers) -> bool {
    headers
        .get("content-type")
        .is_some_and(|value| value.to_ascii_lowercase().contains("json"))
}

impl AccountTransport {
    /// Send `spec` and settle a bearer the vendor rejects inside a 2xx JSON answer (Qwen:
    /// `{"success": false, "data": {"code": "unauthorized"}}`): refresh the token and retry,
    /// then retry on the profile's cookie session alone, which the vendor also accepts.
    /// A streamed answer that is not JSON is handed back unread.
    async fn settle(
        &self,
        spec: &RequestSpec,
        accept: &str,
        cancel: &CancellationToken,
        streaming: bool,
    ) -> Result<Settled, AppError> {
        let mut attempt = self.attempt(spec, accept, cancel).await?;
        let mut recovery = Recovery::Initial;
        loop {
            let status = attempt.response.status;
            let headers = attempt.response.headers.clone();
            if !attempt.response.ok() {
                let text = self.read(attempt, cancel).await?;
                return Err(Self::failure(status, headers, text));
            }
            if streaming && !is_json(&headers) {
                return Ok(Settled::Live(attempt));
            }
            let authorised = attempt.authorised;
            let text = self.read(attempt, cancel).await?;
            let Some(code) = auth_envelope_code(&text) else {
                if recovery != Recovery::Initial {
                    self.logger.log(
                        LogLevel::Info,
                        "ai",
                        "The account request succeeded after a rejected bearer",
                        &[
                            ("origin", json!(self.origin)),
                            ("path", json!(spec.path)),
                            ("via", json!(recovery.as_str())),
                        ],
                    );
                }
                return Ok(Settled::Text(status, headers, text));
            };
            self.logger.log(
                LogLevel::Warn,
                "ai",
                "The vendor rejected the account request in a successful response",
                &[
                    ("origin", json!(self.origin)),
                    ("path", json!(spec.path)),
                    ("vendorCode", json!(code)),
                    ("attempt", json!(recovery.as_str())),
                ],
            );
            let next = match recovery {
                Recovery::Initial => match self.token(cancel, TokenMode::Refresh).await {
                    Ok(Some(token)) => Some((Recovery::Refreshed, Some(token))),
                    Err(error) if cancel.is_cancelled() => return Err(error),
                    Ok(None) | Err(_) => authorised.then_some((Recovery::CookieOnly, None)),
                },
                Recovery::Refreshed => Some((Recovery::CookieOnly, None)),
                Recovery::CookieOnly => None,
            };
            let Some((step, token)) = next else {
                let mut details = zvs_contract::ErrorDetails::new();
                details.insert("status".to_owned(), json!(status));
                details.insert("vendorCode".to_owned(), json!(code));
                details.insert("reason".to_owned(), json!("token-rejected"));
                details.insert("attempt".to_owned(), json!(recovery.as_str()));
                return Err(session_expired(details));
            };
            recovery = step;
            attempt = self.send(spec, accept, cancel, token).await?;
        }
    }
}

#[async_trait]
impl Transport for AccountTransport {
    fn base_url(&self) -> &str {
        &self.base_url
    }

    async fn request(
        &self,
        spec: &RequestSpec,
        cancel: &CancellationToken,
    ) -> Result<TransportResponse, AppError> {
        match self.settle(spec, "application/json", cancel, false).await? {
            Settled::Text(status, headers, text) => {
                Ok(TransportResponse::new(status, headers, text))
            }
            Settled::Live(attempt) => {
                let status = attempt.response.status;
                let headers = attempt.response.headers.clone();
                let text = self.read(attempt, cancel).await?;
                Ok(TransportResponse::new(status, headers, text))
            }
        }
    }

    async fn stream(
        &self,
        spec: &RequestSpec,
        cancel: &CancellationToken,
    ) -> Result<ByteStream, AppError> {
        let attempt = match self.settle(spec, "text/event-stream", cancel, true).await? {
            // A JSON answer to a streamed call (a vendor error frame) goes to the decoder whole.
            Settled::Text(_, _, text) => {
                return Ok(futures::stream::once(async move { Ok(text.into_bytes()) }).boxed());
            }
            Settled::Live(attempt) => attempt,
        };
        let pump = Pump {
            body: attempt.response.body,
            cancel: cancel.clone(),
            deadline: attempt.deadline,
            timeout_seconds: self.timeout_seconds,
            finished: false,
            _scope: attempt.scope,
        };
        Ok(futures::stream::unfold(pump, |mut pump| async move {
            if pump.finished {
                return None;
            }
            let next = tokio::select! {
                biased;
                () = pump.cancel.cancelled() => Some(Err(cancelled())),
                () = sleep_until(pump.deadline) => Some(Err(if pump.cancel.is_cancelled() {
                    cancelled()
                } else {
                    timed_out(pump.timeout_seconds)
                })),
                chunk = pump.body.next() => chunk.map(|chunk| {
                    chunk.map_err(|error| {
                        if pump.cancel.is_cancelled() {
                            cancelled()
                        } else {
                            error
                        }
                    })
                }),
            };
            let next = next?;
            pump.finished = next.is_err();
            Some((next, pump))
        })
        .boxed())
    }
}
