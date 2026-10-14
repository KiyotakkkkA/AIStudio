use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use url::Url;
use zvs_contract::AppError;

use super::errors::{Headers, request_failed};

pub mod account;
pub mod api;
pub mod session;

pub use account::{
    AccountCredentials, AccountTransport, AccountTransportOptions, SharedAccountCredentials,
};
pub use api::{
    ApiKey, ApiTransport, ApiTransportOptions, DEFAULT_TIMEOUT_SECONDS, SecretResolver,
    SharedSecretResolver, normalise_timeout,
};
pub use session::{
    AccountToken, DEFAULT_TOKEN_TYPE, SessionGateway, SessionRequest, SessionResponse,
    SharedSessionGateway,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestMethod {
    Get,
    Post,
    Delete,
}

impl RequestMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Delete => "DELETE",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequestSpec {
    pub method: RequestMethod,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: Option<Value>,
    pub accept: Option<String>,
}

impl RequestSpec {
    pub fn new(method: RequestMethod, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
            accept: None,
        }
    }

    pub fn get(path: impl Into<String>) -> Self {
        Self::new(RequestMethod::Get, path)
    }

    pub fn post(path: impl Into<String>, body: Value) -> Self {
        Self {
            body: Some(body),
            ..Self::new(RequestMethod::Post, path)
        }
    }

    #[must_use]
    pub fn with_accept(mut self, accept: impl Into<String>) -> Self {
        self.accept = Some(accept.into());
        self
    }

    #[must_use]
    pub fn with_query(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.query.push((key.into(), value.into()));
        self
    }

    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportResponse {
    pub status: u16,
    pub headers: Headers,
    pub text: String,
}

impl TransportResponse {
    pub fn new(status: u16, headers: Headers, text: impl Into<String>) -> Self {
        Self {
            status,
            headers,
            text: text.into(),
        }
    }

    pub fn json<T: DeserializeOwned>(&self) -> Result<T, AppError> {
        serde_json::from_str(&self.text).map_err(|_| request_failed())
    }
}

pub type ByteStream = BoxStream<'static, Result<Vec<u8>, AppError>>;

#[async_trait]
pub trait Transport: Send + Sync {
    fn base_url(&self) -> &str;

    async fn request(
        &self,
        spec: &RequestSpec,
        cancel: &CancellationToken,
    ) -> Result<TransportResponse, AppError>;

    async fn stream(
        &self,
        spec: &RequestSpec,
        cancel: &CancellationToken,
    ) -> Result<ByteStream, AppError>;
}

pub type SharedTransport = Arc<dyn Transport>;

pub fn join_url(base_url: &str, path: &str, query: &[(String, String)]) -> Result<Url, AppError> {
    let base = base_url.trim_end_matches('/');
    let trimmed = path.trim_start_matches('/');
    let joined = if path.is_empty() {
        base.to_owned()
    } else {
        format!("{base}/{trimmed}")
    };
    let mut url = Url::parse(&joined).map_err(|_| request_failed())?;
    for (key, value) in query {
        let kept = url
            .query_pairs()
            .filter(|(existing, _)| existing != key)
            .map(|(existing, current)| (existing.into_owned(), current.into_owned()))
            .collect::<Vec<_>>();
        url.query_pairs_mut()
            .clear()
            .extend_pairs(kept)
            .append_pair(key, value);
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zvs_contract::AppErrorCode;

    fn url(base: &str, path: &str, query: &[(&str, &str)]) -> String {
        let query = query
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect::<Vec<_>>();
        join_url(base, path, &query).expect("joins").to_string()
    }

    #[test]
    fn the_base_url_and_path_are_joined_with_exactly_one_slash() {
        assert_eq!(
            url("https://x.dev/v1/", "/models", &[]),
            "https://x.dev/v1/models"
        );
        assert_eq!(
            url("https://x.dev/v1", "models", &[("a", "1")]),
            "https://x.dev/v1/models?a=1"
        );
        assert_eq!(url("https://x.dev/v1//", "", &[]), "https://x.dev/v1");
        assert_eq!(
            url(
                "https://openrouter.ai/api/v1",
                "../frontend/v1/catalog/models",
                &[]
            ),
            "https://openrouter.ai/api/frontend/v1/catalog/models"
        );
        assert_eq!(
            url("https://x.dev/v1", "models?a=0", &[("a", "1"), ("b", "2")]),
            "https://x.dev/v1/models?a=1&b=2"
        );
        assert_eq!(
            join_url("not a url", "models", &[])
                .expect_err("refuses")
                .code,
            AppErrorCode::Unknown
        );
    }

    #[test]
    fn a_response_body_decodes_as_json_or_fails_as_a_request_failure() {
        let response = TransportResponse::new(200, Headers::new(), r#"{"ok":true}"#);
        assert_eq!(
            response.json::<Value>().expect("decodes"),
            serde_json::json!({ "ok": true })
        );
        assert_eq!(
            TransportResponse::new(200, Headers::new(), "nope")
                .json::<Value>()
                .expect_err("refuses")
                .code,
            AppErrorCode::Unknown
        );
        let spec = RequestSpec::get("/models")
            .with_accept("text/plain")
            .with_header("x-a", "1")
            .with_query("q", "v");
        assert_eq!(spec.method.as_str(), "GET");
        assert_eq!(spec.accept.as_deref(), Some("text/plain"));
    }
}
