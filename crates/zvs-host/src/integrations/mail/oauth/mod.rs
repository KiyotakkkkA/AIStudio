pub mod google;
pub mod mailru;
pub mod yandex;

use reqwest::header::ACCEPT;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;
use zvs_contract::{AppError, AppErrorCode, Timestamp};

use super::copy;
use crate::drivers::mcp::oauth::Pkce;

pub const REFRESH_MARGIN_MS: Timestamp = 120_000;
pub const PRESETS: [MailPreset; 3] = [google::PRESET, yandex::PRESET, mailru::PRESET];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MailPreset {
    pub key: &'static str,
    pub label: &'static str,
    pub imap_host: &'static str,
    pub imap_port: u16,
    pub smtp_host: &'static str,
    pub smtp_port: u16,
    pub authorize_endpoint: &'static str,
    pub token_endpoint: &'static str,
    pub scope: &'static str,
    pub authorize_params: &'static [(&'static str, &'static str)],
}

pub fn preset(key: &str) -> Option<MailPreset> {
    PRESETS.into_iter().find(|preset| preset.key == key)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailGrant {
    pub provider: String,
    pub client_id: String,
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<Timestamp>,
}

impl MailGrant {
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
    AppError::new(AppErrorCode::ProviderSessionExpired, copy::SIGN_IN_REQUIRED)
}

pub fn authorize_url(
    preset: &MailPreset,
    client: &OAuthClient<'_>,
    redirect_uri: &str,
    state: &str,
    pkce: &Pkce,
) -> Result<String, AppError> {
    let mut url = Url::parse(preset.authorize_endpoint)
        .map_err(|_| AppError::new(AppErrorCode::ValidationFailed, copy::OAUTH_FAILED))?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", client.id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("scope", preset.scope)
            .append_pair("state", state)
            .append_pair("code_challenge", &pkce.challenge)
            .append_pair("code_challenge_method", "S256");
        for (name, value) in preset.authorize_params {
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
    let response = http
        .post(endpoint)
        .header(ACCEPT, "application/json")
        .form(&fields)
        .send()
        .await
        .map_err(|_| AppError::new(AppErrorCode::ProviderUnreachable, copy::OAUTH_UNREACHABLE))?;
    let status = response.status();
    if !status.is_success() {
        let body: Value = response.json().await.unwrap_or(Value::Null);
        let refused = body
            .get("error")
            .and_then(Value::as_str)
            .is_some_and(|error| error == "invalid_grant");
        return Err(if refused {
            sign_in_required()
        } else {
            AppError::new(AppErrorCode::ProviderAuthFailed, copy::OAUTH_FAILED)
                .with_detail("status", status.as_u16().to_string())
        });
    }
    response
        .json::<TokenResponse>()
        .await
        .map_err(|_| AppError::new(AppErrorCode::ProviderAuthFailed, copy::OAUTH_FAILED))
}

fn expiry(now: Timestamp, seconds: Option<i64>) -> Option<Timestamp> {
    seconds.map(|seconds| now.saturating_add(seconds.saturating_mul(1000)))
}

pub struct CodeExchange<'a> {
    pub preset: &'a MailPreset,
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
) -> Result<MailGrant, AppError> {
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
    Ok(MailGrant {
        provider: exchange.preset.key.to_owned(),
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
    grant: &MailGrant,
    now: Timestamp,
) -> Result<MailGrant, AppError> {
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
    Ok(MailGrant {
        access_token: response.access_token,
        refresh_token: response
            .refresh_token
            .or_else(|| grant.refresh_token.clone()),
        expires_at: expiry(now, response.expires_in),
        ..grant.clone()
    })
}

pub fn xoauth2(user: &str, access_token: &str) -> String {
    format!("user={user}\x01auth=Bearer {access_token}\x01\x01")
}

pub struct XOAuth2 {
    initial: Option<String>,
}

impl XOAuth2 {
    pub fn new(user: &str, access_token: &str) -> Self {
        Self {
            initial: Some(xoauth2(user, access_token)),
        }
    }
}

impl async_imap::Authenticator for XOAuth2 {
    type Response = String;

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        self.initial.take().unwrap_or_default()
    }
}
