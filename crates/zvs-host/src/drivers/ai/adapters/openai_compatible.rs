use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use url::Url;
use zvs_contract::{AdapterFamily, AppError, AuthMode, ModelCapability, ToolMode};

use super::wire::{
    decode, field, first_number, first_string, is_assistant_call, normalise, object, open_frames,
    plain_message, present, stringify, wire_tools,
};
use crate::drivers::ai::capabilities::{AdapterCapabilities, Honours};
use crate::drivers::ai::errors::{
    cancelled, is_cancellation, is_not_found_response, raw_error_text,
};
use crate::drivers::ai::ports::{
    ChatMessage, ChatRole, DeltaStream, DiscoveredModel, EmbeddingDriver, FinishReason,
    GenerateRequest, GenerateResult, TextDelta, TextGenerationDriver, TokenUsage, ToolCall,
};
use crate::drivers::ai::sse::{FrameStream, sse_data};
use crate::drivers::ai::transport::{RequestSpec, SharedTransport, TransportResponse};
use crate::platform::logger::{LogLevel, SharedLogger};

pub const OPENAI_COMPATIBLE_CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    family: AdapterFamily::OpenaiCompatible,
    auth_modes: &[AuthMode::Api],
    streaming: true,
    live_model_list: true,
    embedding: true,
    image: false,
    honours: Honours {
        temperature: true,
        top_k: false,
        top_p: true,
        max_output_tokens: true,
    },
    tools: ToolMode::Native,
};

pub const MODEL_LIST_PATHS: [&str; 2] = ["/models", "/tags"];
pub const OPENROUTER_CATALOG_PATH: &str = "../frontend/v1/catalog/models";
const OPENROUTER_HOSTS: [&str; 2] = ["openrouter.ai", "eu.openrouter.ai"];

pub struct OpenAiCompatibleAdapter {
    transport: SharedTransport,
    logger: SharedLogger,
    dimensions: Mutex<Option<usize>>,
}

impl OpenAiCompatibleAdapter {
    pub fn new(transport: SharedTransport, logger: SharedLogger) -> Self {
        Self {
            transport,
            logger,
            dimensions: Mutex::new(None),
        }
    }

    fn body(&self, request: &GenerateRequest, stream: bool) -> Value {
        self.drop_unsupported(request);
        let messages = request
            .system
            .iter()
            .map(|system| ChatMessage::system(system.clone()))
            .chain(request.messages.iter().cloned())
            .map(|message| wire_message(&message))
            .collect::<Vec<_>>();
        let mut body = object([
            ("model", Value::from(request.model.clone())),
            ("messages", Value::Array(messages)),
            ("stream", Value::Bool(stream)),
        ]);
        let fields = body.as_object_mut().expect("an object");
        if !request.tools.is_empty() {
            fields.insert("tools".to_owned(), wire_tools(&request.tools));
        }
        if let Some(temperature) = request.temperature {
            fields.insert("temperature".to_owned(), json!(temperature));
        }
        if let Some(top_p) = request.top_p {
            fields.insert("top_p".to_owned(), json!(top_p));
        }
        if let Some(max_tokens) = request.max_output_tokens {
            fields.insert("max_tokens".to_owned(), json!(max_tokens));
        }
        body
    }

    fn drop_unsupported(&self, request: &GenerateRequest) {
        let dropped =
            OPENAI_COMPATIBLE_CAPABILITIES.unsupported_parameters(&request.requested_parameters());
        if dropped.is_empty() {
            return;
        }
        self.logger.log(
            LogLevel::Debug,
            "ai",
            "Dropped parameters the adapter family does not honour",
            &[
                ("family", json!(OPENAI_COMPATIBLE_CAPABILITIES.family)),
                ("dropped", json!(dropped)),
            ],
        );
    }

    async fn request(
        &self,
        spec: &RequestSpec,
        cancel: &CancellationToken,
    ) -> Result<TransportResponse, AppError> {
        self.transport
            .request(spec, cancel)
            .await
            .map_err(normalise)
    }

    async fn model_list(&self, cancel: &CancellationToken) -> Result<Value, AppError> {
        let last = MODEL_LIST_PATHS.len() - 1;
        for (index, path) in MODEL_LIST_PATHS.iter().enumerate() {
            match self.request(&RequestSpec::get(*path), cancel).await {
                Ok(response) => return decode(&response.text),
                Err(error) if index == last || !is_not_found_response(&error) => {
                    return Err(error);
                }
                Err(_) => self.logger.log(
                    LogLevel::Debug,
                    "ai",
                    "Model list endpoint is absent, trying the next one",
                    &[
                        ("baseUrl", json!(self.transport.base_url())),
                        ("path", json!(path)),
                    ],
                ),
            }
        }
        Ok(json!({}))
    }

    async fn open_router_metadata(
        &self,
        cancel: &CancellationToken,
    ) -> Result<HashMap<String, Value>, AppError> {
        let host = Url::parse(self.transport.base_url())
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned));
        if !host.is_some_and(|host| OPENROUTER_HOSTS.contains(&host.as_str())) {
            return Ok(HashMap::new());
        }
        let fetched = match self
            .request(&RequestSpec::get(OPENROUTER_CATALOG_PATH), cancel)
            .await
        {
            Ok(response) => decode(&response.text),
            Err(error) => Err(error),
        };
        match fetched {
            Ok(payload) => Ok(entries(Some(&payload), "data")
                .iter()
                .filter_map(|entry| {
                    let endpoint = entry.get("endpoint");
                    first_string(&[field(endpoint, "model_variant_slug"), entry.get("slug")])
                        .map(|id| (id, entry.clone()))
                })
                .collect()),
            Err(error) if cancel.is_cancelled() || is_cancellation(&error) => Err(cancelled()),
            Err(error) => {
                self.logger.log(
                    LogLevel::Warn,
                    "ai",
                    "OpenRouter filter metadata is unavailable",
                    &[("raw", json!(raw_error_text(&error)))],
                );
                Ok(HashMap::new())
            }
        }
    }
}

fn entries<'a>(payload: Option<&'a Value>, key: &str) -> &'a [Value] {
    present(field(payload, key))
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn wire_message(message: &ChatMessage) -> Value {
    if message.role == ChatRole::Tool {
        return object([
            ("role", Value::from("tool")),
            (
                "tool_call_id",
                Value::from(message.tool_call_id.clone().unwrap_or_default()),
            ),
            ("content", Value::from(message.content.clone())),
        ]);
    }
    if is_assistant_call(message) {
        return object([
            ("role", Value::from("assistant")),
            (
                "content",
                if message.content.is_empty() {
                    Value::Null
                } else {
                    Value::from(message.content.clone())
                },
            ),
            (
                "tool_calls",
                Value::Array(
                    message
                        .tool_calls
                        .iter()
                        .map(|call| {
                            object([
                                ("id", Value::from(call.id.clone())),
                                ("type", Value::from("function")),
                                (
                                    "function",
                                    object([
                                        ("name", Value::from(call.name.clone())),
                                        ("arguments", Value::from(call.arguments.clone())),
                                    ]),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]);
    }
    plain_message(message)
}

#[derive(Debug, Default)]
struct ToolCallPart {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Debug, Default)]
pub struct ToolCallAssembler {
    parts: BTreeMap<i64, ToolCallPart>,
}

impl ToolCallAssembler {
    pub fn push(&mut self, raw: Option<&Value>) {
        let Some(Value::Array(entries)) = raw else {
            return;
        };
        for (position, entry) in entries.iter().enumerate() {
            if !entry.is_object() {
                continue;
            }
            let index = entry
                .get("index")
                .and_then(Value::as_i64)
                .unwrap_or_else(|| i64::try_from(position).unwrap_or(i64::MAX));
            let part = self.parts.entry(index).or_default();
            if let Some(id) = entry
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
            {
                id.clone_into(&mut part.id);
            }
            let function = entry.get("function");
            if let Some(name) = field(function, "name").and_then(Value::as_str) {
                part.name.push_str(name);
            }
            match present(field(function, "arguments")) {
                Some(Value::String(arguments)) => part.arguments.push_str(arguments),
                Some(arguments) => part.arguments.push_str(&stringify(arguments)),
                None => {}
            }
        }
    }

    pub fn finish(&self) -> Vec<ToolCall> {
        self.parts
            .iter()
            .filter(|(_, part)| !part.name.is_empty())
            .map(|(index, part)| ToolCall {
                id: if part.id.is_empty() {
                    format!("call_{index}")
                } else {
                    part.id.clone()
                },
                name: part.name.clone(),
                arguments: if part.arguments.trim().is_empty() {
                    "{}".to_owned()
                } else {
                    part.arguments.clone()
                },
            })
            .collect()
    }
}

fn finish_reason(raw: Option<&Value>) -> FinishReason {
    match raw.and_then(Value::as_str) {
        Some("stop" | "end_turn") => FinishReason::Stop,
        Some("length" | "max_tokens") => FinishReason::Length,
        Some("tool_calls" | "function_call") => FinishReason::ToolCalls,
        Some("cancelled" | "aborted") => FinishReason::Cancelled,
        _ => FinishReason::Unknown,
    }
}

fn reasoning_of(content: Option<&Value>) -> Option<String> {
    present(field(content, "reasoning_content"))
        .or_else(|| field(content, "reasoning"))
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn usage(raw: Option<&Value>) -> Option<TokenUsage> {
    let raw = present(raw)?;
    Some(TokenUsage {
        prompt_tokens: first_number(&[raw.get("prompt_tokens"), raw.get("input_tokens")]),
        completion_tokens: first_number(&[raw.get("completion_tokens"), raw.get("output_tokens")]),
        total_tokens: first_number(&[raw.get("total_tokens")]),
    })
}

fn to_vector(raw: Option<&Value>) -> Vec<f32> {
    raw.and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_f64)
                .map(|value| value as f32)
                .collect()
        })
        .unwrap_or_default()
}

fn family_of(external_id: &str) -> Option<String> {
    let family = external_id.split(['/', ':']).next()?;
    (family != external_id).then(|| family.to_owned())
}

fn model_capabilities(entry: &Value) -> Vec<ModelCapability> {
    let mut capabilities = vec![ModelCapability::Streaming];
    let parameters = entry
        .get("supported_parameters")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    if parameters.contains(&"tools") || parameters.contains(&"tool_choice") {
        capabilities.push(ModelCapability::Tools);
    }
    if parameters.contains(&"reasoning") || parameters.contains(&"include_reasoning") {
        capabilities.push(ModelCapability::Reasoning);
    }
    let architecture = entry.get("architecture");
    let modalities = field(architecture, "input_modalities").and_then(Value::as_array);
    let modality = field(architecture, "modality").and_then(Value::as_str);
    if modalities.is_some_and(|values| values.iter().any(|value| value == "image"))
        || modality.is_some_and(|value| value.contains("image"))
    {
        capabilities.push(ModelCapability::Vision);
    }
    capabilities
}

fn to_discovered_model(external_id: String, entry: &Value) -> DiscoveredModel {
    let endpoint = entry.get("endpoint");
    let policy = field(endpoint, "data_policy");
    let no_training = policy.map(|policy| {
        let retention =
            present(policy.get("retainsPrompt")).or_else(|| policy.get("retainsPrompts"));
        retention == Some(&Value::Bool(false))
            && policy.get("training") == Some(&Value::Bool(false))
            && policy.get("trainingOpenRouter") == Some(&Value::Bool(false))
    });
    let details = entry.get("details");
    let top_provider = entry.get("top_provider");
    DiscoveredModel {
        is_free: field(endpoint, "is_free").and_then(Value::as_bool),
        no_training,
        display_name: first_string(&[entry.get("display_name"), entry.get("name")])
            .unwrap_or_else(|| external_id.clone()),
        family: first_string(&[entry.get("owned_by"), field(details, "family")])
            .or_else(|| family_of(&external_id)),
        context_window: first_number(&[
            entry.get("context_length"),
            entry.get("context_window"),
            field(top_provider, "context_length"),
        ]),
        max_output: first_number(&[
            entry.get("max_output_tokens"),
            field(top_provider, "max_completion_tokens"),
        ]),
        size_bytes: first_number(&[entry.get("size")]),
        capabilities: model_capabilities(entry),
        external_id,
    }
}

struct StreamState {
    frames: FrameStream,
    calls: ToolCallAssembler,
    queue: VecDeque<TextDelta>,
    finished: bool,
}

fn chat_deltas(frames: FrameStream) -> DeltaStream {
    let state = StreamState {
        frames,
        calls: ToolCallAssembler::default(),
        queue: VecDeque::new(),
        finished: false,
    };
    futures::stream::unfold(state, |mut state| async move {
        loop {
            if let Some(delta) = state.queue.pop_front() {
                return Some((Ok(delta), state));
            }
            if state.finished {
                return None;
            }
            match state.frames.next().await {
                Some(Ok(payload)) => {
                    let parsed = serde_json::from_str::<Value>(&payload).ok();
                    let delta = parsed
                        .as_ref()
                        .and_then(|parsed| parsed.get("choices"))
                        .and_then(|choices| choices.get(0))
                        .and_then(|choice| choice.get("delta"));
                    if let Some(reasoning) = reasoning_of(delta) {
                        state.queue.push_back(TextDelta::reasoning(reasoning));
                    }
                    if let Some(content) = field(delta, "content")
                        .and_then(Value::as_str)
                        .filter(|content| !content.is_empty())
                    {
                        state.queue.push_back(TextDelta::text(content));
                    }
                    state.calls.push(field(delta, "tool_calls"));
                }
                Some(Err(error)) => {
                    state.finished = true;
                    state.queue.clear();
                    return Some((Err(normalise(error)), state));
                }
                None => {
                    state.finished = true;
                    let calls = state.calls.finish();
                    state
                        .queue
                        .extend(calls.into_iter().map(TextDelta::ToolCall));
                }
            }
        }
    })
    .boxed()
}

#[async_trait]
impl TextGenerationDriver for OpenAiCompatibleAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        OPENAI_COMPATIBLE_CAPABILITIES
    }

    async fn generate(
        &self,
        request: &GenerateRequest,
        cancel: &CancellationToken,
    ) -> Result<GenerateResult, AppError> {
        let spec = RequestSpec::post("/chat/completions", self.body(request, false));
        let response = self.request(&spec, cancel).await?;
        let payload = decode(&response.text)?;
        let choice = payload.get("choices").and_then(|choices| choices.get(0));
        let message = field(choice, "message");
        let mut calls = ToolCallAssembler::default();
        calls.push(field(message, "tool_calls"));
        Ok(GenerateResult {
            model: payload
                .get("model")
                .and_then(Value::as_str)
                .map_or_else(|| request.model.clone(), str::to_owned),
            text: field(message, "content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            reasoning: reasoning_of(message),
            finish_reason: finish_reason(field(choice, "finish_reason")),
            usage: usage(payload.get("usage")),
            tool_calls: calls.finish(),
        })
    }

    fn stream(&self, request: &GenerateRequest, cancel: &CancellationToken) -> DeltaStream {
        let spec = RequestSpec::post("/chat/completions", self.body(request, true));
        chat_deltas(open_frames(
            self.transport.clone(),
            spec,
            cancel.clone(),
            sse_data,
        ))
    }

    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        let payload = self.model_list(cancel).await?;
        let listed = match present(payload.get("data")) {
            Some(_) => entries(Some(&payload), "data"),
            None => entries(Some(&payload), "models"),
        };
        let metadata = self.open_router_metadata(cancel).await?;
        let mut seen = HashSet::new();
        let mut discovered = Vec::new();
        for entry in listed {
            let external_id = first_string(&[
                field(entry.get("endpoint"), "model_variant_slug"),
                entry.get("id"),
                entry.get("slug"),
                entry.get("model"),
                entry.get("name"),
            ])
            .unwrap_or_default();
            if external_id.is_empty() || !seen.insert(external_id.clone()) {
                continue;
            }
            let mut merged = entry.clone();
            if let (Some(endpoint), Some(fields)) = (
                metadata
                    .get(&external_id)
                    .and_then(|known| known.get("endpoint")),
                merged.as_object_mut(),
            ) {
                fields.insert("endpoint".to_owned(), endpoint.clone());
            }
            discovered.push(to_discovered_model(external_id, &merged));
        }
        Ok(discovered)
    }
}

#[async_trait]
impl EmbeddingDriver for OpenAiCompatibleAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        OPENAI_COMPATIBLE_CAPABILITIES
    }

    async fn embed(
        &self,
        texts: &[String],
        model: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let spec = RequestSpec::post(
            "/embeddings",
            object([("model", json!(model)), ("input", json!(texts))]),
        );
        let response = self.request(&spec, cancel).await?;
        let payload = decode(&response.text)?;
        let vectors = entries(Some(&payload), "data")
            .iter()
            .map(|entry| to_vector(entry.get("embedding")))
            .collect::<Vec<_>>();
        if let Some(first) = vectors.first() {
            *self
                .dimensions
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(first.len());
        }
        Ok(vectors)
    }

    fn dimensions(&self) -> Option<usize> {
        *self
            .dimensions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        TextGenerationDriver::list_models(self, cancel).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::ai::mock::{
        MockServer, Reply, fixture_frames, openrouter_transport, search_tool, sse_frames,
        tool_request, transport_for,
    };
    use crate::platform::logger::silent_logger;
    use crate::testing::MemoryLogger;
    use zvs_contract::{AppErrorCode, TunableParameter};

    fn request() -> GenerateRequest {
        GenerateRequest {
            temperature: Some(0.4),
            top_p: Some(0.9),
            top_k: Some(40),
            max_output_tokens: Some(128),
            ..GenerateRequest::new("llama3", vec![ChatMessage::user("Привет")])
        }
    }

    fn adapter(server: &MockServer) -> OpenAiCompatibleAdapter {
        OpenAiCompatibleAdapter::new(transport_for(&server.url("/v1")), silent_logger())
    }

    async fn drain(stream: DeltaStream) -> Result<Vec<TextDelta>, AppError> {
        stream.collect::<Vec<_>>().await.into_iter().collect()
    }

    fn texts(deltas: &[TextDelta]) -> Vec<String> {
        deltas
            .iter()
            .filter_map(|delta| match delta {
                TextDelta::Content { text, .. } => Some(text.clone()),
                TextDelta::ToolCall(_) | TextDelta::WebSources(_) => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn the_adapter_maps_a_request_onto_chat_completions_and_drops_top_k() {
        let server = MockServer::start().await;
        server.reply(
            "/v1/chat/completions",
            Reply::json(json!({
                "model": "llama3",
                "choices": [{ "message": { "content": "Здравствуйте" }, "finish_reason": "stop" }],
                "usage": { "prompt_tokens": 7, "completion_tokens": 3, "total_tokens": 10 },
            })),
        );
        let logger = MemoryLogger::shared();
        let adapter =
            OpenAiCompatibleAdapter::new(transport_for(&server.url("/v1")), logger.clone());
        let result = adapter
            .generate(
                &GenerateRequest {
                    system: Some("Будь краток".to_owned()),
                    ..request()
                },
                &CancellationToken::new(),
            )
            .await
            .expect("generates");
        assert_eq!(result.text, "Здравствуйте");
        assert_eq!(result.finish_reason, FinishReason::Stop);
        assert_eq!(
            result.usage,
            Some(TokenUsage {
                prompt_tokens: Some(7),
                completion_tokens: Some(3),
                total_tokens: Some(10),
            })
        );
        let body = server.last_body();
        assert_eq!(body["temperature"], json!(0.4));
        assert_eq!(body["top_p"], json!(0.9));
        assert_eq!(body["max_tokens"], json!(128));
        assert_eq!(body["stream"], json!(false));
        assert!(body.get("top_k").is_none());
        assert!(body.get("topK").is_none());
        assert_eq!(
            body["messages"],
            json!([
                { "role": "system", "content": "Будь краток" },
                { "role": "user", "content": "Привет" },
            ])
        );
        let dropped = logger.matching("Dropped parameters");
        assert_eq!(dropped.len(), 1);
        assert_eq!(
            dropped[0].fields["dropped"],
            json!([TunableParameter::TopK])
        );
        assert_eq!(dropped[0].fields["family"], json!("openai-compatible"));
    }

    #[tokio::test]
    async fn sse_frames_become_deltas_and_discovery_maps_onto_model_rows() {
        let server = MockServer::start().await;
        server.reply(
            "/v1/chat/completions",
            Reply::chunks(sse_frames(&["При", "вет", "!"]))
                .with_gap(std::time::Duration::from_millis(5)),
        );
        server.reply(
            "/v1/models",
            Reply::json(json!({
                "data": [
                    {
                        "id": "openai/gpt-4o",
                        "name": "GPT-4o",
                        "context_length": 128_000,
                        "owned_by": "openai",
                        "supported_parameters": ["tools", "reasoning"],
                        "architecture": { "input_modalities": ["text", "image"] },
                        "top_provider": { "max_completion_tokens": 16384 },
                    },
                    { "id": "llama3:8b" },
                    { "id": "llama3:8b" },
                    { "id": "  " },
                ],
            })),
        );
        let adapter = adapter(&server);
        let deltas = drain(adapter.stream(&request(), &CancellationToken::new()))
            .await
            .expect("streams");
        assert_eq!(texts(&deltas), ["При", "вет", "!"]);
        assert_eq!(server.last_body()["stream"], json!(true));
        assert_eq!(
            server.last().header("accept").as_deref(),
            Some("text/event-stream")
        );

        let models = TextGenerationDriver::list_models(&adapter, &CancellationToken::new())
            .await
            .expect("lists");
        assert_eq!(models.len(), 2);
        assert_eq!(
            models[0],
            DiscoveredModel {
                is_free: None,
                no_training: None,
                external_id: "openai/gpt-4o".to_owned(),
                display_name: "GPT-4o".to_owned(),
                family: Some("openai".to_owned()),
                context_window: Some(128_000),
                max_output: Some(16384),
                size_bytes: None,
                capabilities: vec![
                    ModelCapability::Streaming,
                    ModelCapability::Tools,
                    ModelCapability::Reasoning,
                    ModelCapability::Vision,
                ],
            }
        );
        assert_eq!(models[1].capabilities, [ModelCapability::Streaming]);
        assert_eq!(models[1].family.as_deref(), Some("llama3"));
    }

    #[tokio::test]
    async fn the_openrouter_catalog_joins_on_the_variant_slug_and_requires_explicit_privacy_flags()
    {
        let server = MockServer::start().await;
        let policies = [
            json!({ "retainsPrompts": false, "training": false, "trainingOpenRouter": false }),
            json!({ "retainsPrompt": false, "training": false, "trainingOpenRouter": false }),
            json!({ "retainsPrompts": true, "training": false, "trainingOpenRouter": false }),
            json!({ "retainsPrompts": false, "training": true, "trainingOpenRouter": false }),
            json!({ "retainsPrompts": false, "training": false, "trainingOpenRouter": true }),
            json!({ "retainsPrompts": false, "training": false }),
        ];
        let mut listed = (0..policies.len())
            .map(|index| json!({ "id": format!("author/model-{index}:free") }))
            .collect::<Vec<_>>();
        listed.push(json!({ "id": "author/unknown:free" }));
        listed.push(json!({ "id": "author/model-0" }));
        server.reply("/api/v1/models", Reply::json(json!({ "data": listed })));
        server.reply(
            "/api/frontend/v1/catalog/models",
            Reply::json(json!({
                "data": policies.iter().enumerate().map(|(index, policy)| json!({
                    "slug": format!("author/model-{index}"),
                    "endpoint": {
                        "model_variant_slug": format!("author/model-{index}:free"),
                        "is_free": index != 1,
                        "data_policy": policy,
                    },
                })).collect::<Vec<_>>(),
            })),
        );
        let adapter = OpenAiCompatibleAdapter::new(openrouter_transport(&server), silent_logger());
        let models = TextGenerationDriver::list_models(&adapter, &CancellationToken::new())
            .await
            .expect("lists");
        assert_eq!(
            models
                .iter()
                .map(|model| model.no_training)
                .collect::<Vec<_>>(),
            [
                Some(true),
                Some(true),
                Some(false),
                Some(false),
                Some(false),
                Some(false),
                None,
                None
            ]
        );
        assert_eq!(
            models.iter().map(|model| model.is_free).collect::<Vec<_>>(),
            [
                Some(true),
                Some(false),
                Some(true),
                Some(true),
                Some(true),
                Some(true),
                None,
                None
            ]
        );
        assert_eq!(server.last().host.as_deref(), Some("openrouter.ai"));
    }

    #[tokio::test]
    async fn openrouter_metadata_failures_preserve_discovery_without_assuming_free_or_private() {
        let server = MockServer::start().await;
        server.reply(
            "/api/v1/models",
            Reply::json(json!({ "data": [{ "id": "author/model:free" }] })),
        );
        server.reply(
            "/api/frontend/v1/catalog/models",
            Reply::json(json!({})).with_status(503),
        );
        let logger = MemoryLogger::shared();
        let adapter = OpenAiCompatibleAdapter::new(openrouter_transport(&server), logger.clone());
        let models = TextGenerationDriver::list_models(&adapter, &CancellationToken::new())
            .await
            .expect("lists");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].is_free, None);
        assert_eq!(models[0].no_training, None);
        assert_eq!(logger.matching("OpenRouter filter metadata").len(), 1);
    }

    #[tokio::test]
    async fn a_base_url_that_speaks_the_openai_dialect_is_listed_in_one_request() {
        let server = MockServer::start().await;
        server.reply(
            "/v1/models",
            Reply::json(json!({ "data": [{ "id": "qwen/qwen3-coder" }] })),
        );
        let models =
            TextGenerationDriver::list_models(&adapter(&server), &CancellationToken::new())
                .await
                .expect("lists");
        assert_eq!(
            models
                .iter()
                .map(|model| model.external_id.as_str())
                .collect::<Vec<_>>(),
            ["qwen/qwen3-coder"]
        );
        assert_eq!(server.paths(), ["/v1/models"]);
    }

    #[tokio::test]
    async fn ollama_tags_answer_where_models_is_absent_and_carry_the_size() {
        let server = MockServer::start().await;
        server.reply(
            "/api/models",
            Reply::json(json!({ "error": "not found" })).with_status(404),
        );
        server.reply(
            "/api/tags",
            Reply::json(json!({
                "models": [
                    {
                        "name": "gpt-oss:120b",
                        "model": "gpt-oss:120b",
                        "size": 65_290_180_781_i64,
                        "details": { "family": "", "parameter_size": "" },
                    },
                    {
                        "name": "qwen3.5:397b",
                        "model": "qwen3.5:397b",
                        "size": 1000,
                        "details": { "family": "qwen3" },
                    },
                    { "name": "  ", "model": "  " },
                ],
            })),
        );
        let adapter =
            OpenAiCompatibleAdapter::new(transport_for(&server.url("/api")), silent_logger());
        let models = TextGenerationDriver::list_models(&adapter, &CancellationToken::new())
            .await
            .expect("lists");
        assert_eq!(server.paths(), ["/api/models", "/api/tags"]);
        assert_eq!(
            models[0],
            DiscoveredModel {
                family: Some("gpt-oss".to_owned()),
                size_bytes: Some(65_290_180_781),
                capabilities: vec![ModelCapability::Streaming],
                ..DiscoveredModel::named("gpt-oss:120b")
            }
        );
        assert_eq!(models[1].family.as_deref(), Some("qwen3"));
        assert_eq!(models.len(), 2);
    }

    #[tokio::test]
    async fn a_rejected_key_is_reported_from_the_first_path_never_retried_against_the_second() {
        let server = MockServer::start().await;
        server.reply(
            "/v1/models",
            Reply::json(json!({ "detail": "Invalid API Key" })).with_status(401),
        );
        let error = TextGenerationDriver::list_models(&adapter(&server), &CancellationToken::new())
            .await
            .expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::ProviderAuthFailed);
        assert_eq!(server.paths(), ["/v1/models"]);
    }

    #[tokio::test]
    async fn a_vendor_with_neither_endpoint_reports_the_second_failure() {
        let server = MockServer::start().await;
        server.reply("/v1/models", Reply::json(json!({})).with_status(404));
        server.reply("/v1/tags", Reply::json(json!({})).with_status(404));
        let error = TextGenerationDriver::list_models(&adapter(&server), &CancellationToken::new())
            .await
            .expect_err("refuses");
        assert!(is_not_found_response(&error));
        assert_eq!(server.paths(), ["/v1/models", "/v1/tags"]);
    }

    #[tokio::test]
    async fn a_non_2xx_response_never_reaches_the_adapter_as_data() {
        let server = MockServer::start().await;
        server.reply(
            "/v1/models",
            Reply::json(json!({ "error": "bad key" })).with_status(401),
        );
        server.reply(
            "/v1/chat/completions",
            Reply::chunks(Vec::<String>::new())
                .with_status(429)
                .with_header("retry-after", "5"),
        );
        let adapter = adapter(&server);
        let error = TextGenerationDriver::list_models(&adapter, &CancellationToken::new())
            .await
            .expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::ProviderAuthFailed);
        assert_eq!(
            error
                .details
                .and_then(|details| details.get("vendorMessage").cloned()),
            Some(json!("bad key"))
        );
        let limited = drain(adapter.stream(&request(), &CancellationToken::new()))
            .await
            .expect_err("refuses");
        assert_eq!(limited.code, AppErrorCode::RateLimited);
    }

    #[tokio::test]
    async fn a_body_that_is_not_json_is_a_request_failure() {
        let server = MockServer::start().await;
        server.reply("/v1/chat/completions", Reply::text("<html>oops</html>"));
        let error = adapter(&server)
            .generate(&request(), &CancellationToken::new())
            .await
            .expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::Unknown);
        assert_eq!(error.message, "Не удалось выполнить запрос к провайдеру");
    }

    #[tokio::test]
    async fn cancelling_mid_stream_ends_with_run_cancelled_and_aborts_the_vendor_request() {
        let server = MockServer::start().await;
        server.reply(
            "/v1/chat/completions",
            Reply::chunks([r#"data: {"choices":[{"delta":{"content":"a"}}]}"#.to_owned() + "\n\n"])
                .holding(),
        );
        let adapter = adapter(&server);
        let cancel = CancellationToken::new();
        let mut stream = adapter.stream(&request(), &cancel);
        let mut seen = Vec::new();
        let mut failure = None;
        while let Some(delta) = stream.next().await {
            match delta {
                Ok(delta) => {
                    seen.push(delta);
                    cancel.cancel();
                }
                Err(error) => failure = Some(error),
            }
        }
        assert_eq!(texts(&seen), ["a"]);
        assert_eq!(failure.expect("an error").code, AppErrorCode::RunCancelled);
        drop(stream);
        assert!(server.wait_for_dropped_stream().await);
    }

    #[tokio::test]
    async fn reasoning_is_streamed_as_its_own_kind() {
        let server = MockServer::start().await;
        server.reply(
            "/v1/chat/completions",
            Reply::chunks([
                "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"think\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"reasoning\":\"more\",\"content\":\"say\"}}]}\n\n",
                "data: not json\n\n",
                "data: [DONE]\n\n",
            ]),
        );
        let deltas = drain(adapter(&server).stream(&request(), &CancellationToken::new()))
            .await
            .expect("streams");
        assert_eq!(
            deltas,
            [
                TextDelta::reasoning("think"),
                TextDelta::reasoning("more"),
                TextDelta::text("say"),
            ]
        );
    }

    #[tokio::test]
    async fn embeddings_are_posted_in_one_batch_and_remember_their_dimensions() {
        let server = MockServer::start().await;
        server.reply(
            "/v1/embeddings",
            Reply::json(json!({ "data": [
                { "embedding": [0.5, 0.25, "x"] },
                { "embedding": [1, 0] },
            ] })),
        );
        let adapter = adapter(&server);
        assert_eq!(adapter.dimensions(), None);
        assert!(
            adapter
                .embed(&[], "e", &CancellationToken::new())
                .await
                .expect("empty")
                .is_empty()
        );
        assert!(server.calls().is_empty());
        let vectors = adapter
            .embed(
                &["a".to_owned(), "b".to_owned()],
                "bge",
                &CancellationToken::new(),
            )
            .await
            .expect("embeds");
        assert_eq!(vectors, [vec![0.5, 0.25], vec![1.0, 0.0]]);
        assert_eq!(adapter.dimensions(), Some(2));
        assert_eq!(
            server.last_body(),
            json!({ "model": "bge", "input": ["a", "b"] })
        );
    }

    #[tokio::test]
    async fn tools_and_tool_messages_map_onto_the_wire_and_streamed_calls_are_assembled() {
        let server = MockServer::start().await;
        server.reply(
            "/v1/chat/completions",
            Reply::chunks(fixture_frames("openrouterToolCalls.sse")),
        );
        let deltas = drain(adapter(&server).stream(&tool_request(), &CancellationToken::new()))
            .await
            .expect("streams");
        assert_eq!(
            deltas,
            [
                TextDelta::text("Сейчас проверю."),
                TextDelta::ToolCall(ToolCall::new(
                    "call_Rk8Yv2",
                    "docs__search",
                    r#"{"query":"rotate token"}"#
                )),
                TextDelta::ToolCall(ToolCall::new("call_P0q1Lm", "clock__now", "{}")),
            ]
        );
        let body = server.last_body();
        assert_eq!(
            body["tools"],
            json!([{
                "type": "function",
                "function": {
                    "name": "docs__search",
                    "description": "Search the docs",
                    "parameters": search_tool().parameters,
                },
            }])
        );
        assert_eq!(
            body["messages"],
            json!([
                { "role": "system", "content": "Be brief." },
                { "role": "user", "content": "How do I rotate a token?" },
                {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": { "name": "docs__search", "arguments": "{\"query\":\"rotate\"}" },
                    }],
                },
                { "role": "tool", "tool_call_id": "call_1", "content": "Use the CLI." },
            ])
        );
    }

    #[tokio::test]
    async fn tool_calls_are_read_from_a_non_streamed_completion() {
        let server = MockServer::start().await;
        server.reply(
            "/v1/chat/completions",
            Reply::json(json!({
                "model": "openai/gpt-4o-mini",
                "choices": [{
                    "finish_reason": "tool_calls",
                    "message": {
                        "content": null,
                        "tool_calls": [{
                            "id": "call_9",
                            "type": "function",
                            "function": { "name": "docs__search", "arguments": "{\"query\":\"x\"}" },
                        }],
                    },
                }],
            })),
        );
        let result = adapter(&server)
            .generate(
                &GenerateRequest {
                    tools: vec![search_tool()],
                    ..GenerateRequest::new("m", vec![ChatMessage::user("x")])
                },
                &CancellationToken::new(),
            )
            .await
            .expect("generates");
        assert_eq!(result.finish_reason, FinishReason::ToolCalls);
        assert_eq!(result.text, "");
        assert_eq!(result.model, "openai/gpt-4o-mini");
        assert_eq!(
            result.tool_calls,
            [ToolCall::new("call_9", "docs__search", r#"{"query":"x"}"#)]
        );
    }

    #[tokio::test]
    async fn a_request_without_tools_sends_no_tools_field() {
        let server = MockServer::start().await;
        server.reply("/v1/chat/completions", Reply::chunks(Vec::<String>::new()));
        drain(adapter(&server).stream(
            &GenerateRequest::new("m", vec![ChatMessage::user("x")]),
            &CancellationToken::new(),
        ))
        .await
        .expect("streams");
        assert!(server.last_body().get("tools").is_none());
    }

    #[test]
    fn the_assembler_orders_by_index_names_missing_ids_and_defaults_empty_arguments() {
        let mut calls = ToolCallAssembler::default();
        calls.push(Some(&json!([
            { "index": 1, "function": { "name": "b", "arguments": { "k": 1, "a": 2 } } },
            { "index": 0, "id": "", "function": { "name": "a", "arguments": "  " } },
            { "index": 2, "function": { "arguments": "{}" } },
            "junk",
        ])));
        calls.push(Some(&json!([{ "index": 0, "function": { "name": "z" } }])));
        calls.push(Some(&json!({ "not": "an array" })));
        assert_eq!(
            calls.finish(),
            [
                ToolCall::new("call_0", "az", "{}"),
                ToolCall::new("call_1", "b", r#"{"k":1,"a":2}"#),
            ]
        );
    }

    #[test]
    fn finish_reasons_and_families_follow_the_vendor_vocabulary() {
        for (raw, reason) in [
            ("end_turn", FinishReason::Stop),
            ("max_tokens", FinishReason::Length),
            ("function_call", FinishReason::ToolCalls),
            ("aborted", FinishReason::Cancelled),
            ("other", FinishReason::Unknown),
        ] {
            assert_eq!(finish_reason(Some(&json!(raw))), reason);
        }
        assert_eq!(family_of("llama3"), None);
        assert_eq!(family_of("openai/gpt"), Some("openai".to_owned()));
        assert_eq!(family_of("qwen3:8b"), Some("qwen3".to_owned()));
        assert_eq!(usage(Some(&Value::Null)), None);
        assert_eq!(
            usage(Some(&json!({ "input_tokens": 2, "output_tokens": 1 }))),
            Some(TokenUsage {
                prompt_tokens: Some(2),
                completion_tokens: Some(1),
                total_tokens: None,
            })
        );
    }
}
