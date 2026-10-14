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
    HttpFailure, cancelled, http_failure, is_signed_out_response, request_failed, session_expired,
    timed_out,
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
        Ok(Attempt {
            response: outcome?,
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

    fn guard(
        &self,
        status: u16,
        headers: &crate::drivers::ai::errors::Headers,
    ) -> Result<(), AppError> {
        if !is_signed_out_response(status, headers) {
            return Ok(());
        }
        let mut details = zvs_contract::ErrorDetails::new();
        details.insert("status".to_owned(), json!(status));
        details.insert(
            "partition".to_owned(),
            Value::from(self.session.partition()),
        );
        Err(session_expired(details))
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
        let attempt = self.attempt(spec, "application/json", cancel).await?;
        let status = attempt.response.status;
        let headers = attempt.response.headers.clone();
        let ok = attempt.response.ok();
        let text = self.read(attempt, cancel).await?;
        self.guard(status, &headers)?;
        if !ok {
            return Err(Self::failure(status, headers, text));
        }
        Ok(TransportResponse::new(status, headers, text))
    }

    async fn stream(
        &self,
        spec: &RequestSpec,
        cancel: &CancellationToken,
    ) -> Result<ByteStream, AppError> {
        let attempt = self.attempt(spec, "text/event-stream", cancel).await?;
        let status = attempt.response.status;
        let headers = attempt.response.headers.clone();
        if !attempt.response.ok() {
            let body = self.read(attempt, cancel).await?;
            self.guard(status, &headers)?;
            return Err(Self::failure(status, headers, body));
        }
        self.guard(status, &headers)?;
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
