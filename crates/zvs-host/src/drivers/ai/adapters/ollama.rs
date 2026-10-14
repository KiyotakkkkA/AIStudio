use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::BoxStream;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AdapterFamily, AppError, AuthMode, ModelCapability, ToolMode};

use super::wire::{
    decode, field, is_assistant_call, normalise, object, open_frames, plain_message, present,
    stringify, wire_tools,
};
use crate::drivers::ai::capabilities::{AdapterCapabilities, Honours};
use crate::drivers::ai::errors::request_failed;
use crate::drivers::ai::ports::{
    ChatMessage, DeltaStream, DiscoveredModel, FinishReason, GenerateRequest, GenerateResult,
    TextDelta, TextGenerationDriver, TokenUsage, ToolCall,
};
use crate::drivers::ai::sse::{FrameStream, ndjson_lines};
use crate::drivers::ai::transport::{RequestSpec, SharedTransport, TransportResponse};
use crate::platform::logger::{LogLevel, SharedLogger};

pub const OLLAMA_CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    family: AdapterFamily::OpenaiCompatible,
    auth_modes: &[AuthMode::Api],
    streaming: true,
    live_model_list: true,
    embedding: false,
    image: false,
    honours: Honours::ALL,
    tools: ToolMode::Native,
};

pub struct OllamaAdapter {
    transport: SharedTransport,
    logger: SharedLogger,
}

struct Step {
    frame: Value,
    deltas: Vec<TextDelta>,
}

struct StepState {
    frames: FrameStream,
    calls: usize,
    finished: bool,
}

impl OllamaAdapter {
    pub fn new(transport: SharedTransport, logger: SharedLogger) -> Self {
        Self { transport, logger }
    }

    async fn request(
        &self,
        spec: &RequestSpec,
        cancel: &CancellationToken,
    ) -> Result<TransportResponse, AppError> {
        self.transport.request(spec, cancel).await.map_err(|error| {
            self.logger.log(
                LogLevel::Debug,
                "ai",
                "Ollama request failed",
                &[("path", json!(spec.path))],
            );
            normalise(error)
        })
    }

    fn steps(
        &self,
        request: &GenerateRequest,
        cancel: &CancellationToken,
    ) -> BoxStream<'static, Result<Step, AppError>> {
        let spec =
            RequestSpec::post("/chat", chat_body(request)).with_accept("application/x-ndjson");
        let state = StepState {
            frames: open_frames(self.transport.clone(), spec, cancel.clone(), ndjson_lines),
            calls: 0,
            finished: false,
        };
        futures::stream::unfold(state, |mut state| async move {
            if state.finished {
                return None;
            }
            let step = match state.frames.next().await? {
                Ok(line) => serde_json::from_str::<Value>(&line)
                    .map_err(|_| request_failed())
                    .map(|frame| {
                        let deltas = frame_deltas(&frame, &mut state.calls);
                        Step { frame, deltas }
                    }),
                Err(error) => Err(normalise(error)),
            };
            state.finished = step.is_err();
            Some((step, state))
        })
        .boxed()
    }
}

fn chat_body(request: &GenerateRequest) -> Value {
    let messages = request
        .system
        .iter()
        .filter(|system| !system.is_empty())
        .map(|system| ChatMessage::system(system.clone()))
        .chain(request.messages.iter().cloned())
        .map(|message| wire_message(&message))
        .collect::<Vec<_>>();
    let mut body = object([
        ("model", Value::from(request.model.clone())),
        ("messages", Value::Array(messages)),
    ]);
    let fields = body.as_object_mut().expect("an object");
    if !request.tools.is_empty() {
        fields.insert("tools".to_owned(), wire_tools(&request.tools));
    }
    fields.insert("stream".to_owned(), Value::Bool(true));
    let mut options = Map::new();
    if let Some(temperature) = request.temperature {
        options.insert("temperature".to_owned(), json!(temperature));
    }
    if let Some(top_k) = request.top_k {
        options.insert("top_k".to_owned(), json!(top_k));
    }
    if let Some(top_p) = request.top_p {
        options.insert("top_p".to_owned(), json!(top_p));
    }
    if let Some(max_output_tokens) = request.max_output_tokens {
        options.insert("num_predict".to_owned(), json!(max_output_tokens));
    }
    fields.insert("options".to_owned(), Value::Object(options));
    body
}

fn wire_message(message: &ChatMessage) -> Value {
    if !is_assistant_call(message) {
        return plain_message(message);
    }
    object([
        ("role", Value::from("assistant")),
        ("content", Value::from(message.content.clone())),
        (
            "tool_calls",
            Value::Array(
                message
                    .tool_calls
                    .iter()
                    .map(|call| {
                        object([(
                            "function",
                            object([
                                ("name", Value::from(call.name.clone())),
                                (
                                    "arguments",
                                    serde_json::from_str(&call.arguments)
                                        .unwrap_or_else(|_| json!({})),
                                ),
                            ]),
                        )])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn tool_calls_of(raw: Option<&Value>) -> Vec<ToolCall> {
    let Some(Value::Array(entries)) = raw else {
        return Vec::new();
    };
    entries
        .iter()
        .filter(|entry| entry.is_object())
        .filter_map(|entry| {
            let function = entry.get("function");
            let name = field(function, "name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())?;
            let arguments = match present(field(function, "arguments")) {
                Some(Value::String(arguments)) => arguments.clone(),
                Some(arguments) => stringify(arguments),
                None => "{}".to_owned(),
            };
            Some(ToolCall {
                id: entry
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                name: name.to_owned(),
                arguments,
            })
        })
        .collect()
}

fn frame_deltas(frame: &Value, calls: &mut usize) -> Vec<TextDelta> {
    let message = frame.get("message");
    let mut deltas = Vec::new();
    if let Some(thinking) = field(message, "thinking")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
    {
        deltas.push(TextDelta::reasoning(thinking));
    }
    if let Some(content) = field(message, "content")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
    {
        deltas.push(TextDelta::text(content));
    }
    for mut call in tool_calls_of(field(message, "tool_calls")) {
        *calls += 1;
        if call.id.is_empty() {
            call.id = format!("call_{calls}");
        }
        deltas.push(TextDelta::ToolCall(call));
    }
    deltas
}

fn count(value: Option<&Value>) -> Option<i64> {
    let number = value.and_then(Value::as_number)?;
    number
        .as_i64()
        .or_else(|| number.as_f64().map(|float| float as i64))
}

fn usage_of(frame: &Value) -> Option<TokenUsage> {
    let prompt_tokens = count(frame.get("prompt_eval_count"));
    let completion_tokens = count(frame.get("eval_count"));
    if prompt_tokens.is_none() && completion_tokens.is_none() {
        return None;
    }
    Some(TokenUsage {
        prompt_tokens,
        completion_tokens,
        total_tokens: prompt_tokens
            .zip(completion_tokens)
            .map(|(prompt, completion)| prompt + completion),
    })
}

#[async_trait]
impl TextGenerationDriver for OllamaAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        OLLAMA_CAPABILITIES
    }

    async fn generate(
        &self,
        request: &GenerateRequest,
        cancel: &CancellationToken,
    ) -> Result<GenerateResult, AppError> {
        let mut result = GenerateResult {
            model: request.model.clone(),
            text: String::new(),
            reasoning: None,
            finish_reason: FinishReason::Unknown,
            usage: None,
            tool_calls: Vec::new(),
        };
        let mut reasoning = String::new();
        let mut steps = self.steps(request, cancel);
        while let Some(step) = steps.next().await {
            let step = step?;
            if let Some(model) = step.frame.get("model").and_then(Value::as_str) {
                model.clone_into(&mut result.model);
            }
            result.usage = usage_of(&step.frame).or(result.usage);
            if step.frame.get("done") == Some(&Value::Bool(true)) {
                result.finish_reason =
                    if step.frame.get("done_reason").and_then(Value::as_str) == Some("length") {
                        FinishReason::Length
                    } else {
                        FinishReason::Stop
                    };
            }
            for delta in step.deltas {
                match delta {
                    TextDelta::ToolCall(call) => result.tool_calls.push(call),
                    TextDelta::Content { text, kind } => match kind {
                        zvs_contract::DeltaKind::Reasoning => reasoning.push_str(&text),
                        zvs_contract::DeltaKind::Text => result.text.push_str(&text),
                    },
                }
            }
        }
        if !result.tool_calls.is_empty() {
            result.finish_reason = FinishReason::ToolCalls;
        }
        result.reasoning = (!reasoning.is_empty()).then_some(reasoning);
        Ok(result)
    }

    fn stream(&self, request: &GenerateRequest, cancel: &CancellationToken) -> DeltaStream {
        self.steps(request, cancel)
            .flat_map(|step| {
                futures::stream::iter(match step {
                    Ok(step) => step.deltas.into_iter().map(Ok).collect::<Vec<_>>(),
                    Err(error) => vec![Err(error)],
                })
            })
            .boxed()
    }

    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        let response = self.request(&RequestSpec::get("/tags"), cancel).await?;
        let payload = decode(&response.text)?;
        let models = present(payload.get("models"))
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        Ok(models
            .iter()
            .filter_map(|entry| {
                let name = entry.get("name").and_then(Value::as_str)?.trim();
                if name.is_empty() {
                    return None;
                }
                Some(DiscoveredModel {
                    family: field(entry.get("details"), "family")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    size_bytes: count(entry.get("size")),
                    capabilities: vec![ModelCapability::Streaming],
                    ..DiscoveredModel::named(name)
                })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::ai::mock::{MockServer, Reply, fixture_text, tool_request, transport_for};
    use crate::platform::logger::silent_logger;
    use std::time::Duration;
    use zvs_contract::AppErrorCode;

    fn adapter(server: &MockServer) -> OllamaAdapter {
        OllamaAdapter::new(transport_for(&server.url("/api")), silent_logger())
    }

    fn lines(frames: &[Value]) -> Vec<String> {
        frames.iter().map(|frame| format!("{frame}\n")).collect()
    }

    async fn drain(stream: DeltaStream) -> Result<Vec<TextDelta>, AppError> {
        stream.collect::<Vec<_>>().await.into_iter().collect()
    }

    #[tokio::test]
    async fn ollama_maps_tools_natively_and_turns_object_arguments_into_a_json_string() {
        let server = MockServer::start().await;
        server.reply(
            "/api/chat",
            Reply::chunks([fixture_text("ollamaToolCalls.ndjson")]),
        );
        let deltas = drain(adapter(&server).stream(&tool_request(), &CancellationToken::new()))
            .await
            .expect("streams");
        assert_eq!(
            deltas,
            [TextDelta::ToolCall(ToolCall::new(
                "call_1",
                "docs__search",
                r#"{"query":"rotate token","k":3}"#
            ))]
        );
        let call = server.last();
        assert_eq!(
            call.header("accept").as_deref(),
            Some("application/x-ndjson")
        );
        let body = call.json();
        assert_eq!(body["tools"].as_array().map(Vec::len), Some(1));
        assert_eq!(
            body["messages"][2],
            json!({
                "role": "assistant",
                "content": "",
                "tool_calls": [{ "function": { "name": "docs__search", "arguments": { "query": "rotate" } } }],
            })
        );
        assert_eq!(
            body["messages"][3],
            json!({ "role": "tool", "content": "Use the CLI." })
        );
        assert_eq!(
            body["messages"][0],
            json!({ "role": "system", "content": "Be brief." })
        );
    }

    #[tokio::test]
    async fn every_tunable_parameter_is_sent_as_an_ollama_option() {
        let server = MockServer::start().await;
        server.reply("/api/chat", Reply::chunks(Vec::<String>::new()));
        let request = GenerateRequest {
            system: Some(String::new()),
            temperature: Some(0.4),
            top_k: Some(40),
            top_p: Some(0.9),
            max_output_tokens: Some(128),
            ..GenerateRequest::new("llama3", vec![ChatMessage::user("Привет")])
        };
        drain(adapter(&server).stream(&request, &CancellationToken::new()))
            .await
            .expect("streams");
        assert_eq!(
            server.last_body(),
            json!({
                "model": "llama3",
                "messages": [{ "role": "user", "content": "Привет" }],
                "stream": true,
                "options": { "temperature": 0.4, "top_k": 40, "top_p": 0.9, "num_predict": 128 },
            })
        );
    }

    #[tokio::test]
    async fn generate_collects_text_reasoning_usage_and_the_finish_reason() {
        let server = MockServer::start().await;
        server.reply(
            "/api/chat",
            Reply::chunks(lines(&[
                json!({ "model": "qwen3:8b", "message": { "thinking": "hm", "content": "" }, "done": false }),
                json!({ "message": { "content": "При" }, "done": false }),
                json!({ "message": { "content": "вет" }, "done": true, "done_reason": "length", "prompt_eval_count": 5, "eval_count": 2 }),
            ])),
        );
        let result = adapter(&server)
            .generate(
                &GenerateRequest::new("llama3", vec![ChatMessage::user("x")]),
                &CancellationToken::new(),
            )
            .await
            .expect("generates");
        assert_eq!(result.model, "qwen3:8b");
        assert_eq!(result.text, "Привет");
        assert_eq!(result.reasoning.as_deref(), Some("hm"));
        assert_eq!(result.finish_reason, FinishReason::Length);
        assert_eq!(
            result.usage,
            Some(TokenUsage {
                prompt_tokens: Some(5),
                completion_tokens: Some(2),
                total_tokens: Some(7),
            })
        );
        assert!(result.tool_calls.is_empty());
    }

    #[tokio::test]
    async fn a_generated_tool_call_sets_the_finish_reason() {
        let server = MockServer::start().await;
        server.reply(
            "/api/chat",
            Reply::chunks([fixture_text("ollamaToolCalls.ndjson")]),
        );
        let result = adapter(&server)
            .generate(&tool_request(), &CancellationToken::new())
            .await
            .expect("generates");
        assert_eq!(result.finish_reason, FinishReason::ToolCalls);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.usage.and_then(|usage| usage.total_tokens), Some(204));
        assert_eq!(result.reasoning, None);
    }

    #[tokio::test]
    async fn a_line_that_is_not_json_fails_the_stream() {
        let server = MockServer::start().await;
        server.reply(
            "/api/chat",
            Reply::chunks([
                "{\"message\":{\"content\":\"a\"}}\n",
                "oops\n",
                "{\"message\":{\"content\":\"b\"}}\n",
            ]),
        );
        let results = adapter(&server)
            .stream(
                &GenerateRequest::new("m", vec![ChatMessage::user("x")]),
                &CancellationToken::new(),
            )
            .collect::<Vec<_>>()
            .await;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].as_ref().expect("a delta"), &TextDelta::text("a"));
        assert_eq!(
            results[1].as_ref().expect_err("an error").code,
            AppErrorCode::Unknown
        );
    }

    #[tokio::test]
    async fn cancelling_an_ollama_stream_ends_it_as_cancelled() {
        let server = MockServer::start().await;
        server.reply(
            "/api/chat",
            Reply::chunks(["{\"message\":{\"content\":\"a\"}}\n"]).holding(),
        );
        let cancel = CancellationToken::new();
        let mut stream = adapter(&server).stream(
            &GenerateRequest::new("m", vec![ChatMessage::user("x")]),
            &cancel,
        );
        assert_eq!(
            stream.next().await.expect("a delta").expect("text"),
            TextDelta::text("a")
        );
        let trigger = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            trigger.cancel();
        });
        let ended = stream
            .next()
            .await
            .expect("an outcome")
            .expect_err("cancelled");
        assert_eq!(ended.code, AppErrorCode::RunCancelled);
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn tags_are_listed_with_family_and_size() {
        let server = MockServer::start().await;
        server.reply(
            "/api/tags",
            Reply::json(json!({ "models": [
                { "name": " llama3:8b ", "size": 4_661_224_676_i64, "details": { "family": "llama" } },
                { "name": "nomic-embed-text", "details": null },
                { "name": "" },
                { "size": 1 },
            ] })),
        );
        let models = adapter(&server)
            .list_models(&CancellationToken::new())
            .await
            .expect("lists");
        assert_eq!(
            models,
            [
                DiscoveredModel {
                    family: Some("llama".to_owned()),
                    size_bytes: Some(4_661_224_676),
                    capabilities: vec![ModelCapability::Streaming],
                    ..DiscoveredModel::named("llama3:8b")
                },
                DiscoveredModel {
                    capabilities: vec![ModelCapability::Streaming],
                    ..DiscoveredModel::named("nomic-embed-text")
                },
            ]
        );
        assert_eq!(server.paths(), ["/api/tags"]);
    }

    #[tokio::test]
    async fn a_failed_tags_request_keeps_its_mapped_error() {
        let server = MockServer::start().await;
        server.reply("/api/tags", Reply::text("down").with_status(502));
        let error = adapter(&server)
            .list_models(&CancellationToken::new())
            .await
            .expect_err("fails");
        assert_eq!(error.code, AppErrorCode::ProviderUnreachable);
    }
}
