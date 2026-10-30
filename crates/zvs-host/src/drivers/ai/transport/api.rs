use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Method, Response};
use serde_json::json;
use tokio::time::{Instant, sleep_until};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;
use zvs_contract::AppError;

use super::{ByteStream, RequestMethod, RequestSpec, Transport, TransportResponse, join_url};
use crate::drivers::ai::errors::{
    Headers, HttpFailure, cancelled, http_failure, network_failure, request_failed, timed_out,
};
use crate::drivers::ai::http::HttpClient;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};
use crate::services::crypto::Plaintext;
use crate::services::secrets::SecretService;

pub const DEFAULT_TIMEOUT_SECONDS: f64 = 60.0;

const FAR_FUTURE: Duration = Duration::from_secs(60 * 60 * 24 * 365 * 30);

pub trait SecretResolver: Send + Sync {
    fn resolve(&self, id: &str) -> Result<Plaintext, AppError>;

    fn replace(&self, _id: &str, _value: Plaintext) -> Result<(), AppError> {
        Err(crate::drivers::ai::openai::auth_error())
    }
}

impl SecretResolver for SecretService {
    fn resolve(&self, id: &str) -> Result<Plaintext, AppError> {
        Self::resolve(self, id)
    }

    fn replace(&self, id: &str, value: Plaintext) -> Result<(), AppError> {
        self.update(
            id,
            crate::services::secrets::SecretUpdateInput {
                value: Some(value),
                ..Default::default()
            },
        )
        .map(|_| ())
    }
}

pub type SharedSecretResolver = Arc<dyn SecretResolver>;

#[derive(Clone)]
pub struct ApiKey {
    secret_id: String,
    resolver: SharedSecretResolver,
}

impl ApiKey {
    pub fn new(secret_id: impl Into<String>, resolver: SharedSecretResolver) -> Self {
        Self {
            secret_id: secret_id.into(),
            resolver,
        }
    }

    pub fn secret_id(&self) -> &str {
        &self.secret_id
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApiKey")
            .field("secret_id", &self.secret_id)
            .finish_non_exhaustive()
    }
}

pub fn normalise_timeout(seconds: Option<f64>) -> f64 {
    match seconds {
        Some(seconds) if seconds.is_finite() && seconds > 0.0 => seconds,
        _ => DEFAULT_TIMEOUT_SECONDS,
    }
}

pub struct ApiTransportOptions {
    pub base_url: String,
    pub api_key: Option<ApiKey>,
    pub timeout_seconds: Option<f64>,
    pub headers: Vec<(String, String)>,
    pub http: HttpClient,
    pub logger: SharedLogger,
}

impl ApiTransportOptions {
    pub fn new(base_url: impl Into<String>, http: HttpClient) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: None,
            timeout_seconds: None,
            headers: Vec::new(),
            http,
            logger: silent_logger(),
        }
    }
}

pub struct ApiTransport {
    base_url: String,
    api_key: Option<ApiKey>,
    timeout_seconds: f64,
    headers: Vec<(String, String)>,
    http: HttpClient,
    logger: SharedLogger,
}

impl fmt::Debug for ApiTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApiTransport")
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key)
            .field("timeout_seconds", &self.timeout_seconds)
            .finish_non_exhaustive()
    }
}

impl ApiTransport {
    pub fn new(options: ApiTransportOptions) -> Self {
        Self {
            base_url: options.base_url,
            api_key: options.api_key,
            timeout_seconds: normalise_timeout(options.timeout_seconds),
            headers: options.headers,
            http: options.http,
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

    fn authorization(&self) -> Result<Option<HeaderValue>, AppError> {
        let Some(api_key) = &self.api_key else {
            return Ok(None);
        };
        let plaintext = api_key.resolver.resolve(&api_key.secret_id)?;
        let key = plaintext.expose().trim();
        if key.is_empty() {
            return Ok(None);
        }
        let bearer = Zeroizing::new(format!("Bearer {key}"));
        let mut value = HeaderValue::from_str(&bearer).map_err(|_| request_failed())?;
        value.set_sensitive(true);
        Ok(Some(value))
    }

    fn headers_for(&self, spec: &RequestSpec, accept: &str) -> Result<HeaderMap, AppError> {
        let mut headers = HeaderMap::new();
        let mut set = |name: &str, value: &str| -> Result<(), AppError> {
            let name = HeaderName::from_bytes(name.to_ascii_lowercase().as_bytes())
                .map_err(|_| request_failed())?;
            let value = HeaderValue::from_str(value).map_err(|_| request_failed())?;
            headers.insert(name, value);
            Ok(())
        };
        set("accept", spec.accept.as_deref().unwrap_or(accept))?;
        for (name, value) in self.headers.iter().chain(&spec.headers) {
            set(name, value)?;
        }
        if spec.body.is_some() {
            set("content-type", "application/json")?;
        }
        if let Some(authorization) = self.authorization()? {
            headers.insert(reqwest::header::AUTHORIZATION, authorization);
        }
        Ok(headers)
    }

    async fn send(
        &self,
        spec: &RequestSpec,
        accept: &str,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Response, AppError> {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let url = join_url(&self.base_url, &spec.path, &spec.query)?;
        let method = match spec.method {
            RequestMethod::Get => Method::GET,
            RequestMethod::Post => Method::POST,
            RequestMethod::Delete => Method::DELETE,
        };
        let mut request = self
            .http
            .client()
            .request(method, url)
            .headers(self.headers_for(spec, accept)?);
        if let Some(body) = &spec.body {
            request = request.body(serde_json::to_vec(body).map_err(|_| request_failed())?);
        }
        tokio::select! {
            biased;
            () = cancel.cancelled() => Err(cancelled()),
            () = sleep_until(deadline) => Err(self.abort_reason(cancel)),
            sent = request.send() => sent.map_err(|error| {
                if cancel.is_cancelled() {
                    cancelled()
                } else {
                    network_failure(&error)
                }
            }),
        }
    }

    async fn read(
        &self,
        response: Response,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<String, AppError> {
        let status = response.status().as_u16();
        tokio::select! {
            biased;
            () = cancel.cancelled() => Err(cancelled()),
            () = sleep_until(deadline) => Err(self.abort_reason(cancel)),
            body = response.bytes() => match body {
                Ok(bytes) => Ok(String::from_utf8_lossy(&bytes).into_owned()),
                Err(error) => {
                    self.logger.log(
                        LogLevel::Debug,
                        "ai",
                        "Could not read the provider response body",
                        &[("status", json!(status))],
                    );
                    Err(network_failure(&error))
                }
            },
        }
    }
}

fn collect_headers(headers: &HeaderMap) -> Headers {
    let mut collected = Headers::new();
    for (name, value) in headers {
        let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
        collected
            .entry(name.as_str().to_ascii_lowercase())
            .and_modify(|existing| {
                existing.push_str(", ");
                existing.push_str(&value);
            })
            .or_insert(value);
    }
    collected
}

struct Pump {
    body: BoxStream<'static, reqwest::Result<Vec<u8>>>,
    cancel: CancellationToken,
    deadline: Instant,
    timeout_seconds: f64,
    finished: bool,
}

#[async_trait]
impl Transport for ApiTransport {
    fn base_url(&self) -> &str {
        &self.base_url
    }

    async fn request(
        &self,
        spec: &RequestSpec,
        cancel: &CancellationToken,
    ) -> Result<TransportResponse, AppError> {
        let deadline = self.deadline();
        let response = self
            .send(spec, "application/json", cancel, deadline)
            .await?;
        let status = response.status();
        let headers = collect_headers(response.headers());
        let text = self.read(response, cancel, deadline).await?;
        if !status.is_success() {
            return Err(http_failure(HttpFailure {
                headers,
                body: Some(text),
                ..HttpFailure::status(status.as_u16())
            }));
        }
        Ok(TransportResponse::new(status.as_u16(), headers, text))
    }

    async fn stream(
        &self,
        spec: &RequestSpec,
        cancel: &CancellationToken,
    ) -> Result<ByteStream, AppError> {
        let deadline = self.deadline();
        let response = self
            .send(spec, "text/event-stream", cancel, deadline)
            .await?;
        let status = response.status();
        if !status.is_success() {
            let headers = collect_headers(response.headers());
            let text = self.read(response, cancel, deadline).await?;
            return Err(http_failure(HttpFailure {
                headers,
                body: Some(text),
                ..HttpFailure::status(status.as_u16())
            }));
        }
        let pump = Pump {
            body: response
                .bytes_stream()
                .map(|chunk| chunk.map(|bytes| bytes.to_vec()))
                .boxed(),
            cancel: cancel.clone(),
            deadline,
            timeout_seconds: self.timeout_seconds,
            finished: false,
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
                            network_failure(&error)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::ai::errors::TIMED_OUT_MESSAGE;
    use crate::drivers::ai::mock::{CountingKey, MockServer, Reply, direct_http};
    use serde_json::Value;
    use zvs_contract::AppErrorCode;

    struct MissingKey;

    impl SecretResolver for MissingKey {
        fn resolve(&self, id: &str) -> Result<Plaintext, AppError> {
            Err(AppError::new(
                AppErrorCode::SecretMissing,
                "У секрета нет сохранённого значения",
            )
            .with_detail("secretId", id))
        }
    }

    fn transport(
        base_url: String,
        configure: impl FnOnce(&mut ApiTransportOptions),
    ) -> ApiTransport {
        let mut options = ApiTransportOptions::new(base_url, direct_http());
        configure(&mut options);
        ApiTransport::new(options)
    }

    fn header(response: &TransportResponse, name: &str) -> Option<String> {
        response.headers.get(name).cloned()
    }

    #[tokio::test]
    async fn the_api_transport_joins_the_base_url_sends_the_bearer_header_and_reads_json() {
        let server = MockServer::start().await;
        server.reply(
            "/v1/models",
            Reply::json(json!({ "ok": true })).with_header("X-Trace", "1"),
        );
        let key = CountingKey::shared("  sk-key  ");
        let api = transport(server.url("/v1/"), |options| {
            options.api_key = Some(ApiKey::new("secret-1", key.clone()));
            options.timeout_seconds = Some(30.0);
        });
        assert_eq!(key.calls(), 0);
        let response = api
            .request(
                &RequestSpec::post("models", json!({ "a": 1 })),
                &CancellationToken::new(),
            )
            .await
            .expect("answers");
        assert_eq!(response.status, 200);
        assert_eq!(
            response.json::<Value>().expect("json"),
            json!({ "ok": true })
        );
        assert_eq!(header(&response, "x-trace").as_deref(), Some("1"));
        let call = server.last();
        assert_eq!(call.method, "POST");
        assert_eq!(call.path, "/v1/models");
        assert_eq!(
            call.header("authorization").as_deref(),
            Some("Bearer sk-key")
        );
        assert_eq!(
            call.header("content-type").as_deref(),
            Some("application/json")
        );
        assert_eq!(call.header("accept").as_deref(), Some("application/json"));
        assert_eq!(String::from_utf8_lossy(&call.body), r#"{"a":1}"#);
        assert_eq!(key.calls(), 1);
        assert_eq!(api.timeout_seconds(), 30.0);
    }

    #[tokio::test]
    async fn the_secret_is_resolved_on_every_request_and_never_kept_on_the_transport() {
        let server = MockServer::start().await;
        server.reply("/v1/models", Reply::json(json!({})));
        let key = CountingKey::shared("sk-or-v1-abcdefghijklmnopqrstuvwxyz");
        let api = transport(server.url("/v1"), |options| {
            options.api_key = Some(ApiKey::new("secret-1", key.clone()));
        });
        for expected in 1..=3 {
            api.request(&RequestSpec::get("models"), &CancellationToken::new())
                .await
                .expect("answers");
            assert_eq!(key.calls(), expected);
        }
        let printed = format!("{api:?}");
        assert!(printed.contains("secret-1"), "{printed}");
        assert!(!printed.contains("sk-or-v1"), "{printed}");
    }

    #[tokio::test]
    async fn no_authorization_header_is_sent_without_a_secret_or_with_a_blank_one() {
        let server = MockServer::start().await;
        server.reply("/v1/models", Reply::json(json!({})));
        let bare = transport(server.url("/v1"), |_| {});
        bare.request(&RequestSpec::get("models"), &CancellationToken::new())
            .await
            .expect("answers");
        assert_eq!(server.last().header("authorization"), None);
        let blank = transport(server.url("/v1"), |options| {
            options.api_key = Some(ApiKey::new("secret-1", CountingKey::shared("   ")));
        });
        blank
            .request(&RequestSpec::get("models"), &CancellationToken::new())
            .await
            .expect("answers");
        assert_eq!(server.last().header("authorization"), None);
    }

    #[tokio::test]
    async fn a_secret_that_cannot_be_resolved_fails_the_call_before_anything_is_sent() {
        let server = MockServer::start().await;
        server.reply("/v1/models", Reply::json(json!({})));
        let api = transport(server.url("/v1"), |options| {
            options.api_key = Some(ApiKey::new("secret-9", Arc::new(MissingKey)));
        });
        let error = api
            .request(&RequestSpec::get("models"), &CancellationToken::new())
            .await
            .expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::SecretMissing);
        assert!(server.calls().is_empty());
    }

    #[tokio::test]
    async fn extra_headers_apply_and_the_spec_wins_over_the_defaults() {
        let server = MockServer::start().await;
        server.reply("/v1/models", Reply::json(json!({})));
        let api = transport(server.url("/v1"), |options| {
            options.headers = vec![("X-Client".to_owned(), "studio".to_owned())];
        });
        api.request(
            &RequestSpec::get("models")
                .with_header("x-client", "override")
                .with_accept("text/plain")
                .with_query("page", "2"),
            &CancellationToken::new(),
        )
        .await
        .expect("answers");
        let call = server.last();
        assert_eq!(call.header("x-client").as_deref(), Some("override"));
        assert_eq!(call.header("accept").as_deref(), Some("text/plain"));
        assert_eq!(call.query.as_deref(), Some("page=2"));
    }

    #[tokio::test]
    async fn a_slow_provider_times_out_as_unreachable_with_the_timeout_named() {
        let server = MockServer::start().await;
        server.reply(
            "/v1/models",
            Reply::json(json!({})).with_delay(Duration::from_secs(5)),
        );
        let api = transport(server.url("/v1"), |options| {
            options.timeout_seconds = Some(0.05);
        });
        let error = api
            .request(&RequestSpec::get("models"), &CancellationToken::new())
            .await
            .expect_err("times out");
        assert_eq!(error.code, AppErrorCode::ProviderUnreachable);
        assert_eq!(error.message, TIMED_OUT_MESSAGE);
        assert_eq!(
            error
                .details
                .and_then(|details| details.get("timeoutSeconds").cloned()),
            Some(json!(0.05))
        );
    }

    #[test]
    fn the_timeout_falls_back_to_sixty_seconds() {
        for seconds in [
            None,
            Some(0.0),
            Some(-1.0),
            Some(f64::NAN),
            Some(f64::INFINITY),
        ] {
            assert_eq!(
                normalise_timeout(seconds),
                DEFAULT_TIMEOUT_SECONDS,
                "{seconds:?}"
            );
        }
        assert_eq!(normalise_timeout(Some(15.0)), 15.0);
    }

    #[tokio::test]
    async fn a_cancelled_request_is_run_cancelled_and_never_reaches_the_vendor_when_already_cancelled()
     {
        let server = MockServer::start().await;
        server.reply(
            "/v1/models",
            Reply::json(json!({})).with_delay(Duration::from_secs(5)),
        );
        let api = transport(server.url("/v1"), |_| {});
        let cancel = CancellationToken::new();
        cancel.cancel();
        let early = api
            .request(&RequestSpec::get("models"), &cancel)
            .await
            .expect_err("cancelled");
        assert_eq!(early.code, AppErrorCode::RunCancelled);
        assert!(server.calls().is_empty());

        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            trigger.cancel();
        });
        let late = api
            .request(&RequestSpec::get("models"), &cancel)
            .await
            .expect_err("cancelled");
        assert_eq!(late.code, AppErrorCode::RunCancelled);
    }

    #[tokio::test]
    async fn a_non_2xx_response_becomes_the_mapped_error_with_the_vendor_message() {
        let server = MockServer::start().await;
        server.reply(
            "/v1/models",
            Reply::json(json!({ "error": "bad key" })).with_status(401),
        );
        server.reply(
            "/v1/chat/completions",
            Reply::chunks(Vec::<String>::new())
                .with_status(429)
                .with_header("retry-after", "5"),
        );
        let api = transport(server.url("/v1"), |_| {});
        let error = api
            .request(&RequestSpec::get("models"), &CancellationToken::new())
            .await
            .expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::ProviderAuthFailed);
        assert_eq!(
            error
                .details
                .and_then(|details| details.get("vendorMessage").cloned()),
            Some(json!("bad key"))
        );
        let limited = api
            .stream(
                &RequestSpec::post("chat/completions", json!({})),
                &CancellationToken::new(),
            )
            .await
            .err()
            .expect("refuses");
        assert_eq!(limited.code, AppErrorCode::RateLimited);
        assert_eq!(
            limited
                .details
                .and_then(|details| details.get("retryAfter").cloned()),
            Some(json!(5))
        );
        assert_eq!(
            server.last().header("accept").as_deref(),
            Some("text/event-stream")
        );
    }

    #[tokio::test]
    async fn a_refused_connection_is_unreachable() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let address = listener.local_addr().expect("an address");
        drop(listener);
        let api = transport(format!("http://{address}/v1"), |_| {});
        let error = api
            .request(&RequestSpec::get("models"), &CancellationToken::new())
            .await
            .expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::ProviderUnreachable);
    }

    #[tokio::test]
    async fn the_stream_yields_chunks_until_the_body_ends() {
        let server = MockServer::start().await;
        server.reply("/v1/stream", Reply::chunks(["a", "b", "c"]));
        let api = transport(server.url("/v1"), |_| {});
        let chunks = api
            .stream(&RequestSpec::get("stream"), &CancellationToken::new())
            .await
            .expect("streams")
            .map(|chunk| chunk.expect("a chunk"))
            .collect::<Vec<_>>()
            .await
            .concat();
        assert_eq!(chunks, b"abc");
    }

    #[tokio::test]
    async fn cancelling_a_stream_ends_it_as_cancelled_and_aborts_the_vendor_request() {
        let server = MockServer::start().await;
        server.reply("/v1/stream", Reply::chunks(["first"]).holding());
        let api = transport(server.url("/v1"), |_| {});
        let cancel = CancellationToken::new();
        let mut stream = api
            .stream(&RequestSpec::get("stream"), &cancel)
            .await
            .expect("streams");
        assert_eq!(
            stream.next().await.expect("a chunk").expect("data"),
            b"first"
        );
        cancel.cancel();
        let ended = stream
            .next()
            .await
            .expect("an outcome")
            .expect_err("cancelled");
        assert_eq!(ended.code, AppErrorCode::RunCancelled);
        assert!(stream.next().await.is_none());
        drop(stream);
        assert!(server.wait_for_dropped_stream().await);
    }

    #[tokio::test]
    async fn the_deadline_covers_the_whole_stream() {
        let server = MockServer::start().await;
        server.reply("/v1/stream", Reply::chunks(["first"]).holding());
        let api = transport(server.url("/v1"), |options| {
            options.timeout_seconds = Some(0.2);
        });
        let mut stream = api
            .stream(&RequestSpec::get("stream"), &CancellationToken::new())
            .await
            .expect("streams");
        assert!(stream.next().await.expect("a chunk").is_ok());
        let ended = stream
            .next()
            .await
            .expect("an outcome")
            .expect_err("times out");
        assert_eq!(ended.code, AppErrorCode::ProviderUnreachable);
        assert_eq!(ended.message, TIMED_OUT_MESSAGE);
        assert!(stream.next().await.is_none());
    }
}
