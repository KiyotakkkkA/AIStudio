use std::time::Duration;

use reqwest::header::ACCEPT;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use url::Url;
use zvs_contract::{AppError, AppErrorCode, Timestamp};

use crate::drivers::mcp::oauth::{Pkce, SIGN_IN_CANCELLED};
use crate::platform::clock::SharedClock;

pub const REFRESH_MARGIN_MS: Timestamp = 120_000;
pub const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
pub const SLOW_DOWN_STEP: Duration = Duration::from_secs(5);

pub const SIGN_IN_REQUIRED: &str = "Требуется вход через браузер. Откройте настройки интеграции.";
pub const OAUTH_FAILED: &str = "Сервер авторизации отклонил запрос";
pub const OAUTH_UNREACHABLE: &str = "Сервер авторизации недоступен";
pub const DEVICE_DENIED: &str = "Вход отклонён на странице подтверждения";
pub const DEVICE_EXPIRED: &str = "Код подтверждения устарел. Повторите вход.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthGrant {
    pub provider: String,
    pub client_id: String,
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<Timestamp>,
}

impl OAuthGrant {
    pub fn parse(raw: &str) -> Result<Self, AppError> {
        serde_json::from_str(raw).map_err(|_| sign_in_required())
    }

    pub fn encode(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn expires_soon(&self, now: Timestamp) -> bool {
        self.expires_at
            .is_some_and(|at| at - REFRESH_MARGIN_MS <= now)
    }
}

pub struct OAuthClient<'a> {
    pub id: &'a str,
    pub secret: Option<&'a str>,
}

pub fn sign_in_required() -> AppError {
    AppError::new(AppErrorCode::ProviderSessionExpired, SIGN_IN_REQUIRED)
}

fn failed() -> AppError {
    AppError::new(AppErrorCode::ProviderAuthFailed, OAUTH_FAILED)
}

fn unreachable() -> AppError {
    AppError::new(AppErrorCode::ProviderUnreachable, OAUTH_UNREACHABLE)
}

pub struct AuthorizeRequest<'a> {
    pub endpoint: &'a str,
    pub client: &'a OAuthClient<'a>,
    pub redirect_uri: &'a str,
    pub scope: &'a str,
    pub state: &'a str,
    pub pkce: &'a Pkce,
    pub extra: &'a [(&'a str, &'a str)],
}

pub fn authorize_url(request: &AuthorizeRequest<'_>) -> Result<String, AppError> {
    let mut url = Url::parse(request.endpoint).map_err(|_| failed())?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", request.client.id)
            .append_pair("redirect_uri", request.redirect_uri)
            .append_pair("scope", request.scope)
            .append_pair("state", request.state)
            .append_pair("code_challenge", &request.pkce.challenge)
            .append_pair("code_challenge_method", "S256");
        for (name, value) in request.extra {
            query.append_pair(name, value);
        }
    }
    Ok(url.to_string())
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

async fn post_form(
    http: &reqwest::Client,
    endpoint: &str,
    fields: &[(&str, &str)],
) -> Result<(reqwest::StatusCode, Value), AppError> {
    let response = http
        .post(endpoint)
        .header(ACCEPT, "application/json")
        .form(fields)
        .send()
        .await
        .map_err(|_| unreachable())?;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    Ok((status, body))
}

async fn token(
    http: &reqwest::Client,
    endpoint: &str,
    client: &OAuthClient<'_>,
    form: &[(&str, &str)],
) -> Result<TokenResponse, AppError> {
    let mut fields: Vec<(&str, &str)> = form.to_vec();
    fields.push(("client_id", client.id));
    if let Some(secret) = client.secret {
        fields.push(("client_secret", secret));
    }
    let (status, body) = post_form(http, endpoint, &fields).await?;
    if !status.is_success() || body.get("error").is_some() {
        let refused = body
            .get("error")
            .and_then(Value::as_str)
            .is_some_and(|error| error == "invalid_grant");
        return Err(if refused {
            sign_in_required()
        } else {
            failed().with_detail("status", status.as_u16().to_string())
        });
    }
    serde_json::from_value::<TokenResponse>(body).map_err(|_| failed())
}

fn expiry(now: Timestamp, seconds: Option<i64>) -> Option<Timestamp> {
    seconds
        .filter(|seconds| *seconds > 0)
        .map(|seconds| now.saturating_add(seconds.saturating_mul(1000)))
}

pub struct CodeExchange<'a> {
    pub provider: &'a str,
    pub token_endpoint: &'a str,
    pub client: OAuthClient<'a>,
    pub code: &'a str,
    pub redirect_uri: &'a str,
    pub verifier: &'a str,
    pub now: Timestamp,
}

pub async fn exchange(
    http: &reqwest::Client,
    exchange: CodeExchange<'_>,
) -> Result<OAuthGrant, AppError> {
    let response = token(
        http,
        exchange.token_endpoint,
        &exchange.client,
        &[
            ("grant_type", "authorization_code"),
            ("code", exchange.code),
            ("redirect_uri", exchange.redirect_uri),
            ("code_verifier", exchange.verifier),
        ],
    )
    .await?;
    Ok(OAuthGrant {
        provider: exchange.provider.to_owned(),
        client_id: exchange.client.id.to_owned(),
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        expires_at: expiry(exchange.now, response.expires_in),
    })
}

pub async fn refresh(
    http: &reqwest::Client,
    token_endpoint: &str,
    client: &OAuthClient<'_>,
    grant: &OAuthGrant,
    now: Timestamp,
) -> Result<OAuthGrant, AppError> {
    let refresh_token = grant
        .refresh_token
        .as_deref()
        .ok_or_else(sign_in_required)?;
    let response = token(
        http,
        token_endpoint,
        client,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ],
    )
    .await?;
    Ok(OAuthGrant {
        access_token: response.access_token,
        refresh_token: response
            .refresh_token
            .or_else(|| grant.refresh_token.clone()),
        expires_at: expiry(now, response.expires_in),
        ..grant.clone()
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DeviceAuthorization {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

const fn default_interval() -> u64 {
    5
}

pub async fn device_code(
    http: &reqwest::Client,
    endpoint: &str,
    client_id: &str,
    scope: &str,
) -> Result<DeviceAuthorization, AppError> {
    let (status, body) = post_form(
        http,
        endpoint,
        &[("client_id", client_id), ("scope", scope)],
    )
    .await?;
    if !status.is_success() || body.get("error").is_some() {
        return Err(failed().with_detail("status", status.as_u16().to_string()));
    }
    serde_json::from_value(body).map_err(|_| failed())
}

pub struct DevicePoll<'a> {
    pub provider: &'a str,
    pub token_endpoint: &'a str,
    pub client_id: &'a str,
    pub device: &'a DeviceAuthorization,
    pub clock: &'a SharedClock,
    pub cancel: &'a CancellationToken,
}

pub async fn poll_device(
    http: &reqwest::Client,
    poll: DevicePoll<'_>,
) -> Result<OAuthGrant, AppError> {
    let started = poll.clock.now();
    let deadline = started.saturating_add(
        i64::try_from(poll.device.expires_in.saturating_mul(1000)).unwrap_or(i64::MAX),
    );
    let mut interval = Duration::from_secs(poll.device.interval.max(1));
    loop {
        tokio::select! {
            () = poll.cancel.cancelled() => {
                return Err(AppError::new(AppErrorCode::Conflict, SIGN_IN_CANCELLED));
            }
            () = tokio::time::sleep(interval) => {}
        }
        if poll.clock.now() >= deadline {
            return Err(AppError::new(
                AppErrorCode::ProviderSessionExpired,
                DEVICE_EXPIRED,
            ));
        }
        let (status, body) = post_form(
            http,
            poll.token_endpoint,
            &[
                ("client_id", poll.client_id),
                ("device_code", &poll.device.device_code),
                ("grant_type", DEVICE_GRANT),
            ],
        )
        .await?;
        match body.get("error").and_then(Value::as_str) {
            Some("authorization_pending") => {}
            Some("slow_down") => {
                interval = body
                    .get("interval")
                    .and_then(Value::as_u64)
                    .map_or(interval + SLOW_DOWN_STEP, Duration::from_secs);
            }
            Some("expired_token") => {
                return Err(AppError::new(
                    AppErrorCode::ProviderSessionExpired,
                    DEVICE_EXPIRED,
                ));
            }
            Some("access_denied") => {
                return Err(AppError::new(
                    AppErrorCode::ProviderAuthFailed,
                    DEVICE_DENIED,
                ));
            }
            Some(_) => return Err(failed()),
            None if status.is_success() => {
                let response: TokenResponse = serde_json::from_value(body).map_err(|_| failed())?;
                return Ok(OAuthGrant {
                    provider: poll.provider.to_owned(),
                    client_id: poll.client_id.to_owned(),
                    access_token: response.access_token,
                    refresh_token: response.refresh_token,
                    expires_at: expiry(poll.clock.now(), response.expires_in),
                });
            }
            None => return Err(failed().with_detail("status", status.as_u16().to_string())),
        }
    }
}

pub fn trusted_https_url(address: &str, hosts: &[&str]) -> Option<Url> {
    let parsed = Url::parse(address).ok()?;
    (parsed.scheme() == "https"
        && parsed.host_str().is_some_and(|host| {
            hosts
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(host))
        })
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.port().is_none())
    .then_some(parsed)
}
