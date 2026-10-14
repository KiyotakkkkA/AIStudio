use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use zvs_contract::{AppErrorCode, RunOutcomeStatus};

use super::adapters::{OllamaAdapter, OpenAiCompatibleAdapter};
use super::http::{HttpClient, HttpClientOptions, ProxyMode};
use super::ports::{ChatMessage, GenerateRequest, TextGenerationDriver};
use super::stream_to_events::stream_to_events;
use super::transport::{ApiKey, ApiTransport, ApiTransportOptions, SecretResolver};
use crate::platform::events::{EventBus, EventBusOptions};
use crate::platform::logger::silent_logger;
use crate::services::crypto::Plaintext;
use crate::testing::MemorySink;

struct EnvKey(String);

impl SecretResolver for EnvKey {
    fn resolve(&self, _id: &str) -> Result<Plaintext, zvs_contract::AppError> {
        Ok(Plaintext::new(self.0.clone()))
    }
}

fn variable(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn openai(base_url: &str, key: &str) -> OpenAiCompatibleAdapter {
    let mut options = ApiTransportOptions::new(base_url, HttpClient::new().expect("a client"));
    options.api_key = Some(ApiKey::new("live", Arc::new(EnvKey(key.to_owned()))));
    options.timeout_seconds = Some(60.0);
    OpenAiCompatibleAdapter::new(Arc::new(ApiTransport::new(options)), silent_logger())
}

fn attached_bus() -> (EventBus, Arc<MemorySink>) {
    let bus = EventBus::new(EventBusOptions::default());
    let sink = MemorySink::shared();
    bus.attach(sink.clone());
    (bus, sink)
}

#[tokio::test]
#[ignore = "talks to a live vendor; set LIVE_BASE_URL and LIVE_API_KEY"]
async fn live_list_models_streaming_abort_and_a_wrong_key() {
    let (Some(base_url), Some(api_key)) = (variable("LIVE_BASE_URL"), variable("LIVE_API_KEY"))
    else {
        return;
    };
    let driver = openai(&base_url, &api_key);
    let models = driver
        .list_models(&CancellationToken::new())
        .await
        .expect("lists");
    println!("listModels -> {}", models.len());
    for model in models.iter().take(8) {
        println!(
            "  {} ctx={:?} out={:?}",
            model.external_id, model.context_window, model.max_output
        );
    }
    let model = variable("LIVE_MODEL")
        .or_else(|| models.first().map(|model| model.external_id.clone()))
        .expect("a model to stream with");
    println!("streaming with {model}");

    let (bus, sink) = attached_bus();
    let full = stream_to_events(
        &bus.open_stream().expect("a stream"),
        driver.stream(
            &GenerateRequest {
                temperature: Some(0.3),
                top_k: Some(40),
                max_output_tokens: Some(512),
                ..GenerateRequest::new(
                    model.clone(),
                    vec![ChatMessage::user("Say one short sentence about the sea.")],
                )
            },
            &CancellationToken::new(),
        ),
    )
    .await;
    println!(
        "stream outcome {:?} tokens={} text: {}",
        full.outcome,
        sink.events().len().saturating_sub(1),
        full.text
    );
    assert_eq!(full.outcome.status, RunOutcomeStatus::Ok);
    assert!(!full.text.is_empty());

    let cancel = CancellationToken::new();
    let stopper = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1200)).await;
        stopper.cancel();
    });
    let aborted = stream_to_events(
        &bus.open_stream().expect("a stream"),
        driver.stream(
            &GenerateRequest::new(
                model.clone(),
                vec![ChatMessage::user("Count slowly from 1 to 200.")],
            ),
            &cancel,
        ),
    )
    .await;
    println!(
        "abort outcome {:?} partial={}",
        aborted.outcome,
        aborted.text.len()
    );

    let failure = openai(&base_url, "sk-definitely-not-a-real-key")
        .generate(
            &GenerateRequest::new(model, vec![ChatMessage::user("hi")]),
            &CancellationToken::new(),
        )
        .await
        .expect_err("a wrong key fails");
    println!("bad key -> {}: {}", failure.code, failure.message);
    assert_eq!(failure.code, AppErrorCode::ProviderAuthFailed);
}

#[tokio::test]
#[ignore = "needs a running Ollama; set ZVS_LIVE_OLLAMA=http://127.0.0.1:11434"]
async fn live_ollama_streams_a_completion() {
    let Some(origin) = variable("ZVS_LIVE_OLLAMA") else {
        return;
    };
    let http = HttpClient::with_options(HttpClientOptions {
        proxy: ProxyMode::Off,
        ..HttpClientOptions::default()
    })
    .expect("a client");
    let mut options =
        ApiTransportOptions::new(format!("{}/api", origin.trim_end_matches('/')), http);
    options.timeout_seconds = Some(300.0);
    let driver = OllamaAdapter::new(Arc::new(ApiTransport::new(options)), silent_logger());
    let models = driver
        .list_models(&CancellationToken::new())
        .await
        .expect("lists the local models");
    println!(
        "tags -> {:?}",
        models
            .iter()
            .map(|model| model.external_id.as_str())
            .collect::<Vec<_>>()
    );
    let model = variable("ZVS_LIVE_OLLAMA_MODEL")
        .or_else(|| models.first().map(|model| model.external_id.clone()))
        .expect("at least one pulled model");
    let (bus, sink) = attached_bus();
    let result = stream_to_events(
        &bus.open_stream().expect("a stream"),
        driver.stream(
            &GenerateRequest {
                max_output_tokens: Some(64),
                ..GenerateRequest::new(
                    model.clone(),
                    vec![ChatMessage::user("Say one short sentence about the sea.")],
                )
            },
            &CancellationToken::new(),
        ),
    )
    .await;
    println!(
        "{model}: outcome {:?}, {} events, text: {}",
        result.outcome,
        sink.events().len(),
        result.text
    );
    assert_eq!(result.outcome.status, RunOutcomeStatus::Ok);
    assert!(!result.text.is_empty() || !result.reasoning.is_empty());
}
