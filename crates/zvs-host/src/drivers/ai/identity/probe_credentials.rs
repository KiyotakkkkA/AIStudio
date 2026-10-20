use std::fmt;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, ErrorDetails};

use super::session::{account_family, account_session};
use super::{SharedIdentityProbe, identity_probe, probe_renewing};
use crate::data::schema::AccountRow;
use crate::drivers::ai::errors::{cancelled, is_session_expired, session_expired};
use crate::drivers::ai::transport::{
    AccountCredentials, AccountToken, SharedSecretResolver, SharedSessionGateway,
};

pub struct ProbeAccountCredentials {
    account: AccountRow,
    session: SharedSessionGateway,
    secrets: SharedSecretResolver,
    probe: SharedIdentityProbe,
}

impl fmt::Debug for ProbeAccountCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProbeAccountCredentials")
            .field("account", &self.account.id)
            .field("family", &self.probe.family())
            .finish_non_exhaustive()
    }
}

impl ProbeAccountCredentials {
    pub fn new(
        account: AccountRow,
        session: SharedSessionGateway,
        secrets: SharedSecretResolver,
    ) -> Result<Self, AppError> {
        let probe = identity_probe(account_family(&account.adapter)?);
        Ok(Self::with_probe(account, session, secrets, probe))
    }

    pub fn with_probe(
        account: AccountRow,
        session: SharedSessionGateway,
        secrets: SharedSecretResolver,
        probe: SharedIdentityProbe,
    ) -> Self {
        Self {
            account,
            session,
            secrets,
            probe,
        }
    }

    async fn read(&self, cancel: &CancellationToken) -> Result<Option<AccountToken>, AppError> {
        let session = account_session(&self.account, self.session.clone(), self.secrets.as_ref())?;
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let result = probe_renewing(self.probe.as_ref(), &session, cancel).await?;
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        if result.identity.external_id != self.account.external_id {
            return Err(session_expired(ErrorDetails::new()));
        }
        if let Some(credential) = result.credential {
            return Ok(Some(credential));
        }
        self.account
            .token_secret_id
            .as_deref()
            .map(|secret_id| {
                self.secrets
                    .resolve(secret_id)
                    .map(|token| AccountToken::bearer(token.expose()))
            })
            .transpose()
    }
}

#[async_trait]
impl AccountCredentials for ProbeAccountCredentials {
    async fn current(&self, cancel: &CancellationToken) -> Result<Option<AccountToken>, AppError> {
        self.read(cancel).await
    }

    async fn refresh(&self, cancel: &CancellationToken) -> Result<Option<AccountToken>, AppError> {
        match self.session.renew(self.probe.endpoint(), cancel).await {
            Err(error) if cancel.is_cancelled() || is_session_expired(&error) => return Err(error),
            Ok(_) | Err(_) => {}
        }
        self.read(cancel).await
    }
}
