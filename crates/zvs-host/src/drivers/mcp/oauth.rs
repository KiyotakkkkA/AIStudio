use std::collections::HashMap;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::StatusCode;
use reqwest::header::{ACCEPT, WWW_AUTHENTICATE};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;
use url::Url;
use uuid::Uuid;
use zvs_contract::{AppError, AppErrorCode, Timestamp};

pub const CALLBACK_PATH: &str = "/callback";
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);
pub const REFRESH_MARGIN_MS: Timestamp = 60_000;
pub const AUTH_REQUIRED: &str =
    "Сервер требует входа: нажмите «Войти», чтобы авторизоваться в браузере";
pub const SIGN_IN_CANCELLED: &str = "Вход отменён";
pub const SIGN_IN_TIMED_OUT: &str = "Вход не завершён за 5 минут";
const MAX_REQUEST_BYTES: usize = 16 * 1024;
const CALLBACK_PAGE: &str = include_str!("oauth_callback.html");
const PAGE_DONE_TITLE: &str = "Вход выполнен";
const PAGE_DONE: &str = "Сервис успешно подключен.";
const PAGE_DONE_HINT: &str = "Вернитесь в ZVS AI Studio — эту вкладку можно закрыть.";
const PAGE_FAILED_TITLE: &str = "Вход не выполнен";
const PAGE_FAILED: &str = "Сервер авторизации не подтвердил вход.";
const PAGE_FAILED_HINT: &str = "Вернитесь в ZVS AI Studio и нажмите «Войти» ещё раз.";
const ICON_DONE: &str = "M9 16.17 4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z";
const ICON_FAILED: &str = "M19 6.41 17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthGrant {
    pub issuer: String,
    pub token_endpoint: String,
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    pub redirect_uri: String,
    pub resource: String,
    #[serde(default)]
    pub scope: Option<String>,
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<Timestamp>,
}

impl OAuthGrant {
    pub fn expires_soon(&self, now: Timestamp) -> bool {
        self.expires_at
            .is_some_and(|at| at - REFRESH_MARGIN_MS <= now)
    }

    pub fn redirect_port(&self) -> Option<u16> {
        Url::parse(&self.redirect_uri).ok()?.port()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct ResourceMetadata {
    #[serde(default)]
    pub resource: Option<String>,
    #[serde(default)]
    pub authorization_servers: Vec<String>,
    #[serde(default)]
    pub scopes_supported: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ServerMetadata {
    #[serde(default)]
    pub issuer: Option<String>,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub registration_endpoint: Option<String>,
    #[serde(default)]
    pub scopes_supported: Vec<String>,
    #[serde(default)]
    pub code_challenge_methods_supported: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthDiscovery {
    pub resource: String,
    pub issuer: String,
    pub server: ServerMetadata,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ClientRegistration {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn generate() -> Self {
        let verifier: String = (0..3)
            .map(|_| Uuid::new_v4().simple().to_string())
            .collect();
        Self::from_verifier(verifier)
    }

    pub fn from_verifier(verifier: String) -> Self {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        Self {
            verifier,
            challenge,
        }
    }
}

pub fn random_state() -> String {
    Uuid::new_v4().simple().to_string()
}

fn failure(message: impl Into<String>) -> AppError {
    AppError::new(AppErrorCode::McpUnavailable, message)
}

pub fn auth_required(challenge: Option<&str>) -> AppError {
    let error = failure(AUTH_REQUIRED).with_detail("auth", "required");
    match challenge {
        Some(challenge) => error.with_detail("challenge", challenge),
        None => error,
    }
}

pub fn is_auth_required(error: &AppError) -> bool {
    error
        .details
        .as_ref()
        .and_then(|details| details.get("auth"))
        .and_then(Value::as_str)
        == Some("required")
}

pub fn challenge_of(error: &AppError) -> Option<String> {
    error
        .details
        .as_ref()
        .and_then(|details| details.get("challenge"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

pub fn challenge_from(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get_all(WWW_AUTHENTICATE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| {
            value
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("bearer")
        })
        .map(str::to_owned)
}

pub fn challenge_param(challenge: &str, name: &str) -> Option<String> {
    let pattern = format!(
        r#"(?i)(?:^|[\s,]){}\s*=\s*(?:"([^"]*)"|([^\s,]+))"#,
        regex::escape(name)
    );
    let captures = regex::Regex::new(&pattern).ok()?.captures(challenge)?;
    captures
        .get(1)
        .or_else(|| captures.get(2))
        .map(|value| value.as_str().to_owned())
}

fn well_known(base: &Url, suffix: &str) -> Vec<String> {
    let origin = base.origin().ascii_serialization();
    let path = base.path().trim_end_matches('/');
    let mut urls = Vec::new();
    if !path.is_empty() {
        urls.push(format!("{origin}/.well-known/{suffix}{path}"));
    }
    urls.push(format!("{origin}/.well-known/{suffix}"));
    urls
}

fn upgraded(candidate: &str, base: &Url) -> Vec<String> {
    let mut urls = Vec::new();
    if let Ok(parsed) = Url::parse(candidate) {
        if parsed.scheme() == "http" && base.scheme() == "https" && parsed.host() == base.host() {
            let mut secure = parsed.clone();
            if secure.set_scheme("https").is_ok() {
                if parsed.port().is_none() {
                    let _ = secure.set_port(None);
                }
                urls.push(secure.to_string());
            }
        }
        urls.push(parsed.to_string());
    }
    urls
}

async fn fetch_json<T: for<'de> Deserialize<'de>>(http: &reqwest::Client, url: &str) -> Option<T> {
    let response = http
        .get(url)
        .header(ACCEPT, "application/json")
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<T>().await.ok()
}

pub async fn probe_challenge(http: &reqwest::Client, url: &str) -> Option<String> {
    let response = http
        .post(url)
        .header(ACCEPT, "application/json, text/event-stream")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "zvs-studio", "version": "0" },
            },
        }))
        .send()
        .await
        .ok()?;
    if response.status() != StatusCode::UNAUTHORIZED && response.status() != StatusCode::FORBIDDEN {
        return None;
    }
    challenge_from(response.headers())
}

pub async fn discover(
    http: &reqwest::Client,
    url: &str,
    challenge: Option<&str>,
) -> Result<OAuthDiscovery, AppError> {
    let base = Url::parse(url).map_err(|_| failure("Некорректный адрес MCP-сервера"))?;
    let mut candidates = Vec::new();
    if let Some(metadata) = challenge.and_then(|value| challenge_param(value, "resource_metadata"))
    {
        candidates.extend(upgraded(&metadata, &base));
    }
    candidates.extend(well_known(&base, "oauth-protected-resource"));
    let mut resource_metadata = None;
    for candidate in &candidates {
        if let Some(metadata) = fetch_json::<ResourceMetadata>(http, candidate).await {
            resource_metadata = Some(metadata);
            break;
        }
    }
    let resource_metadata = resource_metadata.unwrap_or_default();
    let issuer = resource_metadata
        .authorization_servers
        .first()
        .cloned()
        .unwrap_or_else(|| base.origin().ascii_serialization());
    let issuer_url =
        Url::parse(&issuer).map_err(|_| failure("Сервер авторизации указан некорректно"))?;
    let mut metadata_urls = well_known(&issuer_url, "oauth-authorization-server");
    metadata_urls.extend(well_known(&issuer_url, "openid-configuration"));
    metadata_urls.push(format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    ));
    let mut server = None;
    for candidate in &metadata_urls {
        if let Some(metadata) = fetch_json::<ServerMetadata>(http, candidate).await {
            server = Some(metadata);
            break;
        }
    }
    let server = server.ok_or_else(|| {
        failure(format!(
            "Не удалось получить настройки сервера авторизации {issuer}"
        ))
    })?;
    if !server.code_challenge_methods_supported.is_empty()
        && !server
            .code_challenge_methods_supported
            .iter()
            .any(|method| method == "S256")
    {
        return Err(failure("Сервер авторизации не поддерживает PKCE S256"));
    }
    let scopes = if resource_metadata.scopes_supported.is_empty() {
        challenge
            .and_then(|value| challenge_param(value, "scope"))
            .map(|scope| scope.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default()
    } else {
        resource_metadata.scopes_supported.clone()
    };
    let mut scopes: Vec<String> = scopes;
    if server
        .scopes_supported
        .iter()
        .any(|scope| scope == "offline_access")
        && !scopes.iter().any(|scope| scope == "offline_access")
    {
        scopes.push("offline_access".to_owned());
    }
    let resource = resource_metadata
        .resource
        .clone()
        .filter(|resource| !resource.is_empty())
        .unwrap_or_else(|| url.to_owned());
    Ok(OAuthDiscovery {
        resource,
        issuer: server.issuer.clone().unwrap_or(issuer),
        server,
        scope: (!scopes.is_empty()).then(|| scopes.join(" ")),
    })
}

pub async fn register(
    http: &reqwest::Client,
    endpoint: &str,
    redirect_uri: &str,
    client_name: &str,
    scope: Option<&str>,
) -> Result<ClientRegistration, AppError> {
    let mut body = json!({
        "client_name": client_name,
        "redirect_uris": [redirect_uri],
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
        "application_type": "native",
    });
    if let Some(scope) = scope {
        body["scope"] = json!(scope);
    }
    let response = http
        .post(endpoint)
        .header(ACCEPT, "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|cause| failure(format!("Регистрация клиента не удалась: {cause}")))?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(failure(format!(
            "Сервер авторизации отклонил регистрацию клиента ({}) {}",
            status.as_u16(),
            zvs_contract::validate::js_slice(text.trim(), 300)
        )));
    }
    response
        .json::<ClientRegistration>()
        .await
        .map_err(|_| failure("Сервер авторизации прислал непонятный ответ на регистрацию"))
}

pub fn authorize_url(
    discovery: &OAuthDiscovery,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    pkce: &Pkce,
) -> Result<String, AppError> {
    let mut url = Url::parse(&discovery.server.authorization_endpoint)
        .map_err(|_| failure("Адрес авторизации указан некорректно"))?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("state", state)
            .append_pair("code_challenge", &pkce.challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("resource", &discovery.resource);
        if let Some(scope) = &discovery.scope {
            query.append_pair("scope", scope);
        }
    }
    Ok(url.to_string())
}

async fn token_request(
    http: &reqwest::Client,
    endpoint: &str,
    form: &[(&str, &str)],
) -> Result<TokenResponse, AppError> {
    let response = http
        .post(endpoint)
        .header(ACCEPT, "application/json")
        .form(form)
        .send()
        .await
        .map_err(|cause| failure(format!("Сервер авторизации недоступен: {cause}")))?;
    let status = response.status();
    if !status.is_success() {
        let body: Value = response.json().await.unwrap_or(Value::Null);
        let reason = body
            .get("error_description")
            .or_else(|| body.get("error"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        return Err(failure(format!(
            "Сервер авторизации отказал в токене ({}){}",
            status.as_u16(),
            if reason.is_empty() {
                String::new()
            } else {
                format!(": {reason}")
            }
        ))
        .with_detail("auth", "required"));
    }
    response
        .json::<TokenResponse>()
        .await
        .map_err(|_| failure("Сервер авторизации прислал непонятный токен"))
}

fn client_form<'a>(
    form: &mut Vec<(&'a str, &'a str)>,
    client_id: &'a str,
    secret: Option<&'a str>,
) {
    form.push(("client_id", client_id));
    if let Some(secret) = secret {
        form.push(("client_secret", secret));
    }
}

pub struct CodeExchange<'a> {
    pub discovery: &'a OAuthDiscovery,
    pub client: &'a ClientRegistration,
    pub redirect_uri: &'a str,
    pub code: &'a str,
    pub verifier: &'a str,
    pub now: Timestamp,
}

pub async fn exchange_code(
    http: &reqwest::Client,
    exchange: CodeExchange<'_>,
) -> Result<OAuthGrant, AppError> {
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", exchange.code),
        ("redirect_uri", exchange.redirect_uri),
        ("code_verifier", exchange.verifier),
        ("resource", exchange.discovery.resource.as_str()),
    ];
    client_form(
        &mut form,
        &exchange.client.client_id,
        exchange.client.client_secret.as_deref(),
    );
    let token = token_request(http, &exchange.discovery.server.token_endpoint, &form).await?;
    Ok(OAuthGrant {
        issuer: exchange.discovery.issuer.clone(),
        token_endpoint: exchange.discovery.server.token_endpoint.clone(),
        client_id: exchange.client.client_id.clone(),
        client_secret: exchange.client.client_secret.clone(),
        redirect_uri: exchange.redirect_uri.to_owned(),
        resource: exchange.discovery.resource.clone(),
        scope: token.scope.or_else(|| exchange.discovery.scope.clone()),
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at: token
            .expires_in
            .map(|seconds| exchange.now + seconds * 1000),
    })
}

pub async fn refresh(
    http: &reqwest::Client,
    grant: &OAuthGrant,
    now: Timestamp,
) -> Result<OAuthGrant, AppError> {
    let Some(refresh_token) = grant.refresh_token.as_deref() else {
        return Err(auth_required(None));
    };
    let mut form = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("resource", grant.resource.as_str()),
    ];
    client_form(&mut form, &grant.client_id, grant.client_secret.as_deref());
    let token = token_request(http, &grant.token_endpoint, &form).await?;
    Ok(OAuthGrant {
        access_token: token.access_token,
        refresh_token: token.refresh_token.or_else(|| grant.refresh_token.clone()),
        expires_at: token.expires_in.map(|seconds| now + seconds * 1000),
        scope: token.scope.or_else(|| grant.scope.clone()),
        ..grant.clone()
    })
}

pub struct Loopback {
    listener: TcpListener,
    pub redirect_uri: String,
}

impl Loopback {
    pub async fn bind(preferred_port: Option<u16>) -> Result<Self, AppError> {
        let listener = match preferred_port {
            Some(port) => TcpListener::bind(("127.0.0.1", port)).await.ok(),
            None => None,
        };
        let listener = match listener {
            Some(listener) => listener,
            None => TcpListener::bind(("127.0.0.1", 0))
                .await
                .map_err(|cause| failure(format!("Не удалось открыть порт для входа: {cause}")))?,
        };
        let port = listener
            .local_addr()
            .map_err(|cause| failure(cause.to_string()))?
            .port();
        Ok(Self {
            listener,
            redirect_uri: format!("http://127.0.0.1:{port}{CALLBACK_PATH}"),
        })
    }

    pub fn port(&self) -> Option<u16> {
        self.listener
            .local_addr()
            .ok()
            .map(|address| address.port())
    }

    pub async fn wait_for_code(
        self,
        state: &str,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Result<String, AppError> {
        let waiting = async {
            loop {
                let Ok((stream, _)) = self.listener.accept().await else {
                    continue;
                };
                if let Some(outcome) = answer(stream, state).await {
                    return outcome;
                }
            }
        };
        tokio::select! {
            outcome = tokio::time::timeout(timeout, waiting) => {
                outcome.unwrap_or_else(|_| Err(failure(SIGN_IN_TIMED_OUT)))
            }
            () = cancel.cancelled() => Err(failure(SIGN_IN_CANCELLED)),
        }
    }
}

fn page(signed_in: bool) -> String {
    let (title, message, hint, icon, light, soft_light, dark, soft_dark) = if signed_in {
        (
            PAGE_DONE_TITLE,
            PAGE_DONE,
            PAGE_DONE_HINT,
            ICON_DONE,
            "#4d7a2a",
            "#eef3e6",
            "#a9be6a",
            "rgb(169 190 106 / 0.14)",
        )
    } else {
        (
            PAGE_FAILED_TITLE,
            PAGE_FAILED,
            PAGE_FAILED_HINT,
            ICON_FAILED,
            "#bf3a30",
            "#fbe9e7",
            "#ec6b5f",
            "rgb(236 107 95 / 0.14)",
        )
    };
    CALLBACK_PAGE
        .replace("{{title}}", title)
        .replace("{{message}}", message)
        .replace("{{hint}}", hint)
        .replace("{{icon}}", icon)
        .replace("{{tone-light}}", light)
        .replace("{{tone-soft-light}}", soft_light)
        .replace("{{tone-dark}}", dark)
        .replace("{{tone-soft-dark}}", soft_dark)
}

async fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

pub fn parse_callback(target: &str, state: &str) -> Option<Result<String, AppError>> {
    let url = Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
    if url.path() != CALLBACK_PATH {
        return None;
    }
    let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
    if query.get("state").map(String::as_str) != Some(state) {
        return None;
    }
    if let Some(error) = query.get("error") {
        let description = query
            .get("error_description")
            .cloned()
            .unwrap_or_else(|| error.clone());
        return Some(Err(failure(format!("Вход отклонён: {description}"))));
    }
    Some(
        query
            .get("code")
            .filter(|code| !code.is_empty())
            .cloned()
            .ok_or_else(|| failure("Сервер авторизации не вернул код")),
    )
}

async fn answer(mut stream: TcpStream, state: &str) -> Option<Result<String, AppError>> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 2048];
    while !buffer.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut chunk))
            .await
            .ok()?
            .ok()?;
        if read == 0 || buffer.len() > MAX_REQUEST_BYTES {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let text = String::from_utf8_lossy(&buffer);
    let mut parts = text.lines().next().unwrap_or_default().split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    if method != "GET" {
        respond(&mut stream, "405 Method Not Allowed", "").await;
        return None;
    }
    match parse_callback(target, state) {
        None => {
            respond(&mut stream, "404 Not Found", "").await;
            None
        }
        Some(Ok(code)) => {
            respond(&mut stream, "200 OK", &page(true)).await;
            Some(Ok(code))
        }
        Some(Err(error)) => {
            respond(&mut stream, "400 Bad Request", &page(false)).await;
            Some(Err(error))
        }
    }
}

#[cfg(test)]
mod tests;
