use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AdapterFamily, AppError, AppErrorCode, AuthMode, DeltaKind, ModelCapability, ThinkingEffort,
    ToolMode,
};

use super::account_family::flatten_prompt;
use crate::drivers::ai::capabilities::{AdapterCapabilities, Honours};
use crate::drivers::ai::copilot::{Client, failure, protocol_error};
use crate::drivers::ai::ports::{
    DeltaStream, DiscoveredModel, FinishReason, GenerateRequest, GenerateResult, SharedTextDriver,
    TextDelta, TextGenerationDriver,
};
use crate::drivers::ai::tool_dialect::{DefaultDialect, with_tool_dialect};

pub const COPILOT_CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    family: AdapterFamily::Copilot,
    auth_modes: &[AuthMode::Account],
    streaming: true,
    live_model_list: true,
    embedding: false,
    image: false,
    honours: Honours::NONE,
    tools: ToolMode::Injected,
};
pub const EFFORT_UNSUPPORTED: &str =
    "Выбранная модель Copilot не поддерживает этот режим рассуждения. Выберите «Авто»";

pub struct CopilotAdapter {
    identity: String,
}

pub fn copilot_driver(identity: String) -> SharedTextDriver {
    with_tool_dialect(
        Arc::new(CopilotAdapter { identity }),
        Arc::new(DefaultDialect),
        None,
    )
}

fn requested_effort(request: &GenerateRequest) -> Option<&'static str> {
    match request.vendor.effort {
        Some(ThinkingEffort::Fast) => Some("low"),
        Some(ThinkingEffort::Thinking) => Some("high"),
        Some(ThinkingEffort::Auto) | None => None,
    }
}

fn discovered(model: zvs_copilot::Model) -> DiscoveredModel {
    let mut capabilities = vec![ModelCapability::Streaming];
    if model.effort(Some("low")).is_ok() && model.effort(Some("high")).is_ok() {
        capabilities.push(ModelCapability::Reasoning);
        capabilities.push(ModelCapability::AdaptiveReasoning);
    }
    DiscoveredModel {
        display_name: model.name,
        context_window: model
            .capabilities
            .limits
            .max_context_window_tokens
            .filter(|n| *n > 0),
        max_output: model
            .capabilities
            .limits
            .max_output_tokens
            .filter(|n| *n > 0),
        capabilities,
        ..DiscoveredModel::named(model.id)
    }
}

enum TurnEvent {
    Delta(TextDelta),
    Done,
    Ignore,
}

fn decode(event: &Value) -> Result<TurnEvent, AppError> {
    match event["type"].as_str() {
        Some("assistant.message_delta" | "assistant.reasoning_delta") => {
            let text = event["data"]["deltaContent"]
                .as_str()
                .ok_or_else(protocol_error)?;
            Ok(TurnEvent::Delta(
                if event["type"] == "assistant.reasoning_delta" {
                    TextDelta::reasoning(text)
                } else {
                    TextDelta::text(text)
                },
            ))
        }
        Some("session.idle") => Ok(TurnEvent::Done),
        Some("session.error" | "session.shutdown") => Err(failure()),
        _ => Ok(TurnEvent::Ignore),
    }
}

#[async_trait]
impl TextGenerationDriver for CopilotAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        COPILOT_CAPABILITIES
    }

    async fn list_models(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        let mut client = Client::start(cancel).await?;
        client.auth(Some(&self.identity)).await?;
        Ok(client
            .models()
            .await?
            .into_iter()
            .filter(zvs_copilot::Model::enabled)
            .map(discovered)
            .collect())
    }

    fn stream(&self, request: &GenerateRequest, cancel: &CancellationToken) -> DeltaStream {
        let identity = self.identity.clone();
        let request = request.clone();
        let cancel = cancel.clone();
        let start = async move {
            let mut client = Client::start(&cancel).await?;
            client.auth(Some(&identity)).await?;
            let models = client.models().await?;
            let model = models
                .iter()
                .find(|model| model.id == request.model && model.enabled())
                .ok_or_else(|| {
                    AppError::new(
                        AppErrorCode::Conflict,
                        "Выбранная модель Copilot недоступна",
                    )
                })?;
            let effort = model
                .effort(requested_effort(&request))
                .map_err(|_| AppError::new(AppErrorCode::ValidationFailed, EFFORT_UNSUPPORTED))?;
            let session = client.session(Some(&request.model), effort).await?;
            client
                .call(
                    "session.send",
                    json!({"sessionId":session, "prompt":flatten_prompt(&request)}),
                )
                .await?;
            Ok::<_, AppError>((client, session))
        };
        futures::stream::once(start)
            .flat_map(|result| {
                let state = match result {
                    Ok(state) => Ok(state),
                    Err(error) => Err(Some(error)),
                };
                futures::stream::unfold(Some(state), |state| async move {
                    let state = state?;
                    let (mut client, session) = match state {
                        Ok(state) => state,
                        Err(mut error) => return error.take().map(|error| (Err(error), None)),
                    };
                    loop {
                        match client
                            .event(&session)
                            .await
                            .and_then(|event| decode(&event))
                        {
                            Ok(TurnEvent::Delta(delta)) => {
                                return Some((Ok(delta), Some(Ok((client, session)))));
                            }
                            Ok(TurnEvent::Done) => return None,
                            Ok(TurnEvent::Ignore) => {}
                            Err(error) => return Some((Err(error), None)),
                        }
                    }
                })
            })
            .boxed()
    }

    async fn generate(
        &self,
        request: &GenerateRequest,
        cancel: &CancellationToken,
    ) -> Result<GenerateResult, AppError> {
        let mut stream = self.stream(request, cancel);
        let mut text = String::new();
        let mut reasoning = String::new();
        while let Some(delta) = stream.next().await {
            if let TextDelta::Content { text: delta, kind } = delta? {
                match kind {
                    DeltaKind::Text => text.push_str(&delta),
                    DeltaKind::Reasoning => reasoning.push_str(&delta),
                }
            }
        }
        Ok(GenerateResult {
            model: request.model.clone(),
            text,
            reasoning: (!reasoning.is_empty()).then_some(reasoning),
            finish_reason: FinishReason::Stop,
            usage: None,
            tool_calls: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_and_terminal_events_are_distinct_and_errors_are_not_success() {
        assert!(matches!(
            decode(&json!({"type":"session.idle"})).unwrap(),
            TurnEvent::Done
        ));
        assert!(matches!(
            decode(&json!({"type":"assistant.message_delta", "data":{"deltaContent":"hello"}}))
                .unwrap(),
            TurnEvent::Delta(TextDelta::Content {
                kind: DeltaKind::Text,
                ..
            })
        ));
        assert!(decode(&json!({"type":"session.error"})).is_err());
        assert!(decode(&json!({"type":"assistant.message_delta"})).is_err());
    }
}
