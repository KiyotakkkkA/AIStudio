use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures::StreamExt;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppErrorCode, DeltaKind};
use zvs_zai_web::SignedRequest;

use super::*;
use crate::drivers::ai::adapters::account_family::AccountAdapterOptions;
use crate::drivers::ai::errors::BROWSER_VERIFICATION_MESSAGE;
use crate::drivers::ai::mock::{
    MockServer, Recorded, Reply, fixture_frames, fixture_text, transport_for,
};
use crate::drivers::ai::ports::ChatMessage;
use crate::platform::clock::FakeClock;

const BASE: &str = "/api";
const FIXED_NOW: i64 = 1_792_867_620_312;
const USER_ID: &str = "5f0c2b1e-8a43-4d7e-9c1a-2b3c4d5e6f70";
const CHAT_ID: &str = "0d9f6c1a-3b2e-4f5a-8c7d-6e5f4a3b2c1d";

fn fixture(name: &str) -> Value {
    serde_json::from_str(&fixture_text(name)).expect("a JSON fixture")
}

fn route(path: &str) -> String {
    format!("{BASE}{path}")
}

async fn zai_server(frames: Vec<String>) -> MockServer {
    let server = MockServer::start().await;
    server.reply(
        &route(ZAI_MODELS_PATH),
        Reply::json(fixture("zaiModels.json")),
    );
    server.reply(&route(ZAI_USER_PATH), Reply::json(fixture("zaiUser.json")));
    server.reply(
        &route(ZAI_CHATS_NEW_PATH),
        Reply::json(fixture("zaiChatCreated.json")),
    );
    server.reply(&route(ZAI_COMPLETIONS_PATH), Reply::chunks(frames));
    server
}

fn counting_ids() -> crate::drivers::ai::adapters::account_family::NewId {
    let next = Arc::new(AtomicUsize::new(0));
    Arc::new(move || format!("id-{}", next.fetch_add(1, Ordering::SeqCst) + 1))
}

fn adapter(server: &MockServer) -> ZaiWebAdapter {
    ZaiWebAdapter::new(AccountAdapterOptions {
        clock: Arc::new(FakeClock::new(FIXED_NOW)),
        new_id: counting_ids(),
        retry_delay: Duration::ZERO,
        ..AccountAdapterOptions::new(transport_for(&server.url(BASE)))
    })
}

fn prompt() -> GenerateRequest {
    GenerateRequest::new("glm-5.2", vec![ChatMessage::user("Привет")])
}

fn follow_up() -> GenerateRequest {
    GenerateRequest::new(
        "glm-5.2",
        vec![
            ChatMessage::user("Привет"),
            ChatMessage::assistant("Привет, мир!"),
            ChatMessage::user("Как дела?"),
        ],
    )
}

fn token() -> CancellationToken {
    CancellationToken::new()
}

fn count(server: &MockServer, path: &str) -> usize {
    server
        .paths()
        .iter()
        .filter(|recorded| recorded.as_str() == route(path))
        .count()
}

fn completions(server: &MockServer) -> Vec<Recorded> {
    server
        .calls()
        .into_iter()
        .filter(|call| call.path == route(ZAI_COMPLETIONS_PATH))
        .collect()
}

fn query(call: &Recorded) -> Vec<(String, String)> {
    url::form_urlencoded::parse(call.query.as_deref().unwrap_or_default().as_bytes())
        .into_owned()
        .collect()
}

#[tokio::test]
async fn visible_models_are_listed_with_their_reasoning_and_output_limit() {
    let server = zai_server(Vec::new()).await;
    let models = adapter(&server).list_models(&token()).await.expect("lists");
    assert_eq!(
        models,
        vec![
            DiscoveredModel {
                display_name: "GLM-5.3-Flash".to_owned(),
                family: Some("glm".to_owned()),
                max_output: Some(128_000),
                capabilities: vec![ModelCapability::Streaming, ModelCapability::Reasoning],
                ..DiscoveredModel::named("x-preview-l")
            },
            DiscoveredModel {
                display_name: "GLM-5.2".to_owned(),
                family: Some("glm".to_owned()),
                max_output: Some(64_064),
                capabilities: vec![ModelCapability::Streaming, ModelCapability::Reasoning],
                ..DiscoveredModel::named("glm-5.2")
            },
        ]
    );
    let call = server.last();
    assert_eq!(call.header("x-fe-version").as_deref(), Some(ZAI_FE_VERSION));
    assert_eq!(call.header("x-region").as_deref(), Some("overseas"));
    const { assert!(ZAI_WEB_CAPABILITIES.live_model_list) };
}

#[tokio::test]
async fn a_first_turn_opens_a_chat_and_sends_a_signed_completion() {
    let server = zai_server(fixture_frames("zaiStream.sse")).await;
    let deltas = adapter(&server)
        .stream(&prompt(), &token())
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .expect("streams");
    assert_eq!(
        deltas,
        [TextDelta::text("Привет"), TextDelta::text(", мир!")]
    );
    let create = server
        .calls()
        .into_iter()
        .find(|call| call.path == route(ZAI_CHATS_NEW_PATH))
        .expect("a chat is created");
    let history = &create.json()["chat"]["history"];
    assert_eq!(history["currentId"], "id-1");
    assert_eq!(history["messages"]["id-1"]["content"], "Привет");

    let call = &completions(&server)[0];
    let body = call.json();
    assert_eq!(body["chat_id"], CHAT_ID);
    assert_eq!(body["model"], "glm-5.2");
    assert_eq!(
        body["messages"],
        json!([{ "role": "user", "content": "Привет" }])
    );
    assert_eq!(body["signature_prompt"], "Привет");
    assert_eq!(body["current_user_message_id"], "id-1");
    assert_eq!(body["current_user_message_parent_id"], Value::Null);
    assert_eq!(body["id"], "id-2");
    assert_eq!(body["features"]["enable_thinking"], true);
    assert!(body.get("captcha_verify_param").is_none());

    let query = query(call);
    let value = |key: &str| {
        query
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
    };
    assert_eq!(value("timestamp").as_deref(), Some("1792867620312"));
    assert_eq!(
        value("signature_timestamp").as_deref(),
        Some("1792867620312")
    );
    assert_eq!(value("requestId").as_deref(), Some("id-3"));
    assert_eq!(value("user_id").as_deref(), Some(USER_ID));
    assert_eq!(value("version").as_deref(), Some("0.0.1"));
    assert_eq!(value("platform").as_deref(), Some("web"));
    assert_eq!(value("token").as_deref(), Some(""));
    let expected = SignedRequest {
        request_id: "id-3",
        user_id: USER_ID,
        timestamp_ms: FIXED_NOW,
        prompt: "Привет",
    }
    .signature();
    assert_eq!(call.header("x-signature"), Some(expected));
    assert_eq!(call.header("x-fe-version").as_deref(), Some(ZAI_FE_VERSION));
    assert!(
        call.header("referer")
            .is_some_and(|referer| referer.ends_with(&format!("/c/{CHAT_ID}")))
    );
    assert_eq!(ZAI_GENERATION_OBSERVATION.observed_at, "2026-10-24");
}

#[tokio::test]
async fn generate_collects_the_answer_and_the_vendor_usage() {
    let server = zai_server(fixture_frames("zaiStream.sse")).await;
    let result = adapter(&server)
        .generate(&prompt(), &token())
        .await
        .expect("generates");
    assert_eq!(result.text, "Привет, мир!");
    assert_eq!(result.reasoning, None);
    assert_eq!(result.finish_reason, FinishReason::Stop);
    assert_eq!(
        result.usage,
        Some(TokenUsage {
            prompt_tokens: Some(621),
            completion_tokens: Some(3),
            total_tokens: Some(624),
        })
    );
}

#[tokio::test]
async fn a_follow_up_continues_the_chat_with_the_whole_dialog_and_the_previous_answer_as_parent() {
    let server = zai_server(fixture_frames("zaiStream.sse")).await;
    let zai = adapter(&server);
    zai.generate(&prompt(), &token()).await.expect("generates");
    let continued = GenerateRequest {
        system: Some("Отвечай кратко".to_owned()),
        ..follow_up()
    };
    zai.generate(&continued, &token()).await.expect("opens");
    zai.generate(
        &GenerateRequest {
            system: Some("Отвечай кратко".to_owned()),
            messages: [
                continued.messages.clone(),
                vec![
                    ChatMessage::assistant("Хорошо"),
                    ChatMessage::user("Спасибо"),
                ],
            ]
            .concat(),
            ..prompt()
        },
        &token(),
    )
    .await
    .expect("continues");
    assert_eq!(count(&server, ZAI_CHATS_NEW_PATH), 2);
    assert_eq!(count(&server, ZAI_USER_PATH), 1);
    let bodies: Vec<Value> = completions(&server).iter().map(Recorded::json).collect();
    assert_eq!(bodies[1]["current_user_message_parent_id"], Value::Null);
    assert_eq!(bodies[1]["messages"][0]["role"], "system");
    let third = &bodies[2];
    assert_eq!(third["chat_id"], bodies[1]["chat_id"]);
    assert_eq!(third["current_user_message_parent_id"], bodies[1]["id"]);
    assert_eq!(third["messages"].as_array().map(Vec::len), Some(6));
    assert_eq!(
        third["messages"][4],
        json!({ "role": "assistant", "content": "Хорошо" })
    );
    assert_eq!(third["signature_prompt"], "Спасибо");
    assert_eq!(zai.chained_chats(), 3);
}

#[tokio::test]
async fn a_captcha_refusal_asks_for_browser_verification_and_is_never_retried() {
    let server = zai_server(fixture_frames("zaiStream.sse")).await;
    let zai = adapter(&server);
    zai.generate(&prompt(), &token()).await.expect("generates");
    server.reply(
        &route(ZAI_COMPLETIONS_PATH),
        Reply::chunks(fixture_frames("zaiStreamCaptcha.sse")),
    );
    let error = zai
        .generate(&follow_up(), &token())
        .await
        .expect_err("needs a verification");
    assert_eq!(error.code, AppErrorCode::ProviderUnreachable);
    assert_eq!(error.message, BROWSER_VERIFICATION_MESSAGE);
    assert_eq!(
        error
            .details
            .as_ref()
            .and_then(|details| details.get("vendorCode")),
        Some(&json!("FRONTEND_CAPTCHA_REQUIRED"))
    );
    assert_eq!(count(&server, ZAI_CHATS_NEW_PATH), 1);
    assert_eq!(count(&server, ZAI_COMPLETIONS_PATH), 2);
    assert_eq!(zai.chained_chats(), 1);
}

#[tokio::test]
async fn a_refused_chained_turn_is_retried_once_in_a_new_chat() {
    let server = zai_server(fixture_frames("zaiStream.sse")).await;
    let zai = adapter(&server);
    zai.generate(&prompt(), &token()).await.expect("generates");
    server.reply(
        &route(ZAI_COMPLETIONS_PATH),
        Reply::json(json!({ "detail": "chat not found" })).with_status(404),
    );
    zai.generate(&follow_up(), &token())
        .await
        .expect_err("the fresh chat fails too");
    assert_eq!(count(&server, ZAI_CHATS_NEW_PATH), 2);
    assert_eq!(count(&server, ZAI_COMPLETIONS_PATH), 3);
    assert_eq!(zai.chained_chats(), 0);
    let retried = completions(&server)[2].json();
    assert_eq!(retried["current_user_message_parent_id"], Value::Null);
    assert_eq!(retried["messages"].as_array().map(Vec::len), Some(3));
}

#[tokio::test]
async fn an_expired_session_forgets_every_chain_and_the_account_id() {
    let server = zai_server(fixture_frames("zaiStream.sse")).await;
    let zai = adapter(&server);
    zai.generate(&prompt(), &token()).await.expect("generates");
    server.reply(
        &route(ZAI_COMPLETIONS_PATH),
        Reply::chunks(fixture_frames("zaiStreamExpired.sse")),
    );
    let error = zai
        .generate(&follow_up(), &token())
        .await
        .expect_err("expires");
    assert_eq!(error.code, AppErrorCode::ProviderSessionExpired);
    assert_eq!(zai.chained_chats(), 0);
    server.reply(
        &route(ZAI_COMPLETIONS_PATH),
        Reply::chunks(fixture_frames("zaiStream.sse")),
    );
    zai.generate(&prompt(), &token()).await.expect("generates");
    assert_eq!(count(&server, ZAI_USER_PATH), 2);
}

#[tokio::test]
async fn texts_reach_the_stream_as_answer_deltas() {
    let server = zai_server(fixture_frames("zaiStream.sse")).await;
    let deltas = adapter(&server)
        .stream(&prompt(), &token())
        .collect::<Vec<_>>()
        .await;
    assert!(deltas.iter().all(|delta| matches!(
        delta,
        Ok(TextDelta::Content {
            kind: DeltaKind::Text,
            ..
        })
    )));
}
