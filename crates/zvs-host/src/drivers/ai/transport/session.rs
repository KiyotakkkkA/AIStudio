use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use zvs_contract::AppError;

use super::ByteStream;
use crate::drivers::ai::errors::Headers;

pub const DEFAULT_TOKEN_TYPE: &str = "Bearer";

#[derive(Clone, PartialEq, Eq)]
pub struct AccountToken {
    pub token: String,
    pub token_type: String,
}

impl AccountToken {
    pub fn bearer(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            token_type: DEFAULT_TOKEN_TYPE.to_owned(),
        }
    }

    pub fn authorization(&self) -> String {
        format!("{} {}", self.token_type, self.token)
    }
}

impl fmt::Debug for AccountToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AccountToken")
            .field("token_type", &self.token_type)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct SessionRequest {
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl SessionRequest {
    pub fn new(method: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    #[must_use]
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .rev()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

impl fmt::Debug for SessionRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names = self
            .headers
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>();
        formatter
            .debug_struct("SessionRequest")
            .field("method", &self.method)
            .field("headers", &names)
            .field("body", &self.body.as_ref().map(String::len))
            .finish()
    }
}

pub struct SessionResponse {
    pub status: u16,
    pub headers: Headers,
    pub final_url: Option<String>,
    pub redirected: bool,
    pub body: ByteStream,
}

impl SessionResponse {
    pub fn new(status: u16, headers: Headers, body: ByteStream) -> Self {
        Self {
            status,
            headers,
            final_url: None,
            redirected: false,
            body,
        }
    }

    #[must_use]
    pub fn with_response_url(mut self, final_url: Option<String>, redirected: bool) -> Self {
        self.final_url = final_url;
        self.redirected = redirected;
        self
    }

    pub const fn ok(&self) -> bool {
        self.status >= 200 && self.status < 300
    }

    pub async fn bytes(self) -> Result<Vec<u8>, AppError> {
        let mut body = self.body;
        let mut collected = Vec::new();
        while let Some(chunk) = body.next().await {
            collected.extend_from_slice(&chunk?);
        }
        Ok(collected)
    }

    pub async fn text(self) -> Result<String, AppError> {
        let bytes = self.bytes().await?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

impl fmt::Debug for SessionResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SessionResponse")
            .field("status", &self.status)
            .field("redirected", &self.redirected)
            .field("headers", &self.headers)
            .finish_non_exhaustive()
    }
}

#[async_trait]
pub trait SessionGateway: Send + Sync {
    fn partition(&self) -> &str;

    fn user_agent(&self) -> String;

    fn account_token(&self, url: &str) -> Option<AccountToken>;

    async fn fetch(
        &self,
        url: &str,
        request: SessionRequest,
        cancel: &CancellationToken,
    ) -> Result<SessionResponse, AppError>;

    async fn renew(&self, _url: &str, _cancel: &CancellationToken) -> Result<bool, AppError> {
        Ok(false)
    }
}

pub type SharedSessionGateway = Arc<dyn SessionGateway>;

#[cfg(test)]
mod tests {
    use futures::stream;

    use super::*;

    #[test]
    fn a_token_never_prints_its_value() {
        let token = AccountToken::bearer("secret-value");
        assert_eq!(token.authorization(), "Bearer secret-value");
        assert!(!format!("{token:?}").contains("secret-value"));
    }

    #[test]
    fn a_request_prints_header_names_only_and_finds_headers_case_insensitively() {
        let request = SessionRequest::new("POST")
            .with_header("Authorization", "Bearer secret-value")
            .with_header("authorization", "Bearer newer")
            .with_body("{\"a\":1}");
        let printed = format!("{request:?}");
        assert!(!printed.contains("secret-value"));
        assert!(printed.contains("Authorization"));
        assert_eq!(request.header("AUTHORIZATION"), Some("Bearer newer"));
        assert_eq!(request.header("cookie"), None);
    }

    #[test]
    fn response_metadata_defaults_and_keeps_the_url_out_of_debug() {
        let response = SessionResponse::new(200, Headers::new(), Box::pin(stream::empty()));
        assert_eq!(response.final_url, None);
        assert!(!response.redirected);
        let response = response.with_response_url(
            Some("https://chat.qwen.ai/login?token=secret-value".to_owned()),
            true,
        );
        assert!(response.redirected);
        assert!(!format!("{response:?}").contains("secret-value"));
        assert!(response.final_url.as_deref().unwrap().contains("/login"));
    }

    #[tokio::test]
    async fn a_response_body_collects_every_chunk_and_stops_at_the_first_error() {
        let body: ByteStream = Box::pin(stream::iter(vec![
            Ok(b"hel".to_vec()),
            Ok("lo ✓".as_bytes().to_vec()),
        ]));
        let response = SessionResponse::new(200, Headers::new(), body);
        assert!(response.ok());
        assert_eq!(response.text().await.expect("reads"), "hello ✓");

        let failing: ByteStream = Box::pin(stream::iter(vec![
            Ok(b"partial".to_vec()),
            Err(crate::drivers::ai::errors::cancelled()),
        ]));
        let response = SessionResponse::new(502, Headers::new(), failing);
        assert!(!response.ok());
        assert_eq!(
            response.bytes().await.expect_err("fails").code,
            zvs_contract::AppErrorCode::RunCancelled
        );
    }
}
