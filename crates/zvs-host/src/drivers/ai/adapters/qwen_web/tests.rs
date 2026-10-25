use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppErrorCode, DeltaKind, HostEventDraft, RunOutcomeStatus, TunableParameter};

use super::*;
use crate::drivers::ai::adapters::account_family::AccountAdapterOptions;
use crate::drivers::ai::mock::{
    MockServer, Recorded, Reply, fixture_frames, fixture_text, search_tool, transport_for,
};
use crate::drivers::ai::ports::ChatMessage;
use crate::drivers::ai::stream_to_events::stream_to_events;
use crate::drivers::ai::tool_dialect::INJECTED_CALL_KEY;
use crate::drivers::ai::transport::{AccountTransport, AccountTransportOptions};
use crate::platform::clock::FakeClock;
use crate::platform::events::{EventBus, EventBusOptions};
use crate::testing::{FakeReply, FakeSessionGateway, MemoryLogger, MemorySink};

const BASE: &str = "/api/v2";
const QWEN_CHAT_ID: &str = "1a77c526-b841-4fef-805c-8096a2b5e4c3";
const FIXED_NOW: i64 = 1_788_994_021_000;
const FIXED_FID: &str = "b222dc0d-1910-427d-b25e-291a8570e556";

fn prompt() -> GenerateRequest {
    GenerateRequest {
        temperature: Some(0.4),
        top_k: Some(40),
        ..GenerateRequest::new("qwen3.7-plus", vec![ChatMessage::user("Привет")])
    }
}

fn fixture(name: &str) -> Value {
    serde_json::from_str(&fixture_text(name)).expect("a JSON fixture")
}

fn route(path: &str) -> String {
    format!("{BASE}{path}")
}

async fn qwen_server(frames: Vec<String>) -> MockServer {
    let server = MockServer::start().await;
    server.reply(
        &route(QWEN_MODELS_PATH),
        Reply::json(fixture("qwenModels.json")),
    );
    server.reply(
        &route(QWEN_CHATS_NEW_PATH),
        Reply::json(fixture("qwenChatCreated.json")),
    );
    server.reply(&route(QWEN_COMPLETIONS_PATH), Reply::chunks(frames));
    server
}

fn options(server: &MockServer) -> AccountAdapterOptions {
    AccountAdapterOptions {
        clock: Arc::new(FakeClock::new(FIXED_NOW)),
        new_id: Arc::new(|| FIXED_FID.to_owned()),
        retry_delay: Duration::ZERO,
        ..AccountAdapterOptions::new(transport_for(&server.url(BASE)))
    }
}

fn adapter(server: &MockServer) -> QwenWebAdapter {
    QwenWebAdapter::new(options(server))
}

fn token() -> CancellationToken {
    CancellationToken::new()
}

async fn drain(stream: DeltaStream) -> Result<Vec<TextDelta>, AppError> {
    stream.collect::<Vec<_>>().await.into_iter().collect()
}

fn texts(deltas: &[TextDelta], wanted: DeltaKind) -> Vec<String> {
    deltas
        .iter()
        .filter_map(|delta| match delta {
            TextDelta::Content { text, kind } if *kind == wanted => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn bus() -> (EventBus, Arc<MemorySink>) {
    let bus = EventBus::new(EventBusOptions::default());
    let sink = MemorySink::shared();
    bus.attach(sink.clone());
    (bus, sink)
}

fn with_query(call: &Recorded) -> String {
    match &call.query {
        Some(query) => format!("{}?{query}", call.path),
        None => call.path.clone(),
    }
}

fn recording_hook() -> (Arc<Mutex<Vec<AppError>>>, AccountAdapterOptions) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let hook: crate::drivers::ai::adapters::account_family::SessionExpiredHook =
        Arc::new(move |error: &AppError| sink.lock().expect("seen").push(error.clone()));
    let options = AccountAdapterOptions {
        on_session_expired: Some(hook),
        ..AccountAdapterOptions::new(transport_for("http://127.0.0.1:9"))
    };
    (seen, options)
}

#[tokio::test]
async fn the_qwen_family_lists_models_from_the_recorded_payload() {
    let server = qwen_server(Vec::new()).await;
    let models = adapter(&server).list_models(&token()).await.expect("lists");
    let call = server.last();
    assert_eq!(call.method, "GET");
    assert_eq!(call.path, "/api/v2/models/");
    assert_eq!(call.header("version").as_deref(), Some(QWEN_WEB_VERSION));
    assert_eq!(call.header("source").as_deref(), Some("web"));
    assert!(
        call.header("referer")
            .is_some_and(|value| value.ends_with('/'))
    );
    assert_eq!(
        models,
        vec![
            DiscoveredModel {
                display_name: "Qwen3.7-Plus".to_owned(),
                family: Some("qwen".to_owned()),
                context_window: Some(131_072),
                max_output: Some(32_768),
                capabilities: vec![ModelCapability::Streaming, ModelCapability::Reasoning],
                ..DiscoveredModel::named("qwen3.7-plus")
            },
            DiscoveredModel {
                display_name: "Qwen3.5-Omni-Plus".to_owned(),
                family: Some("qwen".to_owned()),
                context_window: Some(262_144),
                max_output: Some(65_536),
                capabilities: vec![ModelCapability::Streaming],
                ..DiscoveredModel::named("qwen3.5-omni-plus")
            },
            DiscoveredModel {
                display_name: "Qwen3.5-Plus".to_owned(),
                family: Some("qwen".to_owned()),
                capabilities: vec![ModelCapability::Streaming],
                ..DiscoveredModel::named("qwen3.5-plus")
            },
        ]
    );
    assert_eq!(QWEN_MODELS_OBSERVATION.observed_at, "2026-09-10");
    const { assert!(QWEN_WEB_CAPABILITIES.live_model_list) };
}

#[tokio::test]
async fn a_hostile_or_malformed_qwen_payload_fails_as_unknown_without_leaking_the_body() {
    let secret = "eyJhbGciOi.SUPERSECRET.signature";
    for body in [
        json!({ "success": true, "data": { "data": "not-a-list", "token": secret } }),
        json!({ "success": true, "data": "not-a-list" }),
        json!({ "success": false, "data": { "data": [] } }),
    ] {
        let server = MockServer::start().await;
        server.reply(&route(QWEN_MODELS_PATH), Reply::json(body));
        let logger = MemoryLogger::shared();
        let qwen = QwenWebAdapter::new(AccountAdapterOptions {
            logger: logger.clone(),
            ..options(&server)
        });
        let error = qwen.list_models(&token()).await.expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::Unknown);
        assert_eq!(
            error
                .details
                .as_ref()
                .and_then(|details| details.get("family")),
            Some(&json!("qwen-web"))
        );
        assert!(!format!("{error:?}").contains(secret));
        let entries = logger.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].scope, "ai");
        let fields = Value::Object(entries[0].fields.clone());
        assert_eq!(fields["family"], json!("qwen-web"));
        assert_eq!(fields["endpoint"], json!(QWEN_MODELS_PATH));
        assert_eq!(fields["status"], json!(200));
        assert_eq!(fields["contentType"], json!("application/json"));
        assert_eq!(fields["payloadShape"], json!("object"));
        assert!(fields["rootFields"].as_array().is_some());
        assert!(!format!("{:?}", entries[0]).contains(secret));
    }
    let broken = MockServer::start().await;
    broken.reply(&route(QWEN_MODELS_PATH), Reply::text("<!doctype html>"));
    assert_eq!(
        adapter(&broken)
            .list_models(&token())
            .await
            .expect_err("refuses")
            .code,
        AppErrorCode::Unknown
    );
}

#[test]
fn malformed_model_entries_are_skipped_without_discarding_valid_models() {
    let models = map_qwen_models(&json!({
        "success": true,
        "data": {
            "data": [
                { "id": 42, "name": "malformed" },
                {
                    "id": "qwen3.7-plus",
                    "name": "Qwen3.7-Plus",
                    "info": { "meta": { "max_context_length": "unknown" } }
                },
                {
                    "id": "qwen3.5-plus",
                    "name": "Qwen3.5-Plus",
                    "info": { "is_active": true }
                }
            ]
        }
    }))
    .expect("the valid envelope survives malformed individual rows");

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].external_id, "qwen3.5-plus");
}

#[tokio::test]
async fn a_rejected_account_session_surfaces_as_session_expired() {
    let session = FakeSessionGateway::shared();
    session.queue(
        "https://chat.qwen.ai/api/v2/models/",
        vec![FakeReply::json(401, &json!({ "detail": "expired" }))],
    );
    let qwen = QwenWebAdapter::new(AccountAdapterOptions::new(Arc::new(AccountTransport::new(
        AccountTransportOptions::new("https://chat.qwen.ai/api/v2", session),
    ))));
    assert_eq!(
        qwen.list_models(&token()).await.expect_err("expired").code,
        AppErrorCode::ProviderSessionExpired
    );
}

#[tokio::test]
async fn unknown_model_capability_values_default_to_fast_mode() {
    let server = MockServer::start().await;
    server.reply(
        &route(QWEN_MODELS_PATH),
        Reply::json(json!({
            "success": true,
            "data": {
                "data": [{
                    "id": "qwen3.7-plus",
                    "info": {
                        "meta": {
                            "capabilities": { "thinking": "enabled" }
                        }
                    }
                }]
            }
        })),
    );
    server.reply(
        &route(QWEN_CHATS_NEW_PATH),
        Reply::json(fixture("qwenChatCreated.json")),
    );
    server.reply(
        &route(QWEN_COMPLETIONS_PATH),
        Reply::chunks(fixture_frames("qwenStream.sse")),
    );
    let logger = MemoryLogger::shared();
    let qwen = QwenWebAdapter::new(AccountAdapterOptions {
        logger: logger.clone(),
        ..options(&server)
    });

    let result = qwen
        .generate(&prompt(), &token())
        .await
        .expect("generation proceeds with conservative defaults");

    assert_eq!(result.text, "Привет, мир!");
    assert_eq!(
        server.last_body()["messages"][0]["feature_config"]["thinking_mode"],
        json!("Fast")
    );
    assert!(!logger.entries().iter().any(|entry| {
        entry.message == "Qwen returned model entries with an unsupported schema"
    }));
}

#[tokio::test]
async fn qwen_error_envelopes_do_not_fall_back_to_generation() {
    let server = MockServer::start().await;
    server.reply(
        &route(QWEN_MODELS_PATH),
        Reply::json(json!({
            "success": false,
            "error": "upstream temporarily unavailable"
        })),
    );
    let qwen = adapter(&server);

    let error = qwen
        .generate(&prompt(), &token())
        .await
        .expect_err("an explicit upstream error must stop generation");

    assert_eq!(error.message, MALFORMED_MODELS_MESSAGE);
    assert_eq!(server.calls().len(), 1);
    assert_eq!(
        error.details.expect("response diagnostics")["rootFields"],
        json!(["success", "error"])
    );
}

#[test]
fn the_account_family_declares_its_honoured_parameters_honestly() {
    let capabilities = QWEN_WEB_CAPABILITIES;
    assert_eq!(capabilities.auth_modes, [AuthMode::Account]);
    assert!(!capabilities.embedding);
    assert!(!capabilities.image);
    assert_eq!(capabilities.honours, Honours::NONE);
    assert_eq!(capabilities.tools, ToolMode::Injected);
    const { assert!(QWEN_WEB_CAPABILITIES.streaming) };
}

#[tokio::test]
async fn the_qwen_family_opens_a_chat_and_streams_reasoning_and_answer_as_distinct_deltas() {
    let server = qwen_server(fixture_frames("qwenStream.sse")).await;
    let deltas = drain(adapter(&server).stream(&prompt(), &token()))
        .await
        .expect("streams");
    let kinds = deltas
        .iter()
        .map(|delta| match delta {
            TextDelta::Content { kind, .. } => *kind,
            TextDelta::ToolCall(_) => panic!("no tool calls"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            DeltaKind::Reasoning,
            DeltaKind::Reasoning,
            DeltaKind::Text,
            DeltaKind::Text,
            DeltaKind::Text
        ]
    );
    assert_eq!(texts(&deltas, DeltaKind::Text), ["Привет", ", мир", "!"]);
    assert_eq!(
        texts(&deltas, DeltaKind::Reasoning),
        [
            "Пользователь здоровается.\nОтвечу коротким приветствием.",
            "Достаточно одного предложения без лишних деталей."
        ]
    );

    let calls = server.calls();
    let create = &calls[1];
    assert_eq!(create.method, "POST");
    assert_eq!(create.path, "/api/v2/chats/new");
    assert_eq!(
        create.json(),
        json!({
            "chatId": "",
            "chat_mode": "normal",
            "chat_type": "t2t",
            "models": ["qwen3.7-plus"],
            "project_id": "",
            "timestamp": FIXED_NOW,
        })
    );
    assert_eq!(create.header("version").as_deref(), Some(QWEN_WEB_VERSION));
    assert_eq!(create.header("source").as_deref(), Some("web"));
    assert!(
        create
            .header("referer")
            .is_some_and(|value| value.ends_with("/c/new-chat"))
    );
    let completion = &calls[2];
    assert_eq!(
        completion.header("accept").as_deref(),
        Some("application/json")
    );
    assert_eq!(
        completion.header("x-accel-buffering").as_deref(),
        Some("no")
    );
    assert_eq!(
        completion.header("x-request-id").as_deref(),
        Some(FIXED_FID)
    );
    assert_eq!(
        completion.header("version").as_deref(),
        Some(QWEN_WEB_VERSION)
    );
    assert_eq!(completion.header("source").as_deref(), Some("web"));
    assert!(
        completion
            .header("referer")
            .is_some_and(|value| value.ends_with(&format!("/c/{QWEN_CHAT_ID}")))
    );
    assert_eq!(
        with_query(completion),
        format!("/api/v2/chat/completions?chat_id={QWEN_CHAT_ID}")
    );
    let body = completion.json();
    assert_eq!(body["chat_id"], json!(QWEN_CHAT_ID));
    assert_eq!(body["chatId"], json!(QWEN_CHAT_ID));
    assert_eq!(body["model"], json!("qwen3.7-plus"));
    assert_eq!(body["stream"], json!(true));
    assert_eq!(body["incremental_output"], json!(true));
    assert_eq!(body["version"], json!("2.1"));
    assert_eq!(body["parentId"], json!(""));
    assert_eq!(body["parent_id"], Value::Null);
    assert_eq!(body["timestamp"], json!(FIXED_NOW / 1000));
    let message = &body["messages"][0];
    assert_eq!(message["fid"], json!(FIXED_FID));
    assert_eq!(message["role"], json!("user"));
    assert_eq!(message["content"], json!("Привет"));
    assert_eq!(message["parent_id"], Value::Null);
    assert_eq!(message["childrenIds"], json!([FIXED_FID]));
    assert_eq!(message["models"], json!(["qwen3.7-plus"]));
    assert_eq!(
        String::from_utf8_lossy(&completion.body)
            .split("\"messages\"")
            .next()
            .expect("a prefix"),
        format!(
            r#"{{"chatId":"{QWEN_CHAT_ID}","chat_id":"{QWEN_CHAT_ID}","chat_mode":"normal","chat_type":"t2t","incremental_output":true,"#
        )
    );
    assert_eq!(QWEN_GENERATION_OBSERVATION.observed_at, "2026-10-24");
}

#[tokio::test]
async fn qwen_generate_collects_the_answer_the_reasoning_and_the_vendor_usage() {
    let server = qwen_server(fixture_frames("qwenStream.sse")).await;
    let result = adapter(&server)
        .generate(&prompt(), &token())
        .await
        .expect("generates");
    assert_eq!(result.model, "qwen3.7-plus");
    assert_eq!(result.text, "Привет, мир!");
    assert_eq!(
        result.reasoning.as_deref(),
        Some(
            "Пользователь здоровается.\nОтвечу коротким приветствием.Достаточно одного предложения без лишних деталей."
        )
    );
    assert_eq!(result.finish_reason, FinishReason::Stop);
    assert_eq!(
        result.usage,
        Some(TokenUsage {
            prompt_tokens: Some(621),
            completion_tokens: Some(186),
            total_tokens: Some(807),
        })
    );
}

#[tokio::test]
async fn qwen_selects_thinking_from_discovery_and_reuses_the_model_list() {
    let server = qwen_server(fixture_frames("qwenStream.sse")).await;
    let qwen = adapter(&server);
    qwen.list_models(&token()).await.expect("lists");
    for (model, thinking) in [
        ("qwen3.7-plus", true),
        ("qwen3.5-omni-plus", false),
        ("qwen3.5-plus", false),
        ("unlisted-model", false),
    ] {
        qwen.generate(
            &GenerateRequest {
                model: model.to_owned(),
                ..prompt()
            },
            &token(),
        )
        .await
        .expect("generates");
        let config = &server.last_body()["messages"][0]["feature_config"];
        assert_eq!(config["thinking_enabled"], json!(thinking), "{model}");
        assert_eq!(
            config["thinking_mode"],
            json!(if thinking { "Auto" } else { "Fast" }),
            "{model}"
        );
        assert_eq!(config["auto_thinking"], json!(thinking), "{model}");
    }
    assert_eq!(
        server
            .paths()
            .iter()
            .filter(|path| *path == "/api/v2/models/")
            .count(),
        1
    );
}

#[test]
fn qwen_uses_legacy_thinking_only_when_the_explicit_capability_is_absent() {
    for (capability, ability, expected) in [
        (json!(false), json!(2), false),
        (json!(0), json!(2), false),
        (json!(true), json!(0), true),
        (Value::Null, json!(2), true),
        (Value::Null, json!(1), true),
        (Value::Null, Value::Null, false),
    ] {
        let models = map_qwen_models(&json!({
            "data": [{
                "id": "model",
                "info": {
                    "meta": {
                        "capabilities": { "thinking": capability, "vision": true },
                        "abilities": { "thinking": ability },
                        "modality": ["image", "audio", "video"],
                    },
                },
            }],
        }))
        .expect("maps");
        let wanted = if expected {
            vec![ModelCapability::Streaming, ModelCapability::Reasoning]
        } else {
            vec![ModelCapability::Streaming]
        };
        assert_eq!(models[0].capabilities, wanted, "{capability} {ability}");
    }
    assert!(
        map_qwen_models(
            &json!({ "data": [{ "id": "m", "info": { "meta": { "max_context_length": 0 } } }] })
        )
        .is_none()
    );
    let deduped = map_qwen_models(&json!({
        "data": [
            { "id": " a " },
            { "id": "a" },
            { "id": "" },
            { "id": "b", "info": { "is_active": false } },
            { "id": "c", "name": "  ", "info": { "name": "Named" } },
        ],
    }))
    .expect("maps");
    assert_eq!(
        deduped
            .iter()
            .map(|model| (model.external_id.as_str(), model.display_name.as_str()))
            .collect::<Vec<_>>(),
        [("a", "a"), ("c", "Named")]
    );
}

#[tokio::test]
async fn qwen_standalone_sse_status_preserves_partial_text_and_expires_the_session_once() {
    for status in [401, 403] {
        let server = qwen_server(fixture_frames(&format!("qwenStreamStatus{status}.sse"))).await;
        let (seen, hooked) = recording_hook();
        let qwen = QwenWebAdapter::new(AccountAdapterOptions {
            on_session_expired: hooked.on_session_expired,
            ..options(&server)
        });
        let (bus, _sink) = bus();
        let stream = bus.open_stream().expect("a stream");
        let result = stream_to_events(&stream, qwen.stream(&prompt(), &token())).await;
        assert_eq!(result.text, "Partial");
        assert_eq!(
            result.outcome.code,
            Some(AppErrorCode::ProviderSessionExpired)
        );
        let seen = seen.lock().expect("seen");
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen[0]
                .details
                .as_ref()
                .and_then(|details| details.get("status")),
            Some(&json!(status))
        );
    }
}

#[tokio::test]
async fn qwen_recognises_standalone_authentication_codes_and_numeric_strings() {
    for code in [
        json!(401),
        json!(403),
        json!("401"),
        json!("403"),
        json!("session_expired"),
    ] {
        let server = qwen_server(vec![format!("data: {}\n\n", json!({ "code": code }))]).await;
        assert_eq!(
            adapter(&server)
                .generate(&prompt(), &token())
                .await
                .expect_err("expired")
                .code,
            AppErrorCode::ProviderSessionExpired,
            "{code}"
        );
    }
}

#[tokio::test]
async fn qwen_allows_success_status_frames_but_rejects_standalone_server_errors() {
    let mut frames = vec!["data: {\"status\":200}\n\n".to_owned()];
    frames.extend(fixture_frames("qwenStream.sse"));
    let successful = qwen_server(frames).await;
    assert_eq!(
        adapter(&successful)
            .generate(&prompt(), &token())
            .await
            .expect("generates")
            .text,
        "Привет, мир!"
    );
    let failed = qwen_server(vec!["data: {\"status\":500}\n\n".to_owned()]).await;
    let error = adapter(&failed)
        .generate(&prompt(), &token())
        .await
        .expect_err("fails");
    assert_eq!(error.code, AppErrorCode::Unknown);
    assert_eq!(error.message, GENERATION_ABORTED_MESSAGE);
    assert_eq!(
        error
            .details
            .as_ref()
            .and_then(|details| details.get("status")),
        Some(&json!(500))
    );
    let errored = qwen_server(vec!["data: {\"error\":\"overloaded\"}\n\n".to_owned()]).await;
    assert_eq!(
        adapter(&errored)
            .generate(&prompt(), &token())
            .await
            .expect_err("fails")
            .message,
        GENERATION_ABORTED_MESSAGE
    );
}

#[tokio::test]
async fn qwen_deltas_reach_the_event_bus_with_reasoning_kept_apart_from_the_answer() {
    let server = qwen_server(fixture_frames("qwenStream.sse")).await;
    let (bus, sink) = bus();
    let stream = bus.open_stream().expect("a stream");
    let result = stream_to_events(&stream, adapter(&server).stream(&prompt(), &token())).await;
    assert_eq!(result.outcome.status, RunOutcomeStatus::Ok);
    assert_eq!(result.text, "Привет, мир!");
    assert!(!result.reasoning.is_empty());
    let events = sink.events();
    let plain = events
        .iter()
        .filter_map(|event| match &event.body {
            HostEventDraft::Token { delta, kind: None } => Some(delta.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(plain, ["Привет", ", мир", "!"]);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                event.body,
                HostEventDraft::Token {
                    kind: Some(DeltaKind::Reasoning),
                    ..
                }
            ))
            .count(),
        2
    );
    assert!(matches!(
        events.last().expect("an end").body,
        HostEventDraft::End { .. }
    ));
}

#[tokio::test]
async fn aborting_a_qwen_stream_stops_emission_and_ends_the_run_as_cancelled() {
    let server = MockServer::start().await;
    server.reply(
        &route(QWEN_MODELS_PATH),
        Reply::json(fixture("qwenModels.json")),
    );
    server.reply(
        &route(QWEN_CHATS_NEW_PATH),
        Reply::json(fixture("qwenChatCreated.json")),
    );
    server.reply(
        &route(QWEN_COMPLETIONS_PATH),
        Reply::chunks(fixture_frames("qwenStream.sse")).with_gap(Duration::from_millis(40)),
    );
    let cancel = token();
    let trigger = cancel.clone();
    let (bus, sink) = bus();
    let stream = bus.open_stream().expect("a stream");
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        trigger.cancel();
    });
    let result = stream_to_events(&stream, adapter(&server).stream(&prompt(), &cancel)).await;
    assert_eq!(result.outcome.status, RunOutcomeStatus::Cancelled);
    assert_eq!(result.outcome.code, Some(AppErrorCode::RunCancelled));
    assert!(server.wait_for_dropped_stream().await);
    assert!(matches!(
        sink.events().last().expect("an end").body,
        HostEventDraft::End { .. }
    ));
}

#[tokio::test]
async fn a_session_that_expires_mid_stream_keeps_the_partial_text_and_asks_for_a_re_link() {
    let server = qwen_server(fixture_frames("qwenStreamExpired.sse")).await;
    let (seen, hooked) = recording_hook();
    let qwen = QwenWebAdapter::new(AccountAdapterOptions {
        on_session_expired: hooked.on_session_expired,
        ..options(&server)
    });
    let (bus, _sink) = bus();
    let stream = bus.open_stream().expect("a stream");
    let result = stream_to_events(&stream, qwen.stream(&prompt(), &token())).await;
    assert_eq!(result.text, "Прив");
    assert_eq!(result.outcome.status, RunOutcomeStatus::Failed);
    assert_eq!(
        result.outcome.code,
        Some(AppErrorCode::ProviderSessionExpired)
    );
    let seen = seen.lock().expect("seen");
    assert_eq!(seen.len(), 1);
    let details = seen[0].details.clone().expect("details");
    assert_eq!(details["status"], json!(401));
    assert_eq!(details["vendorCode"], json!("Unauthorized"));
}

#[tokio::test]
async fn a_truncated_qwen_stream_is_an_error_rather_than_a_short_answer() {
    let server = qwen_server(fixture_frames("qwenStreamTruncated.sse")).await;
    let (bus, _sink) = bus();
    let stream = bus.open_stream().expect("a stream");
    let result = stream_to_events(&stream, adapter(&server).stream(&prompt(), &token())).await;
    assert_eq!(result.text, "Привет, мир");
    assert_eq!(result.outcome.status, RunOutcomeStatus::Failed);
    assert_eq!(result.outcome.code, Some(AppErrorCode::ProviderUnreachable));
}

#[tokio::test]
async fn qwen_generation_logs_dropped_parameters_and_never_a_request_or_response_body() {
    let server = qwen_server(fixture_frames("qwenStream.sse")).await;
    let logger = MemoryLogger::shared();
    let qwen = QwenWebAdapter::new(AccountAdapterOptions {
        logger: logger.clone(),
        ..options(&server)
    });
    drain(qwen.stream(&prompt(), &token()))
        .await
        .expect("streams");
    let entries = logger.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].scope, "ai");
    assert_eq!(
        entries[0].message,
        "Dropped parameters the adapter family does not honour"
    );
    assert_eq!(
        Value::Object(entries[0].fields.clone()),
        json!({
            "family": "qwen-web",
            "dropped": [TunableParameter::Temperature, TunableParameter::TopK],
        })
    );
    let logged = format!("{entries:?}");
    for secret in ["Привет", QWEN_CHAT_ID, FIXED_FID] {
        assert!(!logged.contains(secret), "{secret}");
    }
}

#[tokio::test]
async fn a_malformed_chat_create_reply_fails_before_any_completion_request_is_made() {
    let server = MockServer::start().await;
    server.reply(
        &route(QWEN_MODELS_PATH),
        Reply::json(fixture("qwenModels.json")),
    );
    server.reply(
        &route(QWEN_CHATS_NEW_PATH),
        Reply::json(json!({ "success": true, "data": { "id": "" } })),
    );
    let logger = MemoryLogger::shared();
    let qwen = QwenWebAdapter::new(AccountAdapterOptions {
        logger: logger.clone(),
        ..options(&server)
    });
    let error = drain(qwen.stream(&prompt(), &token()))
        .await
        .expect_err("refuses");
    assert_eq!(error.code, AppErrorCode::Unknown);
    assert_eq!(
        error
            .details
            .as_ref()
            .and_then(|details| details.get("endpoint")),
        Some(&json!(QWEN_CHATS_NEW_PATH))
    );
    assert_eq!(server.calls().len(), 2);
    assert!(!format!("{:?}", logger.entries()).contains("Привет"));
}

#[tokio::test]
async fn the_qwen_driver_emits_a_native_tool_call_from_a_recorded_injected_stream() {
    let server = qwen_server(fixture_frames("qwenToolCall.sse")).await;
    let driver = qwen_web_driver(options(&server));
    let deltas = drain(driver.stream(
        &GenerateRequest {
            tools: vec![search_tool()],
            ..GenerateRequest::new("qwen3.7-plus", vec![ChatMessage::user("rotate?")])
        },
        &token(),
    ))
    .await
    .expect("streams");
    let [TextDelta::ToolCall(call)] = deltas.as_slice() else {
        panic!("one tool call: {deltas:?}");
    };
    assert!(call.id.starts_with("call_"));
    assert_eq!(
        (call.name.as_str(), call.arguments.as_str()),
        ("docs__search", r#"{"query":"rotate token"}"#)
    );
    let sent = server.last_body().to_string();
    assert!(sent.contains("AVAILABLE TOOLS"));
    assert!(sent.contains(INJECTED_CALL_KEY));
}

struct DirectSession {
    http: crate::drivers::ai::http::HttpClient,
}

#[async_trait::async_trait]
impl crate::drivers::ai::transport::SessionGateway for DirectSession {
    fn partition(&self) -> &str {
        "live-qwen-test"
    }

    fn user_agent(&self) -> String {
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36".to_owned()
    }

    fn account_token(&self, _url: &str) -> Option<crate::drivers::ai::transport::AccountToken> {
        None
    }

    async fn fetch(
        &self,
        url: &str,
        request: crate::drivers::ai::transport::SessionRequest,
        _cancel: &CancellationToken,
    ) -> Result<crate::drivers::ai::transport::SessionResponse, AppError> {
        let method = reqwest::Method::from_bytes(request.method.as_bytes())
            .map_err(|_| crate::drivers::ai::errors::request_failed())?;
        let mut outgoing = self.http.client().request(method.clone(), url);
        for (name, value) in &request.headers {
            outgoing = outgoing.header(name, value);
        }
        if let Some(body) = request.body {
            outgoing = outgoing.body(body);
        }
        let response = outgoing
            .send()
            .await
            .map_err(|error| crate::drivers::ai::errors::network_failure(&error))?;
        let mut headers = crate::drivers::ai::errors::Headers::new();
        for (name, value) in response.headers() {
            headers.insert(
                name.as_str().to_ascii_lowercase(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            );
        }
        let status = response.status().as_u16();
        println!(
            "Qwen {method} {}: {status} {}",
            reqwest::Url::parse(url)
                .map(|url| url.path().to_owned())
                .unwrap_or_default(),
            headers.get("content-type").cloned().unwrap_or_default()
        );
        let body = response
            .bytes_stream()
            .map(|chunk| {
                chunk
                    .map(|bytes| bytes.to_vec())
                    .map_err(|error| crate::drivers::ai::errors::network_failure(&error))
            })
            .boxed();
        Ok(crate::drivers::ai::transport::SessionResponse::new(
            status, headers, body,
        ))
    }
}

struct LiveCredentials(String);

#[async_trait::async_trait]
impl crate::drivers::ai::transport::AccountCredentials for LiveCredentials {
    async fn current(
        &self,
        _cancel: &CancellationToken,
    ) -> Result<Option<crate::drivers::ai::transport::AccountToken>, AppError> {
        Ok(Some(crate::drivers::ai::transport::AccountToken::bearer(
            self.0.clone(),
        )))
    }

    async fn refresh(
        &self,
        _cancel: &CancellationToken,
    ) -> Result<Option<crate::drivers::ai::transport::AccountToken>, AppError> {
        Ok(None)
    }
}

#[ignore = "creates up to two vendor chats on chat.qwen.ai; set LIVE_QWEN_TOKEN to a captured bearer token"]
#[tokio::test]
async fn live_qwen_identity_models_and_thinking_and_fast_generation() {
    use crate::drivers::ai::identity::map_qwen_identity;
    use crate::drivers::ai::mock::direct_http;

    let live = std::env::var("LIVE_QWEN_TOKEN").unwrap_or_default();
    if live.is_empty() {
        return;
    }
    let session: crate::drivers::ai::transport::SharedSessionGateway = Arc::new(DirectSession {
        http: direct_http(),
    });
    let credentials: crate::drivers::ai::transport::SharedAccountCredentials =
        Arc::new(LiveCredentials(live));
    let transport = |base: &str| -> crate::drivers::ai::transport::SharedTransport {
        Arc::new(AccountTransport::new(AccountTransportOptions {
            credentials: Some(credentials.clone()),
            ..AccountTransportOptions::new(base, session.clone())
        }))
    };
    let identity = transport("https://chat.qwen.ai/api/v1")
        .request(&RequestSpec::get("/auths/"), &token())
        .await
        .expect("an identity");
    let identity =
        map_qwen_identity(&serde_json::from_str(&identity.text).expect("json")).expect("maps");
    assert!(!identity.identity.external_id.is_empty());
    let qwen = QwenWebAdapter::new(AccountAdapterOptions::new(transport(
        "https://chat.qwen.ai/api/v2",
    )));
    let models = qwen.list_models(&token()).await.expect("models");
    assert!(!models.is_empty());
    println!("Qwen discovered {} models", models.len());
    for thinking in [true, false] {
        let model = models
            .iter()
            .find(|model| model.capabilities.contains(&ModelCapability::Reasoning) == thinking)
            .expect("a model for the mode");
        let result = qwen
            .generate(
                &GenerateRequest::new(
                    model.external_id.clone(),
                    vec![ChatMessage::user("Reply with just the word Hello.")],
                ),
                &token(),
            )
            .await
            .expect("generates");
        assert_eq!(result.finish_reason, FinishReason::Stop);
        assert!(!result.text.trim().is_empty());
        println!(
            "Qwen {}: completed, answer characters={}",
            if thinking { "Thinking" } else { "Fast" },
            result.text.chars().count()
        );
    }
}

fn count(server: &MockServer, path: &str) -> usize {
    server
        .paths()
        .iter()
        .filter(|recorded| recorded.as_str() == path)
        .count()
}

const FIXTURE_RESPONSE_ID: &str = "3112202d-2c44-4b3f-8fc8-783e8b17dbc0";

fn follow_up(answer: &str, question: &str) -> GenerateRequest {
    GenerateRequest::new(
        "qwen3.7-plus",
        vec![
            ChatMessage::user("Привет"),
            ChatMessage::assistant(answer),
            ChatMessage::user(question),
        ],
    )
}

fn completions(server: &MockServer) -> Vec<Value> {
    server
        .calls()
        .iter()
        .filter(|call| call.path == route(QWEN_COMPLETIONS_PATH))
        .map(Recorded::json)
        .collect()
}

#[tokio::test]
async fn a_follow_up_continues_the_vendor_chat_with_only_the_new_message() {
    let server = qwen_server(fixture_frames("qwenStream.sse")).await;
    let qwen = adapter(&server);
    qwen.generate(&prompt(), &token()).await.expect("generates");
    assert_eq!(qwen.chained_chats(), 1);
    qwen.generate(&follow_up("Привет, мир!", "Как дела?"), &token())
        .await
        .expect("continues");
    assert_eq!(count(&server, &route(QWEN_CHATS_NEW_PATH)), 1);
    let bodies = completions(&server);
    assert_eq!(bodies.len(), 2);
    let second = &bodies[1];
    assert_eq!(second["chat_id"], bodies[0]["chat_id"]);
    assert_eq!(second["parent_id"], json!(FIXTURE_RESPONSE_ID));
    assert_eq!(second["parentId"], json!(FIXTURE_RESPONSE_ID));
    assert_eq!(
        second["messages"][0]["parent_id"],
        json!(FIXTURE_RESPONSE_ID)
    );
    assert_eq!(second["messages"][0]["content"], json!("Как дела?"));
    assert_eq!(qwen.chained_chats(), 2);
}

#[tokio::test]
async fn a_dialog_the_adapter_has_not_seen_opens_its_own_chat_with_the_whole_history() {
    let server = qwen_server(fixture_frames("qwenStream.sse")).await;
    let qwen = adapter(&server);
    qwen.generate(&prompt(), &token()).await.expect("generates");
    let edited = GenerateRequest {
        system: Some("Отвечай кратко".to_owned()),
        ..follow_up("Привет, мир!", "Как дела?")
    };
    qwen.generate(&edited, &token()).await.expect("generates");
    let mut changed_question = follow_up("Привет, мир!", "Как дела?");
    changed_question.messages[0] = ChatMessage::user("Здравствуйте");
    qwen.generate(&changed_question, &token())
        .await
        .expect("generates");
    assert_eq!(count(&server, &route(QWEN_CHATS_NEW_PATH)), 3);
    let bodies = completions(&server);
    assert_eq!(bodies[1]["parent_id"], Value::Null);
    assert_eq!(
        bodies[1]["messages"][0]["content"],
        json!(
            "System: Отвечай кратко\n\nUser: Привет\n\nAssistant: Привет, мир!\n\nUser: Как дела?"
        )
    );
    assert_eq!(bodies[2]["parent_id"], Value::Null);
}

#[tokio::test]
async fn the_vendor_keeps_its_own_answer_so_a_reworded_reply_still_continues_the_chat() {
    let server = qwen_server(fixture_frames("qwenStream.sse")).await;
    let qwen = adapter(&server);
    qwen.generate(&prompt(), &token()).await.expect("generates");
    qwen.generate(&follow_up("Другой ответ", "Как дела?"), &token())
        .await
        .expect("continues");
    assert_eq!(count(&server, &route(QWEN_CHATS_NEW_PATH)), 1);
    assert_eq!(
        completions(&server)[1]["parent_id"],
        json!(FIXTURE_RESPONSE_ID)
    );
}

#[tokio::test]
async fn a_refused_chained_turn_is_retried_once_in_a_new_chat_and_the_dead_chat_is_forgotten() {
    let server = qwen_server(fixture_frames("qwenStream.sse")).await;
    let qwen = adapter(&server);
    qwen.generate(&prompt(), &token()).await.expect("generates");
    server.reply(
        &route(QWEN_COMPLETIONS_PATH),
        Reply::json(json!({ "detail": "chat not found" })).with_status(404),
    );
    let error = qwen
        .generate(&follow_up("Привет, мир!", "Как дела?"), &token())
        .await
        .expect_err("the fresh chat fails too");
    assert_ne!(error.code, AppErrorCode::ProviderSessionExpired);
    assert_eq!(count(&server, &route(QWEN_CHATS_NEW_PATH)), 2);
    assert_eq!(count(&server, &route(QWEN_COMPLETIONS_PATH)), 3);
    assert_eq!(qwen.chained_chats(), 0);
    let retried = &completions(&server)[2];
    assert_eq!(retried["parent_id"], Value::Null);
    assert_eq!(
        retried["messages"][0]["content"],
        json!("User: Привет\n\nAssistant: Привет, мир!\n\nUser: Как дела?")
    );
    server.reply(
        &route(QWEN_COMPLETIONS_PATH),
        Reply::chunks(fixture_frames("qwenStream.sse")),
    );
    qwen.generate(&follow_up("Привет, мир!", "Как дела?"), &token())
        .await
        .expect("generates");
    assert_eq!(count(&server, &route(QWEN_CHATS_NEW_PATH)), 3);
    assert_eq!(qwen.chained_chats(), 1);
}

#[tokio::test]
async fn a_rate_limited_chained_turn_is_not_retried() {
    let server = qwen_server(fixture_frames("qwenStream.sse")).await;
    let qwen = adapter(&server);
    qwen.generate(&prompt(), &token()).await.expect("generates");
    server.reply(
        &route(QWEN_COMPLETIONS_PATH),
        Reply::json(json!({ "detail": "too many requests" })).with_status(429),
    );
    let error = qwen
        .generate(&follow_up("Привет, мир!", "Как дела?"), &token())
        .await
        .expect_err("limited");
    assert_eq!(error.code, AppErrorCode::RateLimited);
    assert_eq!(count(&server, &route(QWEN_CHATS_NEW_PATH)), 1);
    assert_eq!(count(&server, &route(QWEN_COMPLETIONS_PATH)), 2);
    assert_eq!(qwen.chained_chats(), 1);
}

#[tokio::test]
async fn an_expired_session_forgets_every_chain() {
    let server = qwen_server(fixture_frames("qwenStream.sse")).await;
    let qwen = adapter(&server);
    qwen.generate(&prompt(), &token()).await.expect("generates");
    assert_eq!(qwen.chained_chats(), 1);
    server.reply(
        &route(QWEN_COMPLETIONS_PATH),
        Reply::chunks(fixture_frames("qwenStreamStatus401.sse")),
    );
    let error = qwen
        .generate(&follow_up("Привет, мир!", "Как дела?"), &token())
        .await
        .expect_err("expires");
    assert_eq!(error.code, AppErrorCode::ProviderSessionExpired);
    assert_eq!(qwen.chained_chats(), 0);
    assert_eq!(count(&server, &route(QWEN_CHATS_NEW_PATH)), 1);
}

#[tokio::test]
async fn the_model_list_is_refreshed_after_its_lifetime_and_a_failed_refresh_keeps_the_old_one() {
    let server = qwen_server(fixture_frames("qwenStream.sse")).await;
    let clock = Arc::new(FakeClock::new(FIXED_NOW));
    let qwen = QwenWebAdapter::new(AccountAdapterOptions {
        clock: clock.clone(),
        ..options(&server)
    });
    qwen.generate(&prompt(), &token()).await.expect("generates");
    clock.advance(MODELS_TTL_MS - 1);
    qwen.generate(&prompt(), &token()).await.expect("generates");
    assert_eq!(count(&server, "/api/v2/models/"), 1);
    clock.advance(1);
    server.reply(
        &route(QWEN_MODELS_PATH),
        Reply::json(json!({ "detail": "busy" })).with_status(503),
    );
    qwen.generate(&prompt(), &token())
        .await
        .expect("uses the old list");
    assert_eq!(count(&server, "/api/v2/models/"), 2);
    assert_eq!(
        server.last_body()["messages"][0]["feature_config"]["thinking_mode"],
        json!("Auto")
    );
}
