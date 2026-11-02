use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::header::{ACCEPT, AUTHORIZATION, ETAG, HeaderMap, IF_NONE_MATCH, USER_AGENT};
use reqwest::{Method, StatusCode};
use serde_json::Value;
use zvs_contract::{AppError, AppErrorCode};

use super::copy;
use super::forge::Forge;

pub const USER_AGENT_VALUE: &str = "ZVS-AI-Studio";
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
pub const SHORT_RETRY_SECONDS: u64 = 5;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RateState {
    pub limit: Option<u64>,
    pub remaining: Option<u64>,
    pub reset_at: Option<u64>,
}

#[derive(Debug)]
pub enum ForgeError {
    RateLimited(u64),
    Failed(AppError),
}

impl ForgeError {
    pub fn into_app(self) -> AppError {
        match self {
            Self::RateLimited(seconds) => {
                AppError::new(AppErrorCode::RateLimited, copy::rate_limited(seconds))
                    .with_detail("retryAfter", seconds.to_string())
            }
            Self::Failed(error) => error,
        }
    }
}

impl From<AppError> for ForgeError {
    fn from(error: AppError) -> Self {
        Self::Failed(error)
    }
}

#[derive(Default)]
pub struct Ledger {
    rates: Mutex<HashMap<String, RateState>>,
    etags: Mutex<HashMap<String, String>>,
}

impl Ledger {
    pub fn rate(&self, account: &str) -> RateState {
        self.rates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(account)
            .copied()
            .unwrap_or_default()
    }

    fn record_rate(&self, account: &str, state: RateState) {
        if state.limit.is_none() && state.remaining.is_none() {
            return;
        }
        self.rates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(account.to_owned(), state);
    }

    fn etag(&self, key: &str) -> Option<String> {
        self.etags
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .cloned()
    }

    fn record_etag(&self, key: String, etag: String) {
        self.etags
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, etag);
    }

    pub fn forget(&self, account: &str) {
        self.rates
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(account);
        let prefix = format!("{account} ");
        self.etags
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|key, _| !key.starts_with(&prefix));
    }
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn number(headers: &HeaderMap, names: &[&str]) -> Option<u64> {
    names.iter().find_map(|name| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
    })
}

pub fn rate_from(headers: &HeaderMap) -> RateState {
    RateState {
        limit: number(headers, &["x-ratelimit-limit", "ratelimit-limit"]),
        remaining: number(headers, &["x-ratelimit-remaining", "ratelimit-remaining"]),
        reset_at: number(headers, &["x-ratelimit-reset", "ratelimit-reset"]),
    }
}

fn wait_seconds(headers: &HeaderMap, rate: RateState) -> u64 {
    number(headers, &["retry-after"])
        .or_else(|| rate.reset_at.map(|reset| reset.saturating_sub(unix_now())))
        .unwrap_or(60)
        .max(1)
}

fn message_of(body: &Value) -> Option<String> {
    let message = body
        .get("message")
        .or_else(|| body.get("error_description"))
        .or_else(|| body.get("error"))?;
    match message {
        Value::String(text) => Some(text.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

pub struct ForgeClient<'a> {
    pub http: &'a reqwest::Client,
    pub forge: Forge,
    pub base: String,
    pub bearer: &'a str,
    pub account: &'a str,
    pub ledger: &'a Ledger,
}

impl ForgeClient<'_> {
    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base.trim_end_matches('/'))
    }

    fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        let mut request = self
            .http
            .request(method, self.url(path))
            .timeout(REQUEST_TIMEOUT)
            .header(AUTHORIZATION, format!("Bearer {}", self.bearer))
            .header(USER_AGENT, USER_AGENT_VALUE);
        if self.forge == Forge::GitHub {
            request = request
                .header(ACCEPT, "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28");
        } else {
            request = request.header(ACCEPT, "application/json");
        }
        request
    }

    async fn finish(
        &self,
        response: Result<reqwest::Response, reqwest::Error>,
        etag_key: Option<String>,
    ) -> Result<Option<Value>, ForgeError> {
        let response = response.map_err(|_| {
            ForgeError::Failed(AppError::new(
                AppErrorCode::ProviderUnreachable,
                copy::UNREACHABLE,
            ))
        })?;
        let status = response.status();
        let headers = response.headers().clone();
        let rate = rate_from(&headers);
        self.ledger.record_rate(self.account, rate);
        if status == StatusCode::NOT_MODIFIED {
            return Ok(None);
        }
        let etag = headers
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let body: Value = response.json().await.unwrap_or(Value::Null);
        if status.is_success() {
            if let (Some(key), Some(etag)) = (etag_key, etag) {
                self.ledger.record_etag(key, etag);
            }
            return Ok(Some(body));
        }
        let exhausted = rate.remaining == Some(0)
            || headers.contains_key("retry-after")
            || status == StatusCode::TOO_MANY_REQUESTS;
        if status == StatusCode::TOO_MANY_REQUESTS || (status == StatusCode::FORBIDDEN && exhausted)
        {
            return Err(ForgeError::RateLimited(wait_seconds(&headers, rate)));
        }
        let detail = message_of(&body).unwrap_or_default();
        let error = match status {
            StatusCode::UNAUTHORIZED => {
                AppError::new(AppErrorCode::ProviderAuthFailed, copy::TOKEN_REJECTED)
            }
            StatusCode::FORBIDDEN => AppError::new(AppErrorCode::PermissionDenied, copy::FORBIDDEN),
            StatusCode::NOT_FOUND => AppError::new(AppErrorCode::NotFound, copy::NOT_FOUND),
            status if status.is_client_error() => {
                AppError::new(AppErrorCode::ValidationFailed, copy::REFUSED)
            }
            _ => AppError::new(AppErrorCode::ProviderUnreachable, copy::UNREACHABLE),
        };
        Err(ForgeError::Failed(
            error
                .with_detail("status", status.as_u16().to_string())
                .with_detail("reason", super::super::common::clip(&detail, 300)),
        ))
    }

    pub async fn get(&self, path: &str) -> Result<Value, ForgeError> {
        let response = self.request(Method::GET, path).send().await;
        self.finish(response, None)
            .await
            .map(|body| body.unwrap_or(Value::Null))
    }

    pub async fn get_changed(&self, path: &str) -> Result<Option<Value>, ForgeError> {
        let key = format!("{} {path}", self.account);
        let mut request = self.request(Method::GET, path);
        if let Some(etag) = self.ledger.etag(&key) {
            request = request.header(IF_NONE_MATCH, etag);
        }
        let response = request.send().await;
        self.finish(response, Some(key)).await
    }

    pub async fn send(
        &self,
        method: Method,
        path: &str,
        body: &Value,
    ) -> Result<Value, ForgeError> {
        let response = self.request(method, path).json(body).send().await;
        self.finish(response, None)
            .await
            .map(|body| body.unwrap_or(Value::Null))
    }

    pub async fn retrying_get(&self, path: &str) -> Result<Value, AppError> {
        match self.get(path).await {
            Err(ForgeError::RateLimited(seconds)) if seconds <= SHORT_RETRY_SECONDS => {
                tokio::time::sleep(Duration::from_secs(seconds)).await;
                self.get(path).await.map_err(ForgeError::into_app)
            }
            outcome => outcome.map_err(ForgeError::into_app),
        }
    }

    pub async fn retrying_send(
        &self,
        method: Method,
        path: &str,
        body: &Value,
    ) -> Result<Value, AppError> {
        match self.send(method.clone(), path, body).await {
            Err(ForgeError::RateLimited(seconds)) if seconds <= SHORT_RETRY_SECONDS => {
                tokio::time::sleep(Duration::from_secs(seconds)).await;
                self.send(method, path, body)
                    .await
                    .map_err(ForgeError::into_app)
            }
            outcome => outcome.map_err(ForgeError::into_app),
        }
    }
}
