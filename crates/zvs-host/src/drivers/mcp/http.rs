use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use futures::StreamExt;
use futures::stream::BoxStream;
use reqwest::StatusCode;
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderName, HeaderValue};
use rmcp::model::ClientJsonRpcMessage;
use rmcp::transport::streamable_http_client::{
    SseError, StreamableHttpClient, StreamableHttpError, StreamableHttpPostResponse,
};
use sse_stream::{Sse, SseStream};
use zvs_contract::{AppError, AppErrorCode, McpLogLevel};

use super::oauth::{auth_required, challenge_from};
use super::transport::{MAX_MESSAGE_BYTES, TransportHooks, decode_text};

const SESSION_HEADER: &str = "mcp-session-id";
const LAST_EVENT_HEADER: &str = "last-event-id";
const EVENT_STREAM: &str = "text/event-stream";
const JSON: &str = "application/json";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct HttpFailure {
    pub message: String,
}

impl HttpFailure {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

type Failure = StreamableHttpError<HttpFailure>;

#[derive(Clone)]
pub struct McpHttpClient {
    client: reqwest::Client,
    hooks: TransportHooks,
    failures: Arc<Mutex<Option<AppError>>>,
}

impl McpHttpClient {
    pub const fn new(
        client: reqwest::Client,
        hooks: TransportHooks,
        failures: Arc<Mutex<Option<AppError>>>,
    ) -> Self {
        Self {
            client,
            hooks,
            failures,
        }
    }

    fn fail(&self, message: String) -> Failure {
        self.fail_with(AppError::new(AppErrorCode::McpUnavailable, message))
    }

    fn fail_with(&self, error: AppError) -> Failure {
        let message = error.message.clone();
        *self.failures.lock().unwrap_or_else(PoisonError::into_inner) = Some(error);
        StreamableHttpError::Client(HttpFailure::new(message))
    }

    fn request(
        &self,
        builder: reqwest::RequestBuilder,
        session_id: Option<&str>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> reqwest::RequestBuilder {
        let mut builder = builder;
        for (name, value) in custom_headers {
            builder = builder.header(name, value);
        }
        if let Some(session) = session_id {
            builder = builder.header(SESSION_HEADER, session);
        }
        builder
    }

    async fn send(
        &self,
        uri: &str,
        builder: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, Failure> {
        builder.send().await.map_err(|cause| {
            let reason = format!("{uri}: {cause}");
            let failure = self.fail(format!("MCP-сервер недоступен: {reason}"));
            self.hooks.close(&reason);
            failure
        })
    }

    fn events(&self, response: reqwest::Response) -> BoxStream<'static, Result<Sse, SseError>> {
        let hooks = self.hooks.clone();
        SseStream::from_bytes_stream(response.bytes_stream())
            .filter_map(move |event| {
                let hooks = hooks.clone();
                async move {
                    match event {
                        Ok(event) => {
                            let Some(data) = event.data.as_deref() else {
                                return Some(Ok(event));
                            };
                            let message = decode_text(data, &hooks)?;
                            let data = serde_json::to_string(&message).ok()?;
                            Some(Ok(Sse {
                                data: Some(data),
                                ..event
                            }))
                        }
                        Err(error) => Some(Err(error)),
                    }
                }
            })
            .boxed()
    }
}

fn session_of(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get(SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn content_type(response: &reqwest::Response) -> String {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

impl StreamableHttpClient for McpHttpClient {
    type Error = HttpFailure;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        _auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, Failure> {
        let body = serde_json::to_vec(&message)?;
        let builder = self.request(
            self.client
                .post(uri.as_ref())
                .header(CONTENT_TYPE, JSON)
                .header(ACCEPT, format!("{JSON}, {EVENT_STREAM}"))
                .body(body),
            session_id.as_deref(),
            custom_headers,
        );
        let response = self.send(&uri, builder).await?;
        let session = session_of(&response);
        let status = response.status();
        let challenge = challenge_from(response.headers());
        if status == StatusCode::UNAUTHORIZED && challenge.is_some() {
            return Err(self.fail_with(auth_required(challenge.as_deref())));
        }
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(self.fail(format!(
                "MCP-сервер отклонил учётные данные ({})",
                status.as_u16()
            )));
        }
        if status == StatusCode::NOT_FOUND && session_id.is_some() {
            return Err(StreamableHttpError::SessionExpired);
        }
        if !status.is_success() {
            return Err(self.fail(format!("MCP-сервер ответил ошибкой {}", status.as_u16())));
        }
        if status == StatusCode::ACCEPTED || status == StatusCode::NO_CONTENT {
            return Ok(StreamableHttpPostResponse::Accepted);
        }
        if content_type(&response).contains(EVENT_STREAM) {
            return Ok(StreamableHttpPostResponse::Sse(
                self.events(response),
                session,
            ));
        }
        let body = response
            .text()
            .await
            .map_err(|cause| self.fail(cause.to_string()))?;
        if body.trim().is_empty() {
            return Ok(StreamableHttpPostResponse::Accepted);
        }
        if body.len() > MAX_MESSAGE_BYTES {
            self.hooks.log(
                McpLogLevel::Error,
                "Сервер прислал тело сверх допустимого размера",
            );
            return Ok(StreamableHttpPostResponse::Accepted);
        }
        Ok(decode_text(&body, &self.hooks)
            .map_or(StreamableHttpPostResponse::Accepted, |message| {
                StreamableHttpPostResponse::Json(message, session)
            }))
    }

    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        _auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), Failure> {
        let builder = self.request(
            self.client.delete(uri.as_ref()),
            Some(&session_id),
            custom_headers,
        );
        let _ = builder.send().await;
        Ok(())
    }

    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        _auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, Failure> {
        let mut builder = self.request(
            self.client
                .get(uri.as_ref())
                .header(ACCEPT, format!("{EVENT_STREAM}, {JSON}")),
            session_id.as_deref(),
            custom_headers,
        );
        if let Some(last) = last_event_id {
            builder = builder.header(LAST_EVENT_HEADER, last);
        }
        let Ok(response) = builder.send().await else {
            return Err(StreamableHttpError::ServerDoesNotSupportSse);
        };
        if !response.status().is_success() || !content_type(&response).contains(EVENT_STREAM) {
            return Err(StreamableHttpError::ServerDoesNotSupportSse);
        }
        Ok(self.events(response))
    }
}
