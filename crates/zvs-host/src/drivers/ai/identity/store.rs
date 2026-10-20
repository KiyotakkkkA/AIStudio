use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AccountStatus, AppError, AppErrorCode, ErrorDetails, SecretScope};

use super::session::account_family;
use super::{AccountIdentity, ProbeFactory, ProbeResult, identity_probes, probe_renewing};
use crate::data::repositories::AccountPatch;
use crate::data::schema::AccountRow;
use crate::data::{Database, Repositories};
use crate::drivers::ai::errors::{is_session_expired, session_expired};
use crate::drivers::ai::transport::{AccountCredentials, AccountToken, SharedSessionGateway};
use crate::platform::clock::SharedClock;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};
use crate::services::crypto::Plaintext;
use crate::services::secrets::{SecretCreateInput, SecretService, SecretUpdateInput};

pub const ACCOUNT_TOKEN_SECRET_TYPE: &str = "account-token";
pub const TOKEN_SKEW_SECONDS: i64 = 60;
pub const IDENTITY_MISMATCH_DETAIL: &str = "В браузере выполнен вход под другим аккаунтом";

pub struct AccountCredentialStoreOptions {
    pub account_id: String,
    pub database: Arc<Database>,
    pub secrets: Arc<SecretService>,
    pub session: SharedSessionGateway,
    pub probes: ProbeFactory,
    pub clock: SharedClock,
    pub logger: SharedLogger,
}

impl AccountCredentialStoreOptions {
    pub fn new(
        account_id: impl Into<String>,
        database: Arc<Database>,
        secrets: Arc<SecretService>,
        session: SharedSessionGateway,
        clock: SharedClock,
    ) -> Self {
        Self {
            account_id: account_id.into(),
            database,
            secrets,
            session,
            probes: identity_probes(),
            clock,
            logger: silent_logger(),
        }
    }
}

pub struct AccountCredentialStore {
    account_id: String,
    database: Arc<Database>,
    secrets: Arc<SecretService>,
    session: SharedSessionGateway,
    probes: ProbeFactory,
    clock: SharedClock,
    logger: SharedLogger,
}

impl fmt::Debug for AccountCredentialStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AccountCredentialStore")
            .field("account_id", &self.account_id)
            .finish_non_exhaustive()
    }
}

pub fn to_identity(row: &AccountRow) -> AccountIdentity {
    AccountIdentity {
        external_id: row.external_id.clone(),
        email_masked: row.email_masked.clone(),
        display_name: row.display_name.clone(),
        avatar_url: row.avatar_url.clone(),
        expires_at: row.token_expires_at,
    }
}

impl AccountCredentialStore {
    pub fn new(options: AccountCredentialStoreOptions) -> Self {
        Self {
            account_id: options.account_id,
            database: options.database,
            secrets: options.secrets,
            session: options.session,
            probes: options.probes,
            clock: options.clock,
            logger: options.logger,
        }
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub async fn probe(&self, cancel: &CancellationToken) -> Result<AccountIdentity, AppError> {
        self.refresh_token(cancel).await?;
        Ok(to_identity(&self.row()?))
    }

    pub fn unlink(&self) -> Result<(), AppError> {
        let now = self.clock.now();
        let removal = self.database.transaction(|transaction| {
            Ok(Repositories::new(transaction)
                .accounts
                .remove(&self.account_id, now)?)
        })?;
        if let Some(secret_id) = &removal.freed_secret_id {
            self.secrets.remove(secret_id)?;
        }
        self.logger.log(
            LogLevel::Info,
            "ai",
            "Unlinked an account",
            &[
                ("accountId", json!(self.account_id)),
                (
                    "detachedProviders",
                    json!(removal.detached_provider_ids.len()),
                ),
                ("deletedToken", json!(removal.freed_secret_id.is_some())),
            ],
        );
        Ok(())
    }

    async fn current_token(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Option<AccountToken>, AppError> {
        let row = self.row()?;
        if !row.is_linked() {
            let mut details = ErrorDetails::new();
            details.insert("accountId".to_owned(), json!(row.id));
            details.insert("status".to_owned(), json!(row.status));
            return Err(session_expired(details));
        }
        let Some(secret_id) = &row.token_secret_id else {
            return Ok(None);
        };
        if self.stale(&row) {
            return self.refresh_token(cancel).await;
        }
        let token = self.secrets.resolve(secret_id)?;
        Ok(Some(AccountToken::bearer(token.expose())))
    }

    async fn refresh_token(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Option<AccountToken>, AppError> {
        let row = self.row()?;
        let probe = (self.probes)(account_family(&row.adapter)?);
        let result = match probe_renewing(probe.as_ref(), self.session.as_ref(), cancel).await {
            Ok(result) => result,
            Err(error) => {
                if is_session_expired(&error) {
                    self.mark_needs_relink(&row, &error.message)?;
                }
                return Err(error);
            }
        };
        if result.identity.external_id != row.external_id {
            self.mark_needs_relink(&row, IDENTITY_MISMATCH_DETAIL)?;
            let mut details = ErrorDetails::new();
            details.insert("accountId".to_owned(), json!(row.id));
            details.insert("reason".to_owned(), json!("identity-mismatch"));
            return Err(session_expired(details));
        }
        self.store(&row, result)
    }

    fn store(
        &self,
        row: &AccountRow,
        result: ProbeResult,
    ) -> Result<Option<AccountToken>, AppError> {
        let now = self.clock.now();
        let ProbeResult {
            identity,
            credential,
        } = result;
        let token_secret_id = match &credential {
            None => row.token_secret_id.clone(),
            Some(credential) => Some(self.write_token(row, &credential.token)?),
        };
        let patch = AccountPatch {
            email_masked: Some(identity.email_masked),
            display_name: Some(identity.display_name),
            avatar_url: Some(identity.avatar_url),
            token_secret_id: Some(token_secret_id),
            token_expires_at: Some(identity.expires_at),
            status: Some(AccountStatus::Linked),
            status_detail: Some(None),
            last_checked_at: Some(Some(now)),
            updated_at: Some(now),
            ..AccountPatch::default()
        };
        self.database.transaction(|transaction| {
            Ok(Repositories::new(transaction)
                .accounts
                .update(&row.id, &patch, now)?)
        })?;
        self.logger.log(
            LogLevel::Info,
            "ai",
            "Refreshed an account identity",
            &[
                ("accountId", json!(row.id)),
                ("adapter", json!(row.adapter)),
                ("hasToken", json!(credential.is_some())),
                ("expiresAt", json!(identity.expires_at)),
            ],
        );
        Ok(credential)
    }

    fn write_token(&self, row: &AccountRow, token: &str) -> Result<String, AppError> {
        if let Some(secret_id) = &row.token_secret_id {
            self.secrets.update(
                secret_id,
                SecretUpdateInput {
                    value: Some(Plaintext::new(token)),
                    ..SecretUpdateInput::default()
                },
            )?;
            return Ok(secret_id.clone());
        }
        let mut input = SecretCreateInput::new(
            ACCOUNT_TOKEN_SECRET_TYPE,
            format!("{}:{}", row.adapter, row.external_id),
            SecretScope::Personal,
        );
        input.value = Some(Plaintext::new(token));
        Ok(self.secrets.create(input)?.id)
    }

    fn mark_needs_relink(&self, row: &AccountRow, detail: &str) -> Result<(), AppError> {
        let now = self.clock.now();
        self.database.transaction(|transaction| {
            Ok(Repositories::new(transaction).accounts.update_status(
                &row.id,
                AccountStatus::NeedsRelink,
                Some(detail.to_owned()),
                now,
            )?)
        })?;
        self.logger.log(
            LogLevel::Warn,
            "ai",
            "Account needs a re-link",
            &[
                ("raw", json!(detail)),
                ("accountId", json!(row.id)),
                ("adapter", json!(row.adapter)),
            ],
        );
        Ok(())
    }

    fn stale(&self, row: &AccountRow) -> bool {
        row.token_expires_at.is_some_and(|expires_at| {
            expires_at - TOKEN_SKEW_SECONDS <= self.clock.now().div_euclid(1000)
        })
    }

    fn row(&self) -> Result<AccountRow, AppError> {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .accounts
                    .find_by_id(&self.account_id)?)
            })?
            .ok_or_else(|| {
                AppError::new(AppErrorCode::NotFound, "Аккаунт не найден")
                    .with_detail("accountId", self.account_id.clone())
            })
    }
}

#[async_trait]
impl AccountCredentials for AccountCredentialStore {
    async fn current(&self, cancel: &CancellationToken) -> Result<Option<AccountToken>, AppError> {
        self.current_token(cancel).await
    }

    async fn refresh(&self, cancel: &CancellationToken) -> Result<Option<AccountToken>, AppError> {
        self.refresh_token(cancel).await
    }
}
