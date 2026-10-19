use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use futures::{StreamExt, stream};
use tokio_util::sync::CancellationToken;
use url::Url;
use zvs_contract::AppError;

use crate::browser::BROWSER_PARTITION;
use crate::drivers::ai::errors::{Headers, cancelled};
use crate::drivers::ai::transport::{
    AccountToken, ByteStream, SessionGateway, SessionRequest, SessionResponse,
};

pub const FAKE_USER_AGENT: &str = "test-agent";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedSessionRequest {
    pub url: String,
    pub request: SessionRequest,
}

#[derive(Debug, Clone)]
pub enum FakeReply {
    Response {
        status: u16,
        headers: Headers,
        chunks: Vec<Result<Vec<u8>, AppError>>,
        hold: bool,
    },
    Failure(AppError),
    Stall,
}

impl FakeReply {
    pub fn json(status: u16, body: &serde_json::Value) -> Self {
        Self::text(status, "application/json", body.to_string())
    }

    pub fn text(status: u16, content_type: &str, body: impl Into<String>) -> Self {
        let mut headers = Headers::new();
        headers.insert("content-type".to_owned(), content_type.to_owned());
        Self::Response {
            status,
            headers,
            chunks: vec![Ok(body.into().into_bytes())],
            hold: false,
        }
    }

    pub fn chunks(status: u16, headers: Headers, chunks: Vec<Vec<u8>>) -> Self {
        Self::Response {
            status,
            headers,
            chunks: chunks.into_iter().map(Ok).collect(),
            hold: false,
        }
    }

    #[must_use]
    pub fn holding(self) -> Self {
        match self {
            Self::Response {
                status,
                headers,
                chunks,
                ..
            } => Self::Response {
                status,
                headers,
                chunks,
                hold: true,
            },
            other => other,
        }
    }
}

type Responder = Box<dyn Fn(&RecordedSessionRequest) -> FakeReply + Send + Sync>;

pub struct FakeSessionGateway {
    partition: String,
    user_agent: String,
    tokens: Mutex<HashMap<String, AccountToken>>,
    requests: Mutex<Vec<RecordedSessionRequest>>,
    scopes: Mutex<Vec<CancellationToken>>,
    queued: Mutex<HashMap<String, Vec<FakeReply>>>,
    responder: Mutex<Responder>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn origin(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .map(|url| url.origin().ascii_serialization())
}

impl Default for FakeSessionGateway {
    fn default() -> Self {
        Self {
            partition: BROWSER_PARTITION.to_owned(),
            user_agent: FAKE_USER_AGENT.to_owned(),
            tokens: Mutex::default(),
            requests: Mutex::default(),
            scopes: Mutex::default(),
            queued: Mutex::default(),
            responder: Mutex::new(Box::new(|_| FakeReply::json(404, &serde_json::json!({})))),
        }
    }
}

impl FakeSessionGateway {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn respond_with(
        &self,
        responder: impl Fn(&RecordedSessionRequest) -> FakeReply + Send + Sync + 'static,
    ) {
        *lock(&self.responder) = Box::new(responder);
    }

    pub fn queue(&self, url: &str, replies: Vec<FakeReply>) -> &Self {
        lock(&self.queued).insert(url.to_owned(), replies);
        self
    }

    pub fn reset(&self) {
        lock(&self.requests).clear();
        lock(&self.scopes).clear();
        lock(&self.queued).clear();
    }

    pub fn scopes(&self) -> Vec<CancellationToken> {
        lock(&self.scopes).clone()
    }

    pub fn requests_to(&self, url: &str) -> Vec<RecordedSessionRequest> {
        self.requests()
            .into_iter()
            .filter(|recorded| recorded.url == url)
            .collect()
    }

    fn take(&self, recorded: &RecordedSessionRequest) -> FakeReply {
        let queued = {
            let mut queued = lock(&self.queued);
            queued.get_mut(&recorded.url).and_then(|replies| {
                if replies.len() > 1 {
                    Some(replies.remove(0))
                } else {
                    replies.first().cloned()
                }
            })
        };
        queued.unwrap_or_else(|| (lock(&self.responder))(recorded))
    }

    pub fn observe(&self, url: &str, token: AccountToken) {
        if let Some(origin) = origin(url) {
            lock(&self.tokens).insert(origin, token);
        }
    }

    pub fn forget(&self, url: &str) {
        if let Some(origin) = origin(url) {
            lock(&self.tokens).remove(&origin);
        }
    }

    pub fn requests(&self) -> Vec<RecordedSessionRequest> {
        lock(&self.requests).clone()
    }

    pub fn last_request(&self) -> Option<RecordedSessionRequest> {
        lock(&self.requests).last().cloned()
    }
}

#[async_trait]
impl SessionGateway for FakeSessionGateway {
    fn partition(&self) -> &str {
        &self.partition
    }

    fn user_agent(&self) -> String {
        self.user_agent.clone()
    }

    fn account_token(&self, url: &str) -> Option<AccountToken> {
        origin(url).and_then(|origin| lock(&self.tokens).get(&origin).cloned())
    }

    async fn fetch(
        &self,
        url: &str,
        request: SessionRequest,
        cancel: &CancellationToken,
    ) -> Result<SessionResponse, AppError> {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let recorded = RecordedSessionRequest {
            url: url.to_owned(),
            request,
        };
        lock(&self.requests).push(recorded.clone());
        lock(&self.scopes).push(cancel.clone());
        let reply = self.take(&recorded);
        match reply {
            FakeReply::Failure(error) => Err(error),
            FakeReply::Stall => {
                cancel.cancelled().await;
                Err(cancelled())
            }
            FakeReply::Response {
                status,
                headers,
                chunks,
                hold,
            } => {
                let body: ByteStream = if hold {
                    Box::pin(stream::iter(chunks).chain(stream::pending()))
                } else {
                    Box::pin(stream::iter(chunks))
                };
                Ok(SessionResponse::new(status, headers, body))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zvs_contract::AppErrorCode;

    use super::*;

    const QWEN: &str = "https://chat.qwen.ai/api/v1/auths/";

    #[tokio::test]
    async fn the_fake_records_requests_and_answers_from_the_responder() {
        let gateway = FakeSessionGateway::shared();
        gateway.respond_with(|recorded| {
            if recorded.request.header("authorization") == Some("Bearer browser-token") {
                FakeReply::json(200, &json!({ "id": "user-1" }))
            } else {
                FakeReply::json(401, &json!({}))
            }
        });
        let cancel = CancellationToken::new();
        let refused = gateway
            .fetch(QWEN, SessionRequest::new("GET"), &cancel)
            .await
            .expect("answers");
        assert_eq!(refused.status, 401);
        let accepted = gateway
            .fetch(
                QWEN,
                SessionRequest::new("GET").with_header("authorization", "Bearer browser-token"),
                &cancel,
            )
            .await
            .expect("answers");
        assert_eq!(accepted.status, 200);
        assert_eq!(
            accepted.text().await.expect("reads"),
            json!({ "id": "user-1" }).to_string()
        );
        assert_eq!(gateway.requests().len(), 2);
        assert_eq!(gateway.partition(), BROWSER_PARTITION);
        assert_eq!(gateway.user_agent(), FAKE_USER_AGENT);

        cancel.cancel();
        assert_eq!(
            gateway
                .fetch(QWEN, SessionRequest::new("GET"), &cancel)
                .await
                .expect_err("cancelled")
                .code,
            AppErrorCode::RunCancelled
        );
        assert_eq!(gateway.requests().len(), 2);
    }

    #[test]
    fn tokens_are_held_per_origin() {
        let gateway = FakeSessionGateway::default();
        gateway.observe(QWEN, AccountToken::bearer("original"));
        assert_eq!(
            gateway.account_token("https://chat.qwen.ai/other"),
            Some(AccountToken::bearer("original"))
        );
        assert_eq!(
            gateway.account_token("https://chat.qwen.ai.example/api/v1/auths/"),
            None
        );
        assert_eq!(gateway.account_token("not a url"), None);
        gateway.forget(QWEN);
        assert_eq!(gateway.account_token(QWEN), None);
    }
}
