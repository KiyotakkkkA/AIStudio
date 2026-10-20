use std::fmt;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AccountFamily, AppError, AppErrorCode};

use super::{identity_probe, origin_of};
use crate::data::schema::AccountRow;
use crate::drivers::ai::transport::{
    AccountToken, SecretResolver, SessionGateway, SessionRequest, SessionResponse,
    SharedSessionGateway,
};
use crate::services::crypto::Plaintext;

pub struct StoredTokenSession {
    inner: SharedSessionGateway,
    origin: Option<String>,
    token: Option<Plaintext>,
}

impl fmt::Debug for StoredTokenSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredTokenSession")
            .field("partition", &self.inner.partition())
            .field("origin", &self.origin)
            .field("stored", &self.token.is_some())
            .finish()
    }
}

pub fn account_family(adapter: &str) -> Result<AccountFamily, AppError> {
    AccountFamily::parse(adapter).ok_or_else(|| {
        AppError::new(
            AppErrorCode::ValidationFailed,
            "У семейства нет проверки личности",
        )
        .with_detail("family", adapter)
    })
}

pub fn account_session(
    account: &AccountRow,
    session: SharedSessionGateway,
    secrets: &dyn SecretResolver,
) -> Result<StoredTokenSession, AppError> {
    let family = account_family(&account.adapter)?;
    let origin = origin_of(identity_probe(family).endpoint());
    let token = account
        .token_secret_id
        .as_deref()
        .map(|secret_id| secrets.resolve(secret_id))
        .transpose()?;
    Ok(StoredTokenSession {
        inner: session,
        origin,
        token,
    })
}

#[async_trait]
impl SessionGateway for StoredTokenSession {
    fn partition(&self) -> &str {
        self.inner.partition()
    }

    fn user_agent(&self) -> String {
        self.inner.user_agent()
    }

    fn account_token(&self, url: &str) -> Option<AccountToken> {
        if self.origin.is_none() || origin_of(url) != self.origin {
            return None;
        }
        self.inner.account_token(url).or_else(|| {
            self.token
                .as_ref()
                .map(|token| AccountToken::bearer(token.expose()))
        })
    }

    async fn fetch(
        &self,
        url: &str,
        request: SessionRequest,
        cancel: &CancellationToken,
    ) -> Result<SessionResponse, AppError> {
        self.inner.fetch(url, request, cancel).await
    }

    async fn renew(&self, url: &str, cancel: &CancellationToken) -> Result<bool, AppError> {
        self.inner.renew(url, cancel).await
    }
}
