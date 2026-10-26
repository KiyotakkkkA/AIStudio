use std::time::Duration;

use serde_json::{Value, json};
use zvs_contract::{
    ConversationDetailDto, ConversationDto, ProviderCapability, ProviderKind, RunHandleDto, RunId,
    RunStatus, Validate,
};

use super::*;
use crate::data::Repositories;
use crate::data::repositories::{ModelDraft, ProviderDraft};
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::platform::logger::silent_logger;
use crate::services::SystemService;
use crate::testing::{FakeScript, TestServices, test_services};

struct Channels {
    host: TestServices,
    server: IpcServer,
    provider_id: String,
}

fn registered() -> Channels {
    let host = test_services(SystemService::new(
        Arc::new(FakeClock::default()),
        EventBus::default(),
        Duration::ZERO,
    ));
    host.drivers
        .driver()
        .expect("a fake driver")
        .script(FakeScript {
            deltas: Some(vec!["Hello".to_owned(), " world".to_owned()]),
            ..FakeScript::default()
        });
    let provider_id = host
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
                &[ModelDraft {
                    context_window: Some(8192),
                    max_output: Some(128),
                    ..ModelDraft::new("fake-model", "Fake", 1)
                }],
                1,
            )?;
            Ok(provider.id)
        })
        .expect("seeds a provider");
    let mut server = IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
    register(&mut server);
    Channels {
        host,
        server,
        provider_id,
    }
}

fn data<T: serde::de::DeserializeOwned + Validate>(envelope: &Value) -> T {
    assert_eq!(envelope["ok"], json!(true), "{envelope}");
    let parsed: T = serde_json::from_value(envelope["data"].clone()).expect("the contract shape");
    parsed.validate().expect("valid");
    parsed
}

fn failed(envelope: &Value) -> &Value {
    assert_eq!(envelope["ok"], json!(false), "{envelope}");
    &envelope["error"]["code"]
}

#[tokio::test]
async fn a_turn_through_the_channels_answers_titles_the_conversation_and_matches_the_contract() {
    let channels = registered();
    let created: ConversationDto = data(
        &channels
            .server
            .dispatch(
                "chat.conversations.create",
                json!({ "providerId": channels.provider_id, "modelId": "fake-model", "systemPrompt": "Be concise." }),
            )
            .await,
    );
    assert_eq!(created.title, "Новый диалог");
    let handle: RunHandleDto = data(
        &channels
            .server
            .dispatch(
                "chat.send",
                json!({ "conversationId": created.id, "text": "  First question  " }),
            )
            .await,
    );
    tokio::time::timeout(Duration::from_secs(10), channels.host.runs.wait(&handle.id))
        .await
        .expect("finishes");
    assert_eq!(
        channels.host.runs.get(&handle.id).expect("a run").status,
        RunStatus::Succeeded
    );
    let detail: ConversationDetailDto = data(
        &channels
            .server
            .dispatch("chat.conversations.get", json!({ "id": created.id }))
            .await,
    );
    assert_eq!(detail.conversation.title, "First question");
    assert_eq!(
        detail
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        ["First question", "Hello world"]
    );
    let listed: Vec<ConversationDto> = data(
        &channels
            .server
            .dispatch("chat.conversations.list", Value::Null)
            .await,
    );
    assert_eq!(listed.len(), 1);
    let cancelled = channels
        .server
        .dispatch("chat.cancel", json!({ "id": handle.id }))
        .await;
    assert_eq!(cancelled["ok"], json!(true), "{cancelled}");
    let truncated: ConversationDetailDto = data(
        &channels
            .server
            .dispatch(
                "chat.conversations.truncate",
                json!({ "id": created.id, "messageId": detail.messages[1].id }),
            )
            .await,
    );
    assert_eq!(truncated.messages.len(), 1);
}

#[tokio::test]
async fn the_conversation_channels_update_settings_and_refuse_bad_input() {
    let channels = registered();
    let created: ConversationDto = data(
        &channels
            .server
            .dispatch(
                "chat.conversations.create",
                json!({ "providerId": channels.provider_id, "modelId": "fake-model" }),
            )
            .await,
    );
    let renamed: ConversationDto = data(
        &channels
            .server
            .dispatch(
                "chat.conversations.rename",
                json!({ "id": created.id, "title": " Plans " }),
            )
            .await,
    );
    assert_eq!(renamed.title, "Plans");
    let related: ConversationDto = data(
        &channels
            .server
            .dispatch(
                "chat.conversations.updateRelations",
                json!({ "id": created.id, "useRelations": true }),
            )
            .await,
    );
    assert!(related.use_relations);
    let stores: ConversationDto = data(
        &channels
            .server
            .dispatch(
                "chat.conversations.updateStores",
                json!({ "id": created.id, "attachedStoreIds": [] }),
            )
            .await,
    );
    assert!(stores.attached_store_ids.is_empty());
    let commands: Vec<zvs_contract::ChatCommandDto> =
        data(&channels.server.dispatch("chat.commands", json!(null)).await);
    assert_eq!(
        commands
            .iter()
            .map(|command| command.command)
            .collect::<Vec<_>>(),
        [zvs_contract::ChatCommand::BrowserUse]
    );
    assert!(
        failed(
            &channels
                .server
                .dispatch(
                    "chat.conversations.updateTools",
                    json!({ "id": created.id })
                )
                .await
        )
        .is_string()
    );
    assert_eq!(
        failed(
            &channels
                .server
                .dispatch(
                    "chat.send",
                    json!({ "conversationId": created.id, "text": "   " })
                )
                .await
        ),
        &json!("VALIDATION_FAILED")
    );
    assert_eq!(
        failed(
            &channels
                .server
                .dispatch(
                    "chat.cancel",
                    json!({ "id": RunId::new(crate::platform::ids::new_id()) })
                )
                .await
        ),
        &json!("NOT_FOUND")
    );
    let removed = channels
        .server
        .dispatch("chat.conversations.remove", json!({ "id": created.id }))
        .await;
    assert_eq!(removed, json!({ "ok": true }));
    assert_eq!(
        failed(
            &channels
                .server
                .dispatch("chat.conversations.get", json!({ "id": created.id }))
                .await
        ),
        &json!("NOT_FOUND")
    );
}

#[tokio::test]
async fn removing_a_dialog_closes_its_terminals() {
    let channels = registered();
    let created: ConversationDto = data(
        &channels
            .server
            .dispatch(
                "chat.conversations.create",
                json!({ "providerId": channels.provider_id, "modelId": "fake-model" }),
            )
            .await,
    );
    channels
        .host
        .terminals
        .create(&zvs_contract::TerminalCreateInput {
            conversation_id: created.id,
            shell: None,
            cols: 80,
            rows: 24,
        })
        .expect("opens a terminal");
    let removed = channels
        .server
        .dispatch("chat.conversations.remove", json!({ "id": created.id }))
        .await;
    assert_eq!(removed, json!({ "ok": true }));
    assert_eq!(channels.host.terminals.open(), 0);
    assert!(channels.host.pty.last().killed());
}

#[tokio::test]
async fn answers_and_skips_for_a_question_that_is_not_pending_conflict() {
    let channels = registered();
    let run = RunId::new(crate::platform::ids::new_id());
    let request = crate::platform::ids::new_id().hyphenated().to_string();
    assert_eq!(
        failed(
            &channels
                .server
                .dispatch(
                    "chat.answer",
                    json!({
                        "id": run,
                        "requestId": request,
                        "answers": [{ "questionId": "q1", "selected": ["o1"] }]
                    })
                )
                .await
        ),
        &json!("CONFLICT")
    );
    assert_eq!(
        failed(
            &channels
                .server
                .dispatch("chat.skip", json!({ "id": run, "requestId": request }))
                .await
        ),
        &json!("CONFLICT")
    );
    assert_eq!(
        failed(
            &channels
                .server
                .dispatch("chat.skip", json!({ "id": run, "requestId": "r1" }))
                .await
        ),
        &json!("VALIDATION_FAILED")
    );
}
