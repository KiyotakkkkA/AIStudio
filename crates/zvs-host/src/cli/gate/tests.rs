use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use zvs_contract::{ExecJudgeDto, ProviderCapability, ProviderKind};

use super::*;
use crate::cli::{CLI_CONSENT_DECLINED, CliClient};
use crate::data::Repositories;
use crate::data::repositories::{ModelDraft, ProviderDraft};
use crate::ipc::{Caller, IpcServer, register};
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::platform::logger::silent_logger;
use crate::services::SystemService;
use crate::testing::{TestServices, test_services};

struct Rig {
    host: TestServices,
    server: Arc<IpcServer>,
    provider: String,
}

fn rig() -> Rig {
    let host = test_services(SystemService::new(
        Arc::new(FakeClock::default()),
        EventBus::default(),
        Duration::ZERO,
    ));
    let provider = host
        .database
        .with_connection(|connection| {
            let repositories = Repositories::new(connection);
            let provider = repositories.providers.create(&ProviderDraft::new(
                ProviderKind::Ollama,
                "Fake",
                "http://localhost:11434",
                vec![ProviderCapability::Text],
                1,
            ))?;
            repositories.models.replace_for_provider(
                &provider.id,
                &[ModelDraft::new("fake-model", "Fake", 1)],
                1,
            )?;
            Ok(provider.id)
        })
        .expect("seeds a provider");
    let mut server = IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
    register(&mut server);
    Rig {
        host,
        server: Arc::new(server),
        provider,
    }
}

impl Rig {
    fn console(&self) -> Caller {
        let (connection, _) = self
            .host
            .cli_connections
            .open(CliClient::default(), Arc::new(|_| {}));
        Caller::Cli { connection }
    }

    async fn call(&self, caller: Caller, channel: &str, payload: Value) -> Value {
        self.server.dispatch(caller, channel, payload).await
    }

    async fn create(&self, caller: Caller) -> String {
        let created = self
            .call(
                caller,
                "chat.conversations.create",
                json!({ "providerId": self.provider, "modelId": "fake-model" }),
            )
            .await;
        assert_eq!(created["ok"], json!(true), "{created}");
        created["data"]["id"].as_str().expect("an id").to_owned()
    }

    fn mode(&self) -> ExecMode {
        self.host.exec_policy.get().expect("a policy").mode
    }

    fn spawn(
        &self,
        caller: Caller,
        channel: &'static str,
        payload: Value,
    ) -> tokio::task::JoinHandle<Value> {
        let server = Arc::clone(&self.server);
        tokio::spawn(async move { server.dispatch(caller, channel, payload).await })
    }

    async fn held(&self) -> zvs_contract::CliConsentDto {
        for _ in 0..500 {
            if let Some(consent) = self.host.cli_connections.pending().into_iter().next() {
                return consent;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        panic!("no consent was held");
    }
}

fn denied(envelope: &Value, message: &str) {
    assert_eq!(envelope["ok"], json!(false), "{envelope}");
    assert_eq!(envelope["error"]["code"], json!("PERMISSION_DENIED"));
    assert_eq!(envelope["error"]["message"], json!(message));
}

fn policy(mode: ExecMode) -> ExecPolicyDto {
    ExecPolicyDto { mode, judge: None }
}

#[test]
fn loosening_is_measured_against_the_current_policy() {
    let strict = |from, to| policy_change(&policy(from), &policy(to), true);
    let lenient = |from, to| policy_change(&policy(from), &policy(to), false);
    assert_eq!(
        strict(ExecMode::AlwaysAsk, ExecMode::AutoBypass),
        Some(CliChangeDto::Mode {
            from: ExecMode::AlwaysAsk,
            to: ExecMode::AutoBypass
        })
    );
    assert_eq!(strict(ExecMode::NoRestrictions, ExecMode::SafetyAsk), None);
    assert_eq!(strict(ExecMode::SafetyAsk, ExecMode::SafetyAsk), None);
    assert_eq!(lenient(ExecMode::AlwaysAsk, ExecMode::AutoBypass), None);
    assert_eq!(lenient(ExecMode::AlwaysAsk, ExecMode::SafetyAsk), None);
    assert!(lenient(ExecMode::AutoBypass, ExecMode::NoRestrictions).is_some());
    let judged = ExecPolicyDto {
        mode: ExecMode::AutoBypass,
        judge: Some(ExecJudgeDto {
            provider_id: "p".to_owned(),
            model_id: "m".to_owned(),
        }),
    };
    assert_eq!(
        policy_change(&policy(ExecMode::AutoBypass), &judged, true),
        Some(CliChangeDto::Judge)
    );
    assert_eq!(
        policy_change(&policy(ExecMode::AutoBypass), &judged, false),
        None
    );
}

#[test]
fn always_answers_and_allow_rules_are_loosening() {
    let host = rig().host;
    let run = "7c9e6679-7425-40de-944b-e07fc1f90ae7";
    let approval = "9b2f7a3e-4c1d-4e8a-9f2b-6d5c4b3a2e10";
    let check = |channel, payload| loosening(&host, channel, &payload).expect("checks");
    assert_eq!(
        check(
            "runs.approve",
            json!({ "id": run, "approvalId": approval, "always": true })
        ),
        Some(CliChangeDto::AlwaysApproval)
    );
    assert_eq!(
        check(
            "runs.approve",
            json!({ "id": run, "approvalId": approval, "always": false })
        ),
        None
    );
    assert_eq!(
        check(
            "folders.answer",
            json!({ "id": run, "approvalId": approval, "answer": "always" })
        ),
        Some(CliChangeDto::AlwaysFolder)
    );
    assert_eq!(
        check(
            "folders.answer",
            json!({ "id": run, "approvalId": approval, "answer": "conversation" })
        ),
        None
    );
    assert_eq!(
        check(
            "execPolicy.allowRule",
            json!({ "runId": run, "approvalId": approval, "scope": "anywhere" })
        ),
        Some(CliChangeDto::AllowRule {
            scope: AllowRuleScope::Anywhere
        })
    );
}

#[tokio::test]
async fn a_console_mode_change_waits_for_the_studio_and_a_decline_keeps_the_mode() {
    let rig = rig();
    let console = rig.console();
    let before = rig.mode();
    let waiting = rig.spawn(
        console,
        "execPolicy.set",
        json!({ "mode": "no-restrictions", "judge": null }),
    );
    let consent = rig.held().await;
    assert_eq!(
        consent.change,
        CliChangeDto::Mode {
            from: before,
            to: ExecMode::NoRestrictions
        }
    );
    rig.host
        .cli_connections
        .answer(&consent.id, false)
        .expect("answers");
    denied(&waiting.await.expect("joins"), CLI_CONSENT_DECLINED);
    assert_eq!(rig.mode(), before);

    let waiting = rig.spawn(
        console,
        "execPolicy.set",
        json!({ "mode": "no-restrictions", "judge": null }),
    );
    let consent = rig.held().await;
    rig.host
        .cli_connections
        .answer(&consent.id, true)
        .expect("answers");
    assert_eq!(waiting.await.expect("joins")["ok"], json!(true));
    assert_eq!(rig.mode(), ExecMode::NoRestrictions);

    let tightened = rig
        .call(
            console,
            "execPolicy.set",
            json!({ "mode": "always-ask", "judge": null }),
        )
        .await;
    assert_eq!(tightened["ok"], json!(true));
    assert_eq!(rig.mode(), ExecMode::AlwaysAsk);
    assert!(rig.host.cli_connections.pending().is_empty());
}

#[tokio::test]
async fn the_setting_lets_auto_through_but_never_no_restrictions() {
    let rig = rig();
    rig.host
        .settings
        .set(CLI_CONFIRM_SETTING_KEY, &json!(false))
        .expect("saves");
    let console = rig.console();
    let auto = rig
        .call(
            console,
            "execPolicy.set",
            json!({ "mode": "auto-bypass", "judge": null }),
        )
        .await;
    assert_eq!(auto["ok"], json!(true), "{auto}");
    assert_eq!(rig.mode(), ExecMode::AutoBypass);
    let waiting = rig.spawn(
        console,
        "execPolicy.set",
        json!({ "mode": "no-restrictions", "judge": null }),
    );
    let consent = rig.held().await;
    rig.host
        .cli_connections
        .answer(&consent.id, false)
        .expect("answers");
    denied(&waiting.await.expect("joins"), CLI_CONSENT_DECLINED);
    assert_eq!(rig.mode(), ExecMode::AutoBypass);
}

#[tokio::test]
async fn the_studio_changes_the_mode_without_a_confirmation() {
    let rig = rig();
    let set = rig
        .call(
            Caller::Studio,
            "execPolicy.set",
            json!({ "mode": "no-restrictions", "judge": null }),
        )
        .await;
    assert_eq!(set["ok"], json!(true));
    assert_eq!(rig.mode(), ExecMode::NoRestrictions);
}

#[tokio::test]
async fn a_connection_reaches_only_the_dialogs_it_created_or_opened() {
    let rig = rig();
    let console = rig.console();
    let other = rig.console();
    let own = rig.create(console).await;
    let studio = rig.create(Caller::Studio).await;

    let listed = rig
        .call(console, "chat.queue.list", json!({ "conversationId": own }))
        .await;
    assert_eq!(listed["ok"], json!(true), "{listed}");
    for (caller, dialog) in [(console, &studio), (other, &own)] {
        denied(
            &rig.call(
                caller,
                "chat.queue.list",
                json!({ "conversationId": dialog }),
            )
            .await,
            CLI_DIALOG_REFUSED,
        );
        denied(
            &rig.call(
                caller,
                "chat.goal.stop",
                json!({ "conversationId": dialog }),
            )
            .await,
            CLI_DIALOG_REFUSED,
        );
    }
    let foreign_run = rig
        .call(
            console,
            "runs.deny",
            json!({ "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7", "approvalId": "9b2f7a3e-4c1d-4e8a-9f2b-6d5c4b3a2e10" }),
        )
        .await;
    denied(&foreign_run, CLI_DIALOG_REFUSED);
    let studio_view = rig
        .call(
            Caller::Studio,
            "chat.queue.list",
            json!({ "conversationId": studio }),
        )
        .await;
    assert_eq!(studio_view["ok"], json!(true));
}

#[tokio::test]
async fn opening_a_studio_dialog_asks_once_until_restart() {
    let rig = rig();
    let first = rig.console();
    let studio = rig.create(Caller::Studio).await;
    let waiting = rig.spawn(first, "chat.conversations.get", json!({ "id": studio }));
    let consent = rig.held().await;
    assert!(matches!(
        &consent.change,
        CliChangeDto::OpenDialog { conversation_id, .. } if conversation_id.to_string() == studio
    ));
    rig.host
        .cli_connections
        .answer(&consent.id, true)
        .expect("answers");
    assert_eq!(waiting.await.expect("joins")["ok"], json!(true));
    let Caller::Cli { connection } = first else {
        panic!("a console caller");
    };
    assert!(rig.host.cli_connections.owns(connection, &studio));

    let second = rig.console();
    let opened = rig
        .call(second, "chat.conversations.get", json!({ "id": studio }))
        .await;
    assert_eq!(opened["ok"], json!(true), "{opened}");
    assert!(rig.host.cli_connections.pending().is_empty());
}

#[tokio::test]
async fn a_declined_open_leaves_the_dialog_closed_to_the_console() {
    let rig = rig();
    let console = rig.console();
    let studio = rig.create(Caller::Studio).await;
    let waiting = rig.spawn(console, "chat.conversations.get", json!({ "id": studio }));
    let consent = rig.held().await;
    rig.host
        .cli_connections
        .answer(&consent.id, false)
        .expect("answers");
    denied(&waiting.await.expect("joins"), CLI_CONSENT_DECLINED);
    denied(
        &rig.call(
            console,
            "chat.queue.list",
            json!({ "conversationId": studio }),
        )
        .await,
        CLI_DIALOG_REFUSED,
    );
}

#[tokio::test]
async fn a_closed_connection_and_studio_only_channels_are_refused() {
    let rig = rig();
    let console = rig.console();
    let Caller::Cli { connection } = console else {
        panic!("a console caller");
    };
    let refused = rig
        .call(console, "settings.set", json!({ "key": "x", "value": 1 }))
        .await;
    denied(&refused, crate::ipc::CLI_CHANNEL_REFUSED);
    rig.host.cli_connections.close(connection);
    denied(
        &rig.call(console, "system.ping", json!({ "sentAt": 1 }))
            .await,
        crate::cli::CLI_CONNECTION_CLOSED,
    );
    denied(
        &rig.call(Caller::Panel, "system.ping", json!({ "sentAt": 1 }))
            .await,
        crate::ipc::CALLER_REFUSED,
    );
}
