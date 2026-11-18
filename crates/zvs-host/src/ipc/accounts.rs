use std::sync::Arc;

use zvs_contract::{
    AccountFamily, AccountLinkInput, AccountRef, AccountsCancelLinkOutput, AppError, AppErrorCode,
};

use super::IpcServer;
use crate::services::Services;

pub(super) fn require_browser(services: &Services, adapter: AccountFamily) -> Result<(), AppError> {
    if matches!(adapter, AccountFamily::QwenWeb | AccountFamily::ZaiWeb)
        && !services.browser_settings.enabled()
    {
        return Err(AppError::new(
            AppErrorCode::Conflict,
            crate::browser::ACCOUNT_NEEDS_BROWSER,
        )
        .with_detail("reason", "browser_disabled"));
    }
    Ok(())
}

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "accounts.openUsage",
            |services: Arc<Services>, input: AccountRef| async move {
                services.accounts.open_usage(&input.id.to_string()).await
            },
        )
        .handle(
            "accounts.list",
            |services: Arc<Services>, _input: ()| async move { services.accounts.list() },
        )
        .handle(
            "accounts.link",
            |services: Arc<Services>, input: AccountLinkInput| async move {
                require_browser(&services, input.adapter)?;
                services
                    .accounts
                    .link_account(input.adapter, input.account_id.map(|id| id.to_string()))
                    .await
            },
        )
        .handle(
            "accounts.cancelLink",
            |services: Arc<Services>, input: AccountLinkInput| async move {
                Ok::<AccountsCancelLinkOutput, AppError>(
                    services.accounts.cancel_link(input.adapter),
                )
            },
        )
        .handle(
            "accounts.unlink",
            |services: Arc<Services>, input: AccountRef| async move {
                services.accounts.disconnect(&input.id.to_string()).await
            },
        )
        .handle(
            "accounts.refresh",
            |services: Arc<Services>, input: AccountRef| async move {
                services.accounts.refresh(&input.id.to_string()).await
            },
        );
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::{Value, json};
    use zvs_contract::{AccountDto, AccountsUnlinkOutput, ResultEnvelope};

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::platform::logger::silent_logger;
    use crate::services::SystemService;
    use crate::testing::test_services;

    #[tokio::test]
    async fn browser_sign_in_is_refused_before_linking_for_studio_and_cli() {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        host.shared()
            .browser_settings
            .set(&zvs_contract::BrowserSettingsDto {
                enabled: false,
                ..Default::default()
            })
            .unwrap();
        let mut server = IpcServer::new(host.shared(), silent_logger());
        register(&mut server);
        crate::ipc::cli::register(&mut server);
        let (connection, _) = host
            .shared()
            .cli_connections
            .open(Default::default(), Arc::new(|_| {}));
        for (caller, channel) in [
            (crate::ipc::Caller::Studio, "accounts.link"),
            (crate::ipc::Caller::Cli { connection }, "cli.accounts.link"),
        ] {
            for adapter in ["qwen-web", "zai-web"] {
                let result = server
                    .dispatch(caller, channel, json!({ "adapter": adapter }))
                    .await;
                assert_eq!(result["error"]["code"], json!("CONFLICT"));
                assert_eq!(
                    result["error"]["message"],
                    json!(crate::browser::ACCOUNT_NEEDS_BROWSER)
                );
                assert_eq!(
                    result["error"]["details"]["reason"],
                    json!("browser_disabled")
                );
            }
        }
        assert!(host.browser.opened().is_empty());
        assert!(host.shared().accounts.list().unwrap().is_empty());
    }

    #[tokio::test]
    async fn all_account_channel_outputs_validate_and_contain_no_bearer_token() {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        let mut server =
            IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
        register(&mut server);

        assert_eq!(
            server
                .dispatch(crate::ipc::Caller::Studio, "accounts.list", Value::Null)
                .await,
            json!({ "ok": true, "data": [] })
        );
        let linked = server
            .dispatch(
                crate::ipc::Caller::Studio,
                "accounts.link",
                json!({ "adapter": "qwen-web" }),
            )
            .await;
        assert_eq!(linked["ok"], json!(true), "{linked}");
        assert_eq!(linked["data"]["status"], json!("linked"));
        let listed = server
            .dispatch(crate::ipc::Caller::Studio, "accounts.list", Value::Null)
            .await;
        let account = serde_json::from_value::<Vec<AccountDto>>(listed["data"].clone())
            .expect("accounts")
            .remove(0);
        assert_eq!(account.email_masked.as_deref(), Some("p***@example.com"));
        let id = account.id.to_string();
        let refreshed = server
            .dispatch(
                crate::ipc::Caller::Studio,
                "accounts.refresh",
                json!({ "id": id }),
            )
            .await;
        assert_eq!(refreshed["ok"], json!(true), "{refreshed}");
        assert_eq!(
            server
                .dispatch(
                    crate::ipc::Caller::Studio,
                    "accounts.cancelLink",
                    json!({ "adapter": "qwen-web" })
                )
                .await,
            json!({ "ok": true, "data": { "cancelled": false } })
        );
        let unlinked = server
            .dispatch(
                crate::ipc::Caller::Studio,
                "accounts.unlink",
                json!({ "id": id }),
            )
            .await;
        assert_eq!(
            serde_json::from_value::<ResultEnvelope<AccountsUnlinkOutput>>(unlinked.clone())
                .expect("an envelope"),
            ResultEnvelope::Ok(AccountsUnlinkOutput {
                id: account.id,
                removed: true,
                browser_session_preserved: true,
            })
        );
        let missing = server
            .dispatch(
                crate::ipc::Caller::Studio,
                "accounts.refresh",
                json!({ "id": id }),
            )
            .await;
        assert_eq!(missing["error"]["code"], json!("NOT_FOUND"));
        let invalid = server
            .dispatch(
                crate::ipc::Caller::Studio,
                "accounts.link",
                json!({ "adapter": "openai-compatible" }),
            )
            .await;
        assert_eq!(invalid["error"]["code"], json!("VALIDATION_FAILED"));
        let printed = format!(
            "{linked}{listed}{refreshed}{unlinked}{:?}",
            host.account_events.events()
        );
        assert!(!printed.contains("private-bearer-token"));
        assert!(!printed.contains("person@example"));
        assert_eq!(host.browser.opened(), ["https://chat.qwen.ai/"]);
    }
}
