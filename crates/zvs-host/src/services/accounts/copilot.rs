use super::*;
use crate::drivers::ai::copilot::{Client, auth_error, protocol_error};
use crate::drivers::ai::identity::AccountIdentity;

fn authorization_url(step: &Value) -> Result<&str, AppError> {
    let url = step["url"].as_str().ok_or_else(protocol_error)?;
    let parsed = Url::parse(url).map_err(|_| protocol_error())?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("github.com")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
    {
        return Err(protocol_error());
    }
    Ok(url)
}

impl Inner {
    pub(super) fn save_copilot(
        &self,
        status: zvs_copilot::AuthStatus,
    ) -> Result<AccountLinkResult, AppError> {
        self.save(
            AccountFamily::Copilot,
            ProbeResult {
                identity: AccountIdentity {
                    external_id: status.identity_key().ok_or_else(auth_error)?,
                    display_name: status.login,
                    ..AccountIdentity::default()
                },
                credential: None,
            },
        )
    }

    pub(super) async fn link_copilot(
        &self,
        account_id: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<AccountLinkResult, AppError> {
        if let Some(id) = account_id
            && self.require(id)?.family() != Some(AccountFamily::Copilot)
        {
            return Err(auth_error());
        }
        let stream = self.events.open_stream()?;
        stream.emit(step([
            ("domain", json!("accounts")),
            ("adapter", json!("copilot")),
            ("phase", json!("waiting-for-sign-in")),
        ]));
        let work = async {
            let mut client = Client::start(cancel).await?;
            match client.auth(None).await {
                Ok(status) => return self.save_copilot(status),
                Err(error) if error.code == AppErrorCode::ProviderSessionExpired => {}
                Err(error) => return Err(error),
            }
            let session = client.session(None, None).await?;
            let flow = client
                .call(
                    "session.accounts.login.begin",
                    json!({"sessionId":session,"kind":"githubDotCom"}),
                )
                .await?;
            let flow_id = flow
                .get("flowId")
                .and_then(Value::as_str)
                .ok_or_else(protocol_error)?;
            let mut current = flow["step"].clone();
            loop {
                match current["kind"].as_str() {
                    Some("open-url") => {
                        self.browser
                            .open_external(authorization_url(&current)?)
                            .await?;
                    }
                    Some("awaiting" | "needs-interaction") => {}
                    Some("completed") if current["result"]["status"] == "completed" => {
                        return self.save_copilot(client.auth(None).await?);
                    }
                    // Never consent to plaintext credential storage or choose another identity silently.
                    _ => return Err(protocol_error()),
                }
                tokio::select! {
                    () = cancel.cancelled() => return Err(operation_cancelled()),
                    () = sleep(self.interval) => {},
                }
                current = client
                    .call(
                        "session.accounts.login.advance",
                        json!({"sessionId":session,"flowId":flow_id}),
                    )
                    .await?;
            }
        };
        let result = tokio::select! {
            biased;
            () = cancel.cancelled() => Ok(AccountLinkResult::Cancelled { detail: NO_SIGN_IN.to_owned() }),
            () = sleep(self.timeout) => Ok(AccountLinkResult::Timeout { detail: NO_SIGN_IN.to_owned() }),
            result = work => result,
        };
        if let Ok(linked) = &result {
            stream.emit(step([
                ("domain", json!("accounts")),
                ("adapter", json!("copilot")),
                ("phase", json!("done")),
                ("result", json!(linked)),
            ]));
        }
        let status = match &result {
            Ok(AccountLinkResult::Linked { .. }) => RunOutcomeStatus::Ok,
            Ok(_) => RunOutcomeStatus::Cancelled,
            Err(_) => RunOutcomeStatus::Failed,
        };
        stream.end(RunOutcomeDto {
            status,
            code: None,
            message: None,
        });
        result
    }

    pub(super) async fn refresh_copilot(
        &self,
        row: AccountRow,
        cancel: &CancellationToken,
    ) -> Result<AccountDto, AppError> {
        let work = async {
            let mut client = Client::start(cancel).await?;
            client.auth(Some(&row.external_id)).await
        };
        let result = tokio::select! {
            () = cancel.cancelled() => return Err(operation_cancelled()),
            () = sleep(self.timeout) => return Err(unchecked()),
            result = work => result,
        };
        match result {
            Ok(status) if !cancel.is_cancelled() => {
                if let AccountLinkResult::Linked { account, .. } = self.save_copilot(status)? {
                    return Ok(account);
                }
                Err(protocol_error())
            }
            Ok(_) => Err(operation_cancelled()),
            Err(error) => {
                if is_session_expired(&error) {
                    self.set_status(
                        &row.id,
                        AccountStatus::NeedsRelink,
                        crate::drivers::ai::copilot::SESSION_EXPIRED,
                    )?;
                }
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_opens_only_https_github_without_embedded_credentials() {
        let url = "https://github.com/login/oauth/authorize?client_id=test";
        assert_eq!(authorization_url(&json!({"url":url})).unwrap(), url);
        for refused in [
            "https://example.com/",
            "http://github.com/",
            "https://github.com:8443/",
            "https://person@github.com/",
            "https://github.com@example.com/",
            "file:///tmp/login",
            "javascript:alert(1)",
        ] {
            assert!(authorization_url(&json!({"url":refused})).is_err());
        }
        assert!(authorization_url(&json!({})).is_err());
    }
}
