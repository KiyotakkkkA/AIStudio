use super::*;
use crate::drivers::ai::identity::AccountIdentity;
use crate::drivers::ai::openai::{auth_error, authorize, credentials, failure};
use crate::drivers::ai::transport::AccountToken;
use zvs_openai::Credentials;

impl Inner {
    pub(super) async fn link_openai(
        &self,
        account_id: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<AccountLinkResult, AppError> {
        let saved = account_id
            .map(|id| {
                let row = self.require(id)?;
                if row.family() != Some(AccountFamily::Openai) {
                    return Err(auth_error());
                }
                let secret = self
                    .secrets
                    .resolve(row.token_secret_id.as_deref().ok_or_else(auth_error)?)?;
                serde_json::from_str::<Credentials>(secret.expose()).map_err(|_| auth_error())
            })
            .transpose()?;
        let host_id = self.database.transaction(|transaction| {
            let settings = Repositories::new(transaction).settings;
            if let Some(saved) = settings.get("internal.openai.host-id")? {
                return serde_json::from_str::<String>(&saved.value)
                    .map_err(|_| DataError::from(failure()));
            }
            let id = format!("urn:uuid:{}", uuid::Uuid::new_v4());
            settings.set(
                "internal.openai.host-id",
                &json!(id).to_string(),
                self.clock.now(),
            )?;
            Ok(id)
        })?;
        let stream = self.events.open_stream()?;
        stream.emit(step([
            ("domain", json!("accounts")),
            ("adapter", json!("openai")),
            ("phase", json!("waiting-for-sign-in")),
        ]));
        let result = tokio::select! {
            () = cancel.cancelled() => Ok(AccountLinkResult::Cancelled { detail: NO_SIGN_IN.to_owned() }),
            _ = sleep(self.timeout) => Ok(AccountLinkResult::Timeout { detail: NO_SIGN_IN.to_owned() }),
            result = authorize(self.browser.as_ref(), &host_id, saved.as_ref(), cancel) => {
                match result {
                    Ok(saved) if !cancel.is_cancelled() => self.save_openai(saved),
                    Ok(_) => Err(operation_cancelled()),
                    Err(error) if error.code == AppErrorCode::RunCancelled => Ok(AccountLinkResult::Cancelled { detail: NO_SIGN_IN.to_owned() }),
                    Err(error) => Err(error),
                }
            }
        };
        match &result {
            Ok(linked) => {
                stream.emit(step([
                    ("domain", json!("accounts")),
                    ("adapter", json!("openai")),
                    ("phase", json!("done")),
                    ("result", json!(linked)),
                ]));
                stream.end(if matches!(linked, AccountLinkResult::Linked { .. }) {
                    ok_outcome()
                } else {
                    RunOutcomeDto {
                        status: RunOutcomeStatus::Cancelled,
                        code: None,
                        message: None,
                    }
                });
            }
            Err(_) => stream.end(RunOutcomeDto {
                status: RunOutcomeStatus::Failed,
                code: None,
                message: Some(LINKING_FAILED.to_owned()),
            }),
        }
        result
    }

    pub(super) fn save_openai(&self, saved: Credentials) -> Result<AccountLinkResult, AppError> {
        let permitted = saved.allows_inference();
        let suffix = saved
            .client_id
            .chars()
            .rev()
            .take(6)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        let title = saved
            .name
            .as_deref()
            .unwrap_or("OpenAI")
            .chars()
            .take(40)
            .collect::<String>();
        let name = format!("{title} · {suffix}");
        let result = self.save(
            AccountFamily::Openai,
            ProbeResult {
                identity: AccountIdentity {
                    external_id: saved.identity_key(),
                    email_masked: saved.email.clone(),
                    display_name: Some(name),
                    avatar_url: None,
                    expires_at: Some(saved.expires_at),
                },
                credential: Some(AccountToken::bearer(
                    serde_json::to_string(&saved).map_err(|_| failure())?,
                )),
            },
        )?;
        if !permitted && let AccountLinkResult::Linked { account, .. } = &result {
            self.set_status(
                &account.id.to_string(),
                AccountStatus::NeedsRelink,
                "Разрешите использование подписки в настройках ChatGPT",
            )?;
            return Ok(AccountLinkResult::Linked {
                account: self.dto(&self.require(&account.id.to_string())?)?,
                replaced_previous_identity: false,
                detail: None,
            });
        }
        Ok(result)
    }

    pub(super) async fn refresh_openai(
        &self,
        row: AccountRow,
        cancel: &CancellationToken,
    ) -> Result<AccountDto, AppError> {
        let secret_id = row.token_secret_id.as_deref().ok_or_else(auth_error)?;
        match credentials(self.secrets.clone(), secret_id, cancel).await {
            Ok(saved) => {
                if cancel.is_cancelled() {
                    return Err(operation_cancelled());
                }
                if saved.identity_key() != row.external_id {
                    return Err(auth_error());
                }
                if let AccountLinkResult::Linked { account, .. } = self.save_openai(saved)? {
                    return Ok(account);
                }
                Err(failure())
            }
            Err(error) => {
                if is_session_expired(&error) {
                    self.set_status(&row.id, AccountStatus::NeedsRelink, SESSION_EXPIRED_DETAIL)?;
                }
                Err(error)
            }
        }
    }
}
