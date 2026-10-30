use async_trait::async_trait;
use futures::{StreamExt, stream::BoxStream};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AdapterFamily, AppError, AppErrorCode, AuthMode, ModelCapability, ProviderUsageDto,
    ThinkingEffort, ToolMode,
};

use super::wire::{decode, open_frames};
use crate::drivers::ai::capabilities::{AdapterCapabilities, Honours};
use crate::drivers::ai::openai::failure;
use crate::drivers::ai::ports::{
    ChatRole, DeltaStream, DiscoveredModel, FinishReason, GenerateRequest, GenerateResult,
    TextDelta, TextGenerationDriver, TokenUsage, ToolCall,
};
use crate::drivers::ai::transport::{RequestSpec, SharedTransport};

pub mod codex;

pub const OPENAI_CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    family: AdapterFamily::Openai,
    auth_modes: &[AuthMode::Account],
    streaming: true,
    live_model_list: true,
    embedding: false,
    image: false,
    honours: Honours {
        temperature: false,
        top_k: false,
        top_p: false,
        max_output_tokens: false,
    },
    tools: ToolMode::Native,
};

#[derive(Clone)]
pub struct OpenaiAdapter {
    transport: SharedTransport,
    codex: bool,
}

pub enum Event {
    Delta(TextDelta),
    Completed(Option<TokenUsage>),
}

pub type Events = BoxStream<'static, Result<Event, AppError>>;

fn reasoning_effort(request: &GenerateRequest) -> Option<&'static str> {
    match request.vendor.effort {
        Some(ThinkingEffort::Fast) => Some("low"),
        Some(ThinkingEffort::Thinking) => Some("high"),
        Some(ThinkingEffort::Auto) | None => None,
    }
}

pub fn request_body(request: &GenerateRequest) -> Value {
    let mut input = Vec::new();
    for message in &request.messages {
        if message.role == ChatRole::Tool {
            input.push(json!({"type":"function_call_output", "call_id":message.tool_call_id, "output":message.content}));
            continue;
        }
        if !message.content.is_empty() {
            let role = if message.role == ChatRole::System {
                "developer"
            } else {
                message.role.as_str()
            };
            input.push(json!({"role":role, "content":message.content}));
        }
        for call in &message.tool_calls {
            input.push(json!({"type":"function_call", "call_id":call.id, "name":call.name, "namespace":"zvs", "arguments":call.arguments}));
        }
    }
    let mut body = json!({"model":request.model, "input":input, "store":false, "stream":true});
    if let Some(effort) = reasoning_effort(request) {
        body["reasoning"] = json!({"effort":effort});
    }
    if let Some(system) = &request.system {
        body["instructions"] = json!(system);
    }
    if !request.tools.is_empty() {
        body["tools"] = json!([{"type":"namespace", "name":"zvs", "description":"Tools provided by ZVS AI Studio", "tools":request.tools.iter().map(|tool|
            json!({"type":"function", "name":tool.name, "description":tool.description, "parameters":tool.parameters, "strict":false})
        ).collect::<Vec<_>>()}]);
    }
    body
}

pub fn stream_error(value: &Value) -> AppError {
    let error = value
        .pointer("/response/error")
        .or_else(|| value.get("error"))
        .unwrap_or(value);
    let code = error
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or("openai-stream-failed");
    let limited = matches!(
        code,
        "subscription_sharing_usage_limit_exceeded" | "rate_limit_exceeded" | "usage_limit_reached"
    );
    AppError::new(
        if limited {
            AppErrorCode::RateLimited
        } else {
            AppErrorCode::ProviderUnreachable
        },
        if limited {
            "Достигнут лимит использования подписки OpenAI"
        } else {
            "OpenAI не завершил ответ"
        },
    )
    .with_detail("reason", code)
}

pub fn decode_event(value: &Value) -> Result<Option<Event>, AppError> {
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match kind {
        "response.output_text.delta" => Ok(value
            .get("delta")
            .and_then(Value::as_str)
            .map(|text| Event::Delta(TextDelta::text(text)))),
        "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => Ok(value
            .get("delta")
            .and_then(Value::as_str)
            .map(|text| Event::Delta(TextDelta::reasoning(text)))),
        "response.output_item.done"
            if value.pointer("/item/type").and_then(Value::as_str) == Some("function_call") =>
        {
            let item = &value["item"];
            let field = |key| item.get(key).and_then(Value::as_str).ok_or_else(failure);
            Ok(Some(Event::Delta(TextDelta::ToolCall(ToolCall::new(
                field("call_id")?,
                field("name")?,
                field("arguments")?,
            )))))
        }
        "response.completed" => {
            if value
                .pointer("/response/status")
                .and_then(Value::as_str)
                .is_some_and(|status| status != "completed")
            {
                return Err(stream_error(value));
            }
            let usage = value
                .pointer("/response/usage")
                .filter(|usage| usage.is_object())
                .map(|usage| TokenUsage {
                    prompt_tokens: usage.get("input_tokens").and_then(Value::as_i64),
                    completion_tokens: usage.get("output_tokens").and_then(Value::as_i64),
                    total_tokens: usage.get("total_tokens").and_then(Value::as_i64),
                });
            Ok(Some(Event::Completed(usage)))
        }
        "response.failed" | "response.incomplete" | "error" => Err(stream_error(value)),
        _ => Ok(None),
    }
}

impl OpenaiAdapter {
    pub fn new(transport: SharedTransport, codex: bool) -> Self {
        Self { transport, codex }
    }

    fn events(&self, request: &GenerateRequest, cancel: &CancellationToken) -> Events {
        if self.codex {
            return codex::events(self.transport.clone(), request.clone(), cancel.clone());
        }
        let transport = self.transport.clone();
        let cancel = cancel.clone();
        let spec = RequestSpec::post("/responses", request_body(request));
        let frames = open_frames(transport, spec, cancel, crate::drivers::ai::sse::sse_data);
        futures::stream::unfold(
            (frames, false, false),
            |(mut frames, mut completed, mut ended)| async move {
                if ended || completed {
                    return None;
                }
                loop {
                    match frames.next().await {
                        Some(Ok(frame)) => {
                            let event = decode(&frame).and_then(|value| decode_event(&value));
                            match event {
                                Ok(Some(event)) => {
                                    if matches!(event, Event::Completed(_)) {
                                        completed = true;
                                    }
                                    return Some((Ok(event), (frames, completed, ended)));
                                }
                                Ok(None) => {}
                                Err(error) => {
                                    ended = true;
                                    return Some((Err(error), (frames, completed, ended)));
                                }
                            }
                        }
                        Some(Err(error)) => return Some((Err(error), (frames, completed, true))),
                        None if completed => return None,
                        None => {
                            return Some((
                                Err(failure().with_detail("reason", "incomplete-stream")),
                                (frames, completed, true),
                            ));
                        }
                    }
                }
            },
        )
        .boxed()
    }
}

#[async_trait]
impl TextGenerationDriver for OpenaiAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        OPENAI_CAPABILITIES
    }

    async fn generate(
        &self,
        request: &GenerateRequest,
        cancel: &CancellationToken,
    ) -> Result<GenerateResult, AppError> {
        let mut events = self.events(request, cancel);
        let mut result = GenerateResult {
            model: request.model.clone(),
            text: String::new(),
            reasoning: None,
            finish_reason: FinishReason::Stop,
            usage: None,
            tool_calls: Vec::new(),
        };
        while let Some(event) = events.next().await {
            match event? {
                Event::Delta(TextDelta::Content {
                    text,
                    kind: zvs_contract::DeltaKind::Text,
                }) => result.text.push_str(&text),
                Event::Delta(TextDelta::Content { text, .. }) => {
                    result.reasoning.get_or_insert_default().push_str(&text)
                }
                Event::Delta(TextDelta::ToolCall(call)) => result.tool_calls.push(call),
                Event::Delta(TextDelta::WebSources(_)) => {}
                Event::Completed(usage) => result.usage = usage,
            }
        }
        if !result.tool_calls.is_empty() {
            result.finish_reason = FinishReason::ToolCalls;
        }
        Ok(result)
    }

    fn stream(&self, request: &GenerateRequest, cancel: &CancellationToken) -> DeltaStream {
        self.events(request, cancel)
            .filter_map(|event| async move {
                match event {
                    Ok(Event::Delta(delta)) => Some(Ok(delta)),
                    Ok(Event::Completed(_)) => None,
                    Err(error) => Some(Err(error)),
                }
            })
            .boxed()
    }

    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        let response = self
            .transport
            .request(&RequestSpec::get("/models"), cancel)
            .await?;
        let catalog = zvs_openai::models(response.json()?).map_err(|_| failure())?;
        Ok(catalog
            .into_iter()
            .map(|model| DiscoveredModel {
                display_name: model.display_name,
                context_window: model.context_window,
                family: Some("openai".to_owned()),
                capabilities: vec![
                    ModelCapability::Streaming,
                    ModelCapability::Tools,
                    ModelCapability::Reasoning,
                    ModelCapability::AdaptiveReasoning,
                ],
                ..DiscoveredModel::named(model.slug)
            })
            .collect())
    }

    async fn usage(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Option<ProviderUsageDto>, AppError> {
        let mut usage = ProviderUsageDto {
            source: if self.codex { "codex" } else { "direct" }.to_owned(), windows: Vec::new(),
            detail: Some("OpenAI не передаёт остаток лимита для этого подключения. Расход подписки доступен в настройках ChatGPT.".to_owned()),
            checked_at: crate::drivers::ai::openai::now_seconds() * 1000,
        };
        if self.codex {
            match codex::usage(self.transport.clone(), cancel).await {
                Ok(windows) if !windows.is_empty() => {
                    usage.windows = windows;
                    usage.detail = None;
                }
                Ok(_) => {}
                Err(error) if error.code == AppErrorCode::NotFound => {
                    usage.detail = Some(error.message)
                }
                Err(_) => {}
            }
        }
        Ok(Some(usage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::ai::mock::{MockServer, Reply, transport_for};
    use crate::drivers::ai::ports::ChatMessage;

    #[tokio::test]
    async fn fragmented_stream_requires_completion_and_retains_usage() {
        let server = MockServer::start().await;
        let frames = [json!({"type":"response.output_text.delta", "delta":"Ответ"}),
            json!({"type":"response.completed", "response":{"status":"completed", "usage":{"input_tokens":5,"output_tokens":2,"total_tokens":7}}})]
            .map(|value| format!("data: {value}\n\n"));
        server.reply(
            "/v1/responses",
            Reply::chunks([
                frames[0][..10].to_owned(),
                frames[0][10..].to_owned(),
                frames[1].clone(),
            ]),
        );
        let driver = OpenaiAdapter::new(transport_for(&server.url("/v1")), false);
        let request = GenerateRequest::new("selected", vec![ChatMessage::user("Привет")]);
        let result = driver
            .generate(&request, &CancellationToken::new())
            .await
            .expect("completed");
        assert_eq!(result.text, "Ответ");
        assert_eq!(result.usage.expect("usage").total_tokens, Some(7));
        server.reply("/v1/responses", Reply::chunks([frames[0].clone()]));
        assert!(
            driver
                .generate(&request, &CancellationToken::new())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn model_discovery_reads_the_account_catalog() {
        let server = MockServer::start().await;
        server.reply("/v1/models", Reply::json(json!({"models":[
            {"slug":"visible","display_name":"Visible","visibility":"list","context_window":32000},
            {"slug":"hidden","display_name":"Hidden","visibility":"hide"}
        ]})));
        let driver = OpenaiAdapter::new(transport_for(&server.url("/v1")), false);
        let models = driver
            .list_models(&CancellationToken::new())
            .await
            .expect("catalog");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].external_id, "visible");
        assert_eq!(models[0].context_window, Some(32000));
    }

    #[tokio::test]
    async fn a_limit_error_after_text_does_not_return_a_successful_generation() {
        let server = MockServer::start().await;
        server.reply("/responses", Reply::chunks([
            format!("data: {}\n\n", json!({"type":"response.output_text.delta", "delta":"partial"})),
            format!("data: {}\n\n", json!({"type":"response.failed", "response":{"error":{"code":"subscription_sharing_usage_limit_exceeded"}}})),
        ]));
        let driver = OpenaiAdapter::new(transport_for(&server.origin()), false);
        let error = driver
            .generate(
                &GenerateRequest::new("selected", Vec::new()),
                &CancellationToken::new(),
            )
            .await
            .expect_err("limit");
        assert_eq!(error.code, AppErrorCode::RateLimited);
    }

    #[test]
    fn selected_effort_reaches_responses_and_default_remains_unspecified() {
        for (effort, expected) in [
            (None, None),
            (Some(ThinkingEffort::Auto), None),
            (Some(ThinkingEffort::Fast), Some("low")),
            (Some(ThinkingEffort::Thinking), Some("high")),
        ] {
            let mut request = GenerateRequest::new("selected", Vec::new());
            request.vendor.effort = effort;
            let body = request_body(&request);
            assert_eq!(
                body.pointer("/reasoning/effort").and_then(Value::as_str),
                expected
            );
            if expected.is_none() {
                assert!(body.get("reasoning").is_none());
            }
        }
    }

    #[test]
    fn request_preserves_tool_history_and_omits_unsupported_parameters() {
        let request = GenerateRequest {
            system: Some("instructions".to_owned()),
            temperature: Some(1.0),
            max_output_tokens: Some(10),
            ..GenerateRequest::new(
                "selected",
                vec![
                    ChatMessage::system("developer"),
                    ChatMessage::assistant("")
                        .with_tool_calls(vec![ToolCall::new("call", "lookup", "{}")]),
                    ChatMessage::tool("call", "result"),
                ],
            )
        };
        let body = request_body(&request);
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert_eq!(body["input"][0]["role"], "developer");
        assert_eq!(body["input"][1]["type"], "function_call");
        assert_eq!(body["input"][2]["call_id"], "call");
        assert!(body.get("temperature").is_none());
        assert!(body.get("max_output_tokens").is_none());
    }

    #[test]
    fn a_limit_failure_after_deltas_is_an_error() {
        let error = decode_event(
            &json!({"type":"response.failed","response":{"error":{"code":"subscription_sharing_usage_limit_exceeded"}}}),
        );
        assert!(matches!(
            error,
            Err(AppError {
                code: AppErrorCode::RateLimited,
                ..
            })
        ));
        assert!(decode_event(&json!({"type":"response.incomplete"})).is_err());
    }
}
