use std::path::Path;
use std::time::Duration;

use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use zvs_contract::{AppError, AppErrorCode};

use super::copy;
use crate::services::crypto::Plaintext;

pub const DEFAULT_BASE: &str = "https://api.telegram.org";
pub const LONG_POLL_SECONDS: u64 = 50;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
pub const FILE_MAX_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    TokenRejected,
    RateLimited(u64),
    Conflict,
    Unreachable,
    Refused(String),
}

impl ApiError {
    pub fn into_app(self) -> AppError {
        match self {
            Self::TokenRejected => {
                AppError::new(AppErrorCode::ProviderAuthFailed, copy::TOKEN_REJECTED)
            }
            Self::RateLimited(seconds) => {
                AppError::new(AppErrorCode::RateLimited, copy::rate_limited(seconds))
                    .with_detail("retryAfter", seconds.to_string())
            }
            Self::Conflict => AppError::new(AppErrorCode::Conflict, copy::POLL_CONFLICT),
            Self::Unreachable => {
                AppError::new(AppErrorCode::ProviderUnreachable, copy::UNREACHABLE)
            }
            Self::Refused(description) => {
                AppError::new(AppErrorCode::ValidationFailed, copy::API_FAILED)
                    .with_detail("description", description)
            }
        }
    }
}

#[derive(Deserialize)]
struct Envelope {
    ok: bool,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    error_code: Option<u16>,
    #[serde(default)]
    parameters: Option<Parameters>,
}

#[derive(Deserialize)]
struct Parameters {
    #[serde(default)]
    retry_after: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Bot {
    pub id: i64,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub first_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Chat {
    pub id: i64,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct User {
    pub id: i64,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Document {
    pub file_id: String,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub file_size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReplyTo {
    pub message_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Message {
    pub message_id: i64,
    pub date: i64,
    pub chat: Chat,
    #[serde(default)]
    pub from: Option<User>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub caption: Option<String>,
    #[serde(default)]
    pub document: Option<Document>,
    #[serde(default)]
    pub reply_to_message: Option<Box<ReplyTo>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Update {
    pub update_id: i64,
    #[serde(default)]
    pub message: Option<Message>,
    #[serde(default)]
    pub channel_post: Option<Message>,
}

impl Update {
    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref().or(self.channel_post.as_ref())
    }
}

pub struct BotApi {
    http: reqwest::Client,
    base: String,
    token: Plaintext,
}

impl BotApi {
    pub fn new(http: reqwest::Client, base: &str, token: Plaintext) -> Self {
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
            token,
        }
    }

    fn url(&self, method: &str) -> String {
        format!("{}/bot{}/{method}", self.base, self.token.expose())
    }

    async fn read<T: DeserializeOwned>(
        response: Result<reqwest::Response, reqwest::Error>,
    ) -> Result<T, ApiError> {
        let response = response.map_err(|_| ApiError::Unreachable)?;
        let status = response.status().as_u16();
        let envelope: Envelope = response.json().await.map_err(|_| {
            if status == 401 || status == 404 {
                ApiError::TokenRejected
            } else {
                ApiError::Unreachable
            }
        })?;
        if envelope.ok {
            return serde_json::from_value(envelope.result.unwrap_or(Value::Null))
                .map_err(|_| ApiError::Refused(copy::UNEXPECTED_REPLY.to_owned()));
        }
        Err(match envelope.error_code.unwrap_or(status) {
            401 | 404 => ApiError::TokenRejected,
            409 => ApiError::Conflict,
            429 => ApiError::RateLimited(
                envelope
                    .parameters
                    .and_then(|parameters| parameters.retry_after)
                    .unwrap_or(1),
            ),
            _ => ApiError::Refused(envelope.description.unwrap_or_default()),
        })
    }

    async fn call<T: DeserializeOwned>(
        &self,
        method: &str,
        body: Value,
        timeout: Duration,
    ) -> Result<T, ApiError> {
        Self::read(
            self.http
                .post(self.url(method))
                .timeout(timeout)
                .json(&body)
                .send()
                .await,
        )
        .await
    }

    pub async fn get_me(&self) -> Result<Bot, ApiError> {
        self.call("getMe", json!({}), REQUEST_TIMEOUT).await
    }

    pub async fn get_updates(
        &self,
        offset: Option<i64>,
        timeout: u64,
    ) -> Result<Vec<Update>, ApiError> {
        let mut body = json!({
            "timeout": timeout,
            "allowed_updates": ["message", "channel_post"],
        });
        if let Some(offset) = offset {
            body["offset"] = json!(offset);
        }
        self.call(
            "getUpdates",
            body,
            Duration::from_secs(timeout) + REQUEST_TIMEOUT,
        )
        .await
    }

    pub async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
        reply_to: Option<i64>,
    ) -> Result<Message, ApiError> {
        let mut body = json!({ "chat_id": chat_id, "text": text });
        if let Some(reply_to) = reply_to {
            body["reply_parameters"] =
                json!({ "message_id": reply_to, "allow_sending_without_reply": true });
        }
        self.call("sendMessage", body, REQUEST_TIMEOUT).await
    }

    pub async fn send_document(
        &self,
        chat_id: &str,
        path: &Path,
        caption: Option<&str>,
    ) -> Result<Message, ApiError> {
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|_| ApiError::Refused(copy::FILE_MISSING.to_owned()))?;
        let name = path.file_name().map_or_else(
            || "file".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        let mut form = Form::new()
            .text("chat_id", chat_id.to_owned())
            .part("document", Part::bytes(bytes).file_name(name));
        if let Some(caption) = caption {
            form = form.text("caption", caption.to_owned());
        }
        Self::read(
            self.http
                .post(self.url("sendDocument"))
                .timeout(REQUEST_TIMEOUT * 4)
                .multipart(form)
                .send()
                .await,
        )
        .await
    }
}
