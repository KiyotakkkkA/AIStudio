pub mod probe_credentials;
pub mod qwen;
pub mod session;
pub mod store;

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Map, Value, json};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use url::Url;
use zvs_contract::{AccountFamily, AppError, AppErrorCode, AuthMode, ErrorDetails};

pub use probe_credentials::ProbeAccountCredentials;
pub use qwen::{QWEN_IDENTITY_ENDPOINT, QWEN_LOGIN_URL, QwenIdentityProbe, map_qwen_identity};
pub use session::{StoredTokenSession, account_session};
pub use store::{
    ACCOUNT_TOKEN_SECRET_TYPE, AccountCredentialStore, TOKEN_SKEW_SECONDS, to_identity,
};

use super::errors::{
    HttpFailure, cancelled, http_failure, is_signed_out_response, session_expired, timed_out,
};
use super::transport::{AccountToken, SessionGateway, SessionRequest};

pub const IDENTITY_TIMEOUT_SECONDS: f64 = 20.0;
pub const MALFORMED_IDENTITY_MESSAGE: &str = "Вендор вернул неизвестный формат профиля";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountIdentity {
    pub external_id: String,
    pub email_masked: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub expires_at: Option<i64>,
}

impl AccountIdentity {
    pub fn new(external_id: impl Into<String>) -> Self {
        Self {
            external_id: external_id.into(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    pub identity: AccountIdentity,
    pub credential: Option<AccountToken>,
}

#[async_trait]
pub trait IdentityProbe: Send + Sync {
    fn family(&self) -> AccountFamily;

    fn endpoint(&self) -> &str;

    fn login_url(&self) -> &str;

    async fn probe(
        &self,
        session: &dyn SessionGateway,
        cancel: &CancellationToken,
    ) -> Result<ProbeResult, AppError>;
}

pub type SharedIdentityProbe = Arc<dyn IdentityProbe>;
pub type ProbeFactory = Arc<dyn Fn(AccountFamily) -> SharedIdentityProbe + Send + Sync>;

static QWEN: LazyLock<SharedIdentityProbe> = LazyLock::new(|| Arc::new(QwenIdentityProbe));

pub fn identity_probe(family: AccountFamily) -> SharedIdentityProbe {
    match family {
        AccountFamily::QwenWeb => Arc::clone(&QWEN),
    }
}

pub fn identity_probes() -> ProbeFactory {
    Arc::new(identity_probe)
}

pub fn origin_of(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .map(|url| url.origin().ascii_serialization())
}

pub fn malformed_identity(family: AccountFamily) -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, MALFORMED_IDENTITY_MESSAGE)
        .with_detail("family", family.as_str())
}

pub fn family_details(family: AccountFamily) -> ErrorDetails {
    let mut details = ErrorDetails::new();
    details.insert("family".to_owned(), json!(family.as_str()));
    details
}

pub async fn fetch_identity(
    family: AccountFamily,
    endpoint: &str,
    session: &dyn SessionGateway,
    cancel: &CancellationToken,
    credential: Option<&AccountToken>,
) -> Result<Value, AppError> {
    let origin = origin_of(endpoint).unwrap_or_default();
    let mut request = SessionRequest::new("GET")
        .with_header("accept", "application/json")
        .with_header("user-agent", session.user_agent())
        .with_header("origin", origin.clone())
        .with_header("referer", format!("{origin}/"));
    if let Some(credential) = credential {
        request = request.with_header("authorization", credential.authorization());
    }
    let limit = Duration::from_secs_f64(IDENTITY_TIMEOUT_SECONDS);
    let scope = cancel.child_token();
    let _guard = scope.clone().drop_guard();
    let response = tokio::select! {
        biased;
        () = cancel.cancelled() => return Err(cancelled()),
        () = sleep(limit) => return Err(timed_out(IDENTITY_TIMEOUT_SECONDS)),
        fetched = session.fetch(endpoint, request, &scope) => fetched?,
    };
    let status = response.status;
    let ok = response.ok();
    let headers = response.headers.clone();
    let body = tokio::select! {
        biased;
        () = cancel.cancelled() => return Err(cancelled()),
        () = sleep(limit) => String::new(),
        text = response.text() => text.unwrap_or_default(),
    };
    if is_signed_out_response(status, &headers) {
        let mut details = family_details(family);
        details.insert("status".to_owned(), json!(status));
        details.insert("partition".to_owned(), json!(session.partition()));
        return Err(session_expired(details));
    }
    if !ok {
        return Err(http_failure(HttpFailure {
            headers,
            body: Some(body),
            auth_mode: Some(AuthMode::Account),
            ..HttpFailure::status(status)
        }));
    }
    serde_json::from_str(&body).map_err(|_| malformed_identity(family))
}

pub(crate) struct Payload<'a>(&'a Map<String, Value>);

impl<'a> Payload<'a> {
    pub(crate) fn of(value: &'a Value) -> Option<Self> {
        value.as_object().map(Self)
    }

    pub(crate) fn text(&self, key: &str) -> Result<Option<String>, ()> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(text)) => {
                let trimmed = text.trim();
                Ok((!trimmed.is_empty()).then(|| trimmed.to_owned()))
            }
            Some(_) => Err(()),
        }
    }

    pub(crate) fn id(&self, key: &str) -> Result<String, ()> {
        match self.0.get(key) {
            Some(Value::String(text)) => Ok(text.clone()),
            Some(Value::Number(number)) => Ok(number.to_string()),
            _ => Err(()),
        }
    }

    pub(crate) fn integer(&self, key: &str) -> Result<Option<i64>, ()> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => safe_integer(value).map(Some).ok_or(()),
        }
    }
}

fn safe_integer(value: &Value) -> Option<i64> {
    const MAX_SAFE: i64 = 9_007_199_254_740_991;
    let number = value.as_number()?;
    let whole = number.as_i64().or_else(|| {
        number
            .as_f64()
            .filter(|float| float.fract() == 0.0 && float.abs() <= MAX_SAFE as f64)
            .map(|float| float as i64)
    })?;
    (-MAX_SAFE..=MAX_SAFE).contains(&whole).then_some(whole)
}

#[cfg(test)]
mod tests;
