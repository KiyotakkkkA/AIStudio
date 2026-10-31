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
pub const AUTO_MODEL_NAME: &str = "Автовыбор модели";
pub const MODEL_UNAVAILABLE: &str = "Выбранная модель Copilot недоступна";
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

fn auto_model() -> DiscoveredModel {
    DiscoveredModel {
        display_name: AUTO_MODEL_NAME.to_owned(),
        capabilities: vec![ModelCapability::Streaming],
        ..DiscoveredModel::named(zvs_copilot::AUTO_MODEL)
    }
}

fn catalog(models: Vec<zvs_copilot::Model>) -> Vec<DiscoveredModel> {
    zvs_copilot::selectable(models)
        .into_iter()
        .map(discovered)
        .chain(std::iter::once(auto_model()))
        .collect()
}

fn session_effort(
    models: &[zvs_copilot::Model],
    request: &GenerateRequest,
) -> Result<Option<&'static str>, AppError> {
    let choice = zvs_copilot::choose(models, &request.model)
        .ok_or_else(|| AppError::new(AppErrorCode::Conflict, MODEL_UNAVAILABLE))?;
    let requested = requested_effort(request);
    choice
        .effort(requested)
        .map(|effort| effort.and(requested))
        .map_err(|_| AppError::new(AppErrorCode::ValidationFailed, EFFORT_UNSUPPORTED))
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
        Ok(catalog(client.models().await?))
    }

    fn stream(&self, request: &GenerateRequest, cancel: &CancellationToken) -> DeltaStream {
        let identity = self.identity.clone();
        let request = request.clone();
        let cancel = cancel.clone();
        let start = async move {
            let mut client = Client::start(&cancel).await?;
            client.auth(Some(&identity)).await?;
            let models = if request.model == zvs_copilot::AUTO_MODEL {
                Vec::new()
            } else {
                client.models().await?
            };
            let effort = session_effort(&models, &request)?;
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

    fn listed(id: &str, policy: &str) -> zvs_copilot::Model {
        serde_json::from_value(json!({
            "id": id,
            "name": id,
            "capabilities": {"supports": {"reasoningEffort": true}, "limits": {}},
            "supportedReasoningEfforts": ["low", "high"],
            "policy": {"state": policy}
        }))
        .expect("model")
    }

    #[test]
    fn an_account_without_listed_models_still_offers_auto_selection() {
        let models = catalog(Vec::new());
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].external_id, zvs_copilot::AUTO_MODEL);
        assert_eq!(models[0].display_name, AUTO_MODEL_NAME);
        assert!(!models[0].capabilities.contains(&ModelCapability::Reasoning));

        let models = catalog(vec![
            listed("gpt", "enabled"),
            listed("off", "disabled"),
            listed(zvs_copilot::AUTO_MODEL, "enabled"),
        ]);
        assert_eq!(
            models
                .iter()
                .map(|model| model.external_id.as_str())
                .collect::<Vec<_>>(),
            ["gpt", zvs_copilot::AUTO_MODEL]
        );
    }

    fn turn(model: &str, effort: Option<ThinkingEffort>) -> GenerateRequest {
        let mut request = GenerateRequest::new(model, Vec::new());
        request.vendor.effort = effort;
        request
    }

    #[test]
    fn auto_needs_no_catalog_entry_and_keeps_the_default_effort() {
        assert_eq!(
            session_effort(&[], &turn(zvs_copilot::AUTO_MODEL, None)),
            Ok(None)
        );
        assert_eq!(
            session_effort(
                &[],
                &turn(zvs_copilot::AUTO_MODEL, Some(ThinkingEffort::Auto))
            ),
            Ok(None)
        );
        assert_eq!(
            session_effort(
                &[],
                &turn(zvs_copilot::AUTO_MODEL, Some(ThinkingEffort::Thinking))
            )
            .unwrap_err()
            .message,
            EFFORT_UNSUPPORTED
        );
        let models = [listed("gpt", "enabled"), listed("off", "disabled")];
        assert_eq!(
            session_effort(&models, &turn("gpt", Some(ThinkingEffort::Fast))),
            Ok(Some("low"))
        );
        assert_eq!(
            session_effort(&models, &turn("off", None))
                .unwrap_err()
                .message,
            MODEL_UNAVAILABLE
        );
    }

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
