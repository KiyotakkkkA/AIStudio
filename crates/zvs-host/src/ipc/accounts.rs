use std::sync::Arc;

use zvs_contract::{AccountLinkInput, AccountRef, AccountsCancelLinkOutput, AppError};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "accounts.list",
            |services: Arc<Services>, _input: ()| async move { services.accounts.list() },
        )
        .handle(
            "accounts.link",
            |services: Arc<Services>, input: AccountLinkInput| async move {
                services.accounts.link(input.adapter).await
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
                services.accounts.unlink(&input.id.to_string())
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
            server.dispatch("accounts.list", Value::Null).await,
            json!({ "ok": true, "data": [] })
        );
        let linked = server
            .dispatch("accounts.link", json!({ "adapter": "qwen-web" }))
            .await;
        assert_eq!(linked["ok"], json!(true), "{linked}");
        assert_eq!(linked["data"]["status"], json!("linked"));
        let listed = server.dispatch("accounts.list", Value::Null).await;
        let account = serde_json::from_value::<Vec<AccountDto>>(listed["data"].clone())
            .expect("accounts")
            .remove(0);
        assert_eq!(account.email_masked.as_deref(), Some("p***@example.com"));
        let id = account.id.to_string();
        let refreshed = server
            .dispatch("accounts.refresh", json!({ "id": id }))
            .await;
        assert_eq!(refreshed["ok"], json!(true), "{refreshed}");
        assert_eq!(
            server
                .dispatch("accounts.cancelLink", json!({ "adapter": "qwen-web" }))
                .await,
            json!({ "ok": true, "data": { "cancelled": false } })
        );
        let unlinked = server
            .dispatch("accounts.unlink", json!({ "id": id }))
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
            .dispatch("accounts.refresh", json!({ "id": id }))
            .await;
        assert_eq!(missing["error"]["code"], json!("NOT_FOUND"));
        let invalid = server
            .dispatch("accounts.link", json!({ "adapter": "openai-compatible" }))
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
