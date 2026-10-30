use std::sync::LazyLock;
use std::time::Duration;

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use url::Url;
use zvs_contract::{AppError, AppErrorCode};
use zvs_openai::{API_BASE, Credentials, TOKEN_URL, TokenResponse};

use super::errors::cancelled;
use super::http::HttpClient;
use super::transport::SecretResolver;
use crate::services::accounts::AccountBrowser;
use crate::services::crypto::Plaintext;

static REFRESH: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub fn failure() -> AppError {
    AppError::new(
        AppErrorCode::ProviderUnreachable,
        "Не удалось выполнить запрос OpenAI",
    )
}

pub fn auth_error() -> AppError {
    AppError::new(
        AppErrorCode::ProviderSessionExpired,
        "Требуется повторный вход в OpenAI",
    )
}

pub fn now_seconds() -> i64 {
    crate::platform::clock::Clock::now(&crate::platform::clock::SystemClock) / 1000
}

fn random() -> String {
    format!(
        "{}{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

async fn response_json(request: reqwest::RequestBuilder) -> Result<Value, AppError> {
    let response = request
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| failure())?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(auth_error());
    }
    let value = response.json::<Value>().await.map_err(|_| failure())?;
    if !status.is_success() {
        let code = value
            .get("error")
            .and_then(Value::as_str)
            .or_else(|| value.pointer("/error/code").and_then(Value::as_str));
        if matches!(
            code,
            Some(
                "invalid_grant"
                    | "invalid_token"
                    | "refresh_token_expired"
                    | "refresh_token_reused"
            )
        ) {
            return Err(auth_error());
        }
        return Err(failure()
            .with_detail("status", status.as_u16())
            .with_detail("reason", code.unwrap_or("openai-request-failed")));
    }
    Ok(value)
}

#[derive(Clone, Deserialize)]
struct Claims {
    sub: String,
    nonce: Option<String>,
    email: Option<String>,
    name: Option<String>,
}

async fn validate_identity(
    http: &HttpClient,
    token: &str,
    client_id: &str,
    nonce: Option<&str>,
) -> Result<Claims, AppError> {
    let header = decode_header(token).map_err(|_| auth_error())?;
    if !matches!(header.alg, Algorithm::RS256 | Algorithm::ES256) {
        return Err(auth_error());
    }
    let keys: JwkSet = serde_json::from_value(
        response_json(
            http.client()
                .get("https://auth.openai.com/.well-known/jwks.json"),
        )
        .await?,
    )
    .map_err(|_| auth_error())?;
    let key = keys
        .find(header.kid.as_deref().ok_or_else(auth_error)?)
        .ok_or_else(auth_error)?;
    let mut validation = Validation::new(header.alg);
    validation.set_audience(&[client_id]);
    validation.set_issuer(&["https://auth.openai.com"]);
    validation.set_required_spec_claims(&["sub", "exp", "iat", "iss", "aud"]);
    validation.leeway = 5;
    let claims = decode::<Claims>(
        token,
        &DecodingKey::from_jwk(key).map_err(|_| auth_error())?,
        &validation,
    )
    .map_err(|_| auth_error())?
    .claims;
    if claims.sub.is_empty()
        || nonce.is_some_and(|expected| claims.nonce.as_deref() != Some(expected))
    {
        return Err(auth_error());
    }
    Ok(claims)
}

pub async fn authorize(
    browser: &dyn AccountBrowser,
    host_id: &str,
    saved: Option<&Credentials>,
    cancel: &CancellationToken,
) -> Result<Credentials, AppError> {
    let http = HttpClient::new()?;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|_| failure())?;
    let port = listener.local_addr().map_err(|_| failure())?.port();
    let redirect = format!("http://127.0.0.1:{port}/auth/callback");
    let state = random();
    let nonce = random();
    let verifier = random();
    let mut url = zvs_openai::authorization_url(
        host_id,
        &redirect,
        &state,
        &nonce,
        &verifier,
        saved.map(|saved| (saved.client_id.as_str(), saved.id_token.as_str())),
    );
    if saved.is_some_and(|saved| !saved.allows_inference()) {
        url.query_pairs_mut().append_pair("prompt", "consent");
    }
    browser.open_external(url.as_str()).await?;
    let (code, client_id) = loop {
        let (mut stream, _) = tokio::select! {
            () = cancel.cancelled() => return Err(cancelled()),
            accepted = listener.accept() => accepted.map_err(|_| failure())?,
        };
        let mut bytes = Vec::new();
        let received = tokio::select! {
            () = cancel.cancelled() => return Err(cancelled()),
            result = tokio::time::timeout(Duration::from_secs(3), async {
                let mut chunk = [0; 1024];
                while bytes.len() < 8192 && !bytes.windows(4).any(|part| part == b"\r\n\r\n") {
                    let size = stream.read(&mut chunk).await?;
                    if size == 0 { break; }
                    bytes.extend_from_slice(&chunk[..size]);
                }
                Ok::<_, std::io::Error>(bytes.len() < 8192 && bytes.windows(4).any(|part| part == b"\r\n\r\n"))
            }) => matches!(result, Ok(Ok(true))),
        };
        if !received {
            continue;
        }
        let line = String::from_utf8_lossy(&bytes);
        let mut fields = line.lines().next().unwrap_or_default().split_whitespace();
        let method = fields.next();
        let path = fields.next().unwrap_or_default();
        let Ok(callback) = Url::parse(&format!("http://127.0.0.1:{port}{path}")) else {
            continue;
        };
        let result = if method == Some("GET") {
            zvs_openai::callback(
                &callback,
                &state,
                saved.map(|saved| saved.client_id.as_str()),
            )
        } else {
            Err(zvs_openai::CallbackError::Invalid)
        };
        let valid = !matches!(result, Err(zvs_openai::CallbackError::Invalid));
        let reply = if valid {
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\nВход завершён. Можно вернуться в ZVS AI Studio."
        } else {
            "HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nInvalid callback"
        };
        let _ = stream.write_all(reply.as_bytes()).await;
        if !valid {
            continue;
        }
        break result.map_err(|error| match error {
            zvs_openai::CallbackError::Denied => cancelled(),
            zvs_openai::CallbackError::Invalid => auth_error(),
        })?;
    };
    let response: TokenResponse = serde_json::from_value(
        response_json(http.client().post(TOKEN_URL).form(&[
            ("grant_type", "authorization_code"),
            ("client_id", &client_id),
            ("code", &code),
            ("code_verifier", &verifier),
            ("redirect_uri", &redirect),
            ("resource", API_BASE),
        ]))
        .await?,
    )
    .map_err(|_| auth_error())?;
    let id_token = response.id_token.ok_or_else(auth_error)?;
    let claims = validate_identity(&http, &id_token, &client_id, Some(&nonce)).await?;
    if saved.is_some_and(|saved| saved.subject != claims.sub)
        || !response.token_type.eq_ignore_ascii_case("bearer")
        || response.access_token.is_empty()
        || response.expires_in <= 0
    {
        return Err(auth_error());
    }
    Ok(Credentials {
        client_id,
        host_id: host_id.to_owned(),
        subject: claims.sub,
        email: claims.email,
        name: claims.name,
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        id_token,
        scope: response.scope,
        expires_at: now_seconds()
            .checked_add(response.expires_in)
            .ok_or_else(auth_error)?,
        earliest_refresh_at: response.earliest_refresh_at,
    })
}

pub async fn credentials(
    resolver: super::transport::SharedSecretResolver,
    secret_id: &str,
    cancel: &CancellationToken,
) -> Result<Credentials, AppError> {
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let secret_id = secret_id.to_owned();
    let task =
        tokio::spawn(async move { refresh_credentials(resolver.as_ref(), &secret_id).await });
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(cancelled()),
        result = task => result.map_err(|_| failure())?,
    }
}

async fn refresh_credentials(
    resolver: &dyn SecretResolver,
    secret_id: &str,
) -> Result<Credentials, AppError> {
    let _lock = REFRESH.lock().await;
    let mut saved: Credentials =
        serde_json::from_str(resolver.resolve(secret_id)?.expose()).map_err(|_| auth_error())?;
    if saved.expires_at > now_seconds() + 60 {
        return Ok(saved);
    }
    if saved
        .earliest_refresh_at
        .is_some_and(|earliest| earliest > now_seconds())
        && saved.expires_at > now_seconds()
    {
        return Ok(saved);
    }
    let http = HttpClient::new()?;
    let response: TokenResponse = serde_json::from_value(
        response_json(http.client().post(TOKEN_URL).form(&[
            ("grant_type", "refresh_token"),
            ("client_id", saved.client_id.as_str()),
            ("refresh_token", saved.refresh_token.as_str()),
            ("resource", API_BASE),
        ]))
        .await?,
    )
    .map_err(|_| auth_error())?;
    if !response.token_type.eq_ignore_ascii_case("bearer")
        || response.access_token.is_empty()
        || response.refresh_token.is_empty()
        || response.expires_in <= 0
    {
        return Err(auth_error());
    }
    if let Some(id_token) = response.id_token {
        let claims = validate_identity(&http, &id_token, &saved.client_id, None).await?;
        if claims.sub != saved.subject {
            return Err(auth_error());
        }
        saved.id_token = id_token;
    }
    saved.access_token = response.access_token;
    saved.refresh_token = response.refresh_token;
    saved.expires_at = now_seconds()
        .checked_add(response.expires_in)
        .ok_or_else(auth_error)?;
    saved.earliest_refresh_at = response.earliest_refresh_at;
    saved.scope = response.scope;
    resolver.replace(
        secret_id,
        Plaintext::new(serde_json::to_string(&saved).map_err(|_| failure())?),
    )?;
    Ok(saved)
}

pub fn permission_error() -> AppError {
    AppError::new(
        AppErrorCode::PermissionDenied,
        "Разрешите использование подписки в настройках ChatGPT",
    )
    .with_detail("reason", json!("subscription-sharing-disabled"))
}

pub async fn revoke(resolver: &dyn SecretResolver, secret_id: &str) -> Result<(), AppError> {
    let _lock = REFRESH.lock().await;
    let saved: Credentials =
        serde_json::from_str(resolver.resolve(secret_id)?.expose()).map_err(|_| auth_error())?;
    let http = HttpClient::new()?;
    let configuration = response_json(
        http.client()
            .get("https://auth.openai.com/.well-known/openid-configuration"),
    )
    .await?;
    let endpoint = configuration
        .get("revocation_endpoint")
        .and_then(Value::as_str)
        .ok_or_else(failure)?;
    let url = Url::parse(endpoint).map_err(|_| failure())?;
    if url.scheme() != "https" || url.host_str() != Some("auth.openai.com") {
        return Err(failure());
    }
    let response = http
        .client()
        .post(url)
        .form(&[
            ("token", saved.refresh_token.as_str()),
            ("token_type_hint", "refresh_token"),
            ("client_id", saved.client_id.as_str()),
        ])
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| failure())?;
    if !response.status().is_success() {
        return Err(failure());
    }
    Ok(())
}
