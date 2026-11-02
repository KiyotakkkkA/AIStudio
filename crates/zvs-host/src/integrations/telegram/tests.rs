use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppErrorCode, CreateIntegrationInput, IntegrationAccountDto, IntegrationConfig,
    IntegrationKind, IntegrationStatus, IntegrationValue, SecretId, SecretScope, ToolTier,
};

use super::*;
use crate::integrations::common::{ACCESS, READ_ONLY};
use crate::integrations::stub::{Reply, StubServer};
use crate::integrations::{
    IntegrationCallTarget, IntegrationService, IntegrationServiceOptions, account_tool_key,
};
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::services::crypto::Plaintext;
use crate::services::{SecretCreateInput, SystemService};
use crate::testing::{TestServices, test_services};
use crate::workers::WorkerSupervisor;

const TOKEN: &str = "123456:planted-bot-token";
const NOW: i64 = 1_793_000_000_000;

static SECRETS: AtomicUsize = AtomicUsize::new(0);

fn path(method: &str) -> String {
    format!("/bot{TOKEN}/{method}")
}

fn ok(result: Value) -> Reply {
    Reply::json(&json!({ "ok": true, "result": result }))
}

fn update(id: i64, chat: i64, text: &str) -> Value {
    json!({
        "update_id": id,
        "message": {
            "message_id": id * 10,
            "date": 1_793_000_000,
            "chat": { "id": chat, "type": "private", "first_name": "Иван" },
            "from": { "id": chat, "is_bot": false, "first_name": "Иван", "username": "ivan" },
            "text": text,
        },
    })
}

struct Rig {
    host: TestServices,
    server: StubServer,
    service: IntegrationService,
}

async fn rig() -> Rig {
    let host = test_services(SystemService::new(
        Arc::new(FakeClock::new(NOW)),
        EventBus::default(),
        Duration::ZERO,
    ));
    let server = StubServer::start().await;
    server.steady(
        &path("getMe"),
        ok(json!({ "id": 1, "is_bot": true, "first_name": "Помощник", "username": "zvs_bot" })),
    );
    server.steady(
        &path("getUpdates"),
        ok(json!([])).delayed(Duration::from_millis(50)),
    );
    let provider =
        Arc::new(TelegramProvider::new(reqwest::Client::new()).with_base(server.origin()));
    let services = host.shared();
    let workers =
        WorkerSupervisor::new(host.clock.clone(), crate::platform::logger::silent_logger());
    let service = IntegrationService::new(IntegrationServiceOptions {
        providers: vec![provider],
        clock: host.clock.clone(),
        approvals: Some(services.runs.approvals()),
        ..IntegrationServiceOptions::new(
            host.database.shared(),
            services.tools.clone(),
            Arc::clone(&host.nodes),
            Arc::clone(&services.secrets),
            workers,
        )
    })
    .expect("an integration service");
    Rig {
        host,
        server,
        service,
    }
}

impl Rig {
    fn secret(&self) -> SecretId {
        let name = format!("Бот {}", SECRETS.fetch_add(1, Ordering::SeqCst));
        let id = self
            .host
            .shared()
            .secrets
            .create(SecretCreateInput {
                value: Some(Plaintext::new(TOKEN)),
                ..SecretCreateInput::new("user", name, SecretScope::Personal)
            })
            .expect("a secret")
            .id;
        SecretId::parse(&id).expect("a uuid")
    }

    fn config(&self, access: &str, allowed: &str) -> IntegrationConfig {
        IntegrationConfig::from([
            (
                BOT_TOKEN.to_owned(),
                IntegrationValue::secret(self.secret()),
            ),
            (ACCESS.to_owned(), IntegrationValue::text(access)),
            (ALLOWED_CHATS.to_owned(), IntegrationValue::text(allowed)),
        ])
    }

    async fn account(&self, config: IntegrationConfig, enabled: bool) -> IntegrationAccountDto {
        self.service
            .create(&CreateIntegrationInput {
                config: Some(config),
                enabled: Some(enabled),
                ..CreateIntegrationInput::new(IntegrationKind::Telegram, "Поддержка")
            })
            .await
            .expect("creates")
    }

    fn reload(&self, account: &IntegrationAccountDto) -> IntegrationAccountDto {
        self.service
            .account(&account.id.to_string())
            .expect("the account")
    }

    async fn call(
        &self,
        account: &IntegrationAccountDto,
        tool: &str,
        input: Value,
    ) -> Result<Value, zvs_contract::AppError> {
        self.service
            .gateway()
            .call(
                IntegrationCallTarget {
                    account_id: account.id.to_string(),
                    tool: tool.to_owned(),
                },
                input.as_object().cloned().unwrap_or_default(),
                None,
                CancellationToken::new(),
            )
            .await
    }
}

async fn eventually(mut check: impl FnMut() -> bool) {
    for _ in 0..500 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the condition did not hold in time");
}

#[tokio::test]
async fn a_message_fires_the_trigger_once_skips_the_backlog_and_survives_a_restart() {
    let rig = rig().await;
    rig.server
        .queue(&path("getUpdates"), ok(json!([update(10, 42, "старое")])))
        .queue(
            &path("getUpdates"),
            ok(json!([update(11, 42, "Не работает вход")])),
        );
    let account = rig.account(rig.config(READ_ONLY, ""), true).await;
    eventually(|| rig.reload(&account).counters.events == 1).await;

    let first = rig.server.calls_to(&path("getUpdates"));
    assert_eq!(first[0].json()["offset"], json!(-1));
    assert_eq!(first[1].json()["offset"], json!(11));

    let recent = rig
        .call(&account, RECENT, json!({}))
        .await
        .expect("recent messages");
    assert_eq!(recent["untrusted"], json!(true));
    let messages = recent["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0]["text"],
        json!({ "text": "Не работает вход", "untrusted": true })
    );
    assert_eq!(messages[0]["from"]["name"]["untrusted"], json!(true));
    assert_eq!(messages[0]["chatId"], json!("42"));

    let id = account.id.to_string();
    rig.service.disable(&id).await.expect("disables");
    let before = rig.server.calls_to(&path("getUpdates")).len();
    rig.server.queue(
        &path("getUpdates"),
        ok(json!([update(11, 42, "Не работает вход")])),
    );
    rig.service.enable(&id).await.expect("enables");
    eventually(|| rig.reload(&account).counters.duplicates == 1).await;
    let resumed = &rig.server.calls_to(&path("getUpdates"))[before];
    assert_eq!(resumed.json()["offset"], json!(12));
    assert_eq!(rig.reload(&account).counters.events, 1);
}

#[tokio::test]
async fn messages_from_chats_outside_the_list_are_not_emitted() {
    let rig = rig().await;
    rig.server.queue(&path("getUpdates"), ok(json!([]))).queue(
        &path("getUpdates"),
        ok(json!([update(1, 7, "чужой"), update(2, 42, "свой")])),
    );
    let account = rig.account(rig.config(READ_ONLY, "42"), true).await;
    eventually(|| rig.reload(&account).counters.events == 1).await;
    let recent = rig
        .call(&account, RECENT, json!({}))
        .await
        .expect("recent messages");
    assert_eq!(recent["messages"].as_array().map(Vec::len), Some(1));
    assert_eq!(recent["messages"][0]["chatId"], json!("42"));
}

#[tokio::test]
async fn sending_needs_read_write_access_and_an_allowed_chat_and_registers_at_ask() {
    let rig = rig().await;
    rig.server.steady(
        &path("sendMessage"),
        ok(json!({
            "message_id": 99,
            "date": 1_793_000_000,
            "chat": { "id": 42, "type": "private" },
        })),
    );
    let reader = rig.account(rig.config(READ_ONLY, ""), false).await;
    let refused = rig
        .call(
            &reader,
            SEND_MESSAGE,
            json!({ "chatId": "42", "text": "Привет" }),
        )
        .await
        .expect_err("read-only refuses");
    assert_eq!(refused.code, AppErrorCode::PermissionDenied);
    assert_eq!(refused.message, SEND_DISABLED);
    let detail = rig.service.get(&reader.id.to_string()).expect("the detail");
    assert!(detail.tools.iter().all(|entry| !entry.tool.side_effect));

    let writer = rig.account(rig.config(READ_WRITE, "42"), true).await;
    let other = rig
        .call(
            &writer,
            SEND_MESSAGE,
            json!({ "chatId": "43", "text": "Привет" }),
        )
        .await
        .expect_err("another chat is refused");
    assert_eq!(other.message, CHAT_NOT_ALLOWED);
    assert!(rig.server.calls_to(&path("sendMessage")).is_empty());

    let sent = rig
        .call(
            &writer,
            SEND_MESSAGE,
            json!({ "chatId": "42", "text": "Заявка принята", "replyToMessageId": 110 }),
        )
        .await
        .expect("sends");
    assert_eq!(sent["messageId"], json!(99));
    let body = rig.server.calls_to(&path("sendMessage"))[0].json();
    assert_eq!(body["chat_id"], json!("42"));
    assert_eq!(body["text"], json!("Заявка принята"));
    assert_eq!(body["reply_parameters"]["message_id"], json!(110));

    let key = account_tool_key(SEND_MESSAGE, &writer.id.to_string());
    let registered = rig
        .host
        .shared()
        .tools
        .find_by_key(&key)
        .expect("reads")
        .expect("registered");
    assert_eq!(registered.tier, ToolTier::Ask);
    assert!(!registered.read_only);
}

#[tokio::test]
async fn a_file_goes_out_as_a_multipart_document() {
    let rig = rig().await;
    rig.server.steady(
        &path("sendDocument"),
        ok(json!({
            "message_id": 5,
            "date": 1_793_000_000,
            "chat": { "id": 42, "type": "private" },
        })),
    );
    let directory = tempfile::tempdir().expect("a directory");
    let file = directory.path().join("отчёт.txt");
    std::fs::write(&file, "содержимое отчёта").expect("writes");
    let account = rig.account(rig.config(READ_WRITE, ""), false).await;
    rig.call(
        &account,
        SEND_FILE,
        json!({ "chatId": "42", "path": file.to_string_lossy(), "caption": "Отчёт" }),
    )
    .await
    .expect("sends");
    let call = &rig.server.calls_to(&path("sendDocument"))[0];
    assert!(
        call.header("content-type")
            .is_some_and(|value| value.starts_with("multipart/form-data"))
    );
    let body = call.text();
    assert!(body.contains("содержимое отчёта"));
    assert!(body.contains("name=\"chat_id\""));
    assert!(body.contains("Отчёт"));

    let missing = rig
        .call(
            &account,
            SEND_FILE,
            json!({ "chatId": "42", "path": directory.path().join("нет.txt").to_string_lossy() }),
        )
        .await
        .expect_err("a missing file");
    assert_eq!(missing.code, AppErrorCode::NotFound);
}

#[tokio::test]
async fn a_rate_limited_poll_backs_off_and_carries_on() {
    let rig = rig().await;
    rig.server
        .queue(&path("getUpdates"), ok(json!([])))
        .queue(
            &path("getUpdates"),
            Reply::json(&json!({
                "ok": false,
                "error_code": 429,
                "description": "Too Many Requests: retry after 1",
                "parameters": { "retry_after": 1 },
            }))
            .status(429),
        )
        .queue(
            &path("getUpdates"),
            ok(json!([update(3, 42, "после паузы")])),
        );
    let account = rig.account(rig.config(READ_ONLY, ""), true).await;
    eventually(|| rig.reload(&account).counters.events == 1).await;
    let account = rig.reload(&account);
    assert_eq!(account.status, IntegrationStatus::Running);
    assert_eq!(account.counters.errors, 0);
}

#[tokio::test]
async fn a_rejected_token_fails_the_account_without_retrying_or_leaking_the_token() {
    let rig = rig().await;
    rig.server.steady(
        &path("getMe"),
        Reply::json(&json!({ "ok": false, "error_code": 401, "description": "Unauthorized" }))
            .status(401),
    );
    let account = rig.account(rig.config(READ_ONLY, ""), true).await;
    eventually(|| rig.reload(&account).status == IntegrationStatus::Failed).await;
    let failed = rig.reload(&account);
    assert_eq!(failed.status_detail.as_deref(), Some(TOKEN_REJECTED));
    let activity = rig
        .service
        .activity(&zvs_contract::IntegrationActivityInput {
            id: account.id,
            limit: None,
        })
        .expect("activity");
    assert!(
        activity
            .iter()
            .all(|entry| !entry.message.contains("planted"))
    );
    let checks = rig
        .service
        .test(&account.id.to_string())
        .await
        .expect("a test result");
    assert!(!checks.ok);
    assert!(
        !serde_json::to_string(&checks)
            .expect("json")
            .contains("planted")
    );
}
