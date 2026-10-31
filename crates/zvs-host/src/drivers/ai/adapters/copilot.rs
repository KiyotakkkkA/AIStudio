use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AdapterFamily, AppError, AppErrorCode, AuthMode, DeltaKind, ModelCapability, ProviderUsageDto,
    ProviderUsageWindowDto, ThinkingEffort, ToolMode,
};

use super::account_family::flatten_prompt;
use crate::drivers::ai::capabilities::{AdapterCapabilities, Honours};
use crate::drivers::ai::copilot::{Client, failure, protocol_error};
use crate::drivers::ai::ports::{
    DeltaStream, DiscoveredModel, FinishReason, GenerateRequest, GenerateResult, SharedTextDriver,
    TextDelta, TextGenerationDriver, TokenUsage, TurnUsage,
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

const USAGE_SOURCE: &str = "copilot";
const QUOTA_TIMEOUT: Duration = Duration::from_secs(15);
pub const QUOTA_PREMIUM_LABEL: &str = "Премиум-запросы";
pub const QUOTA_CHAT_LABEL: &str = "Запросы в чате";
pub const QUOTA_UNLIMITED: &str = "Без ограничений";
pub const QUOTA_PENDING: &str = "Copilot передаёт остаток лимита вместе с ответом.";
pub const QUOTA_OVERAGE: &str = "Запросов сверх лимита";

/// The latest `quotaSnapshots` per identity, from the account quota call or a turn.
static QUOTAS: LazyLock<Mutex<HashMap<String, (Value, i64)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

fn remember_quota(identity: &str, snapshots: &Value, at: i64) {
    if snapshots.as_object().is_some_and(|map| !map.is_empty()) {
        QUOTAS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(identity.to_owned(), (snapshots.clone(), at));
    }
}

fn remembered_quota(identity: &str) -> Option<(Value, i64)> {
    QUOTAS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(identity)
        .cloned()
}

fn count(value: f64) -> String {
    let rounded = value.round();
    if rounded >= 1e15 {
        return format!("{rounded:.0}");
    }
    let digits = format!("{rounded:.0}");
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push('\u{a0}');
        }
        grouped.push(digit);
    }
    grouped
}

fn usage_report(snapshots: &Value, checked_at: i64) -> ProviderUsageDto {
    let quotas = zvs_copilot::quota_snapshots(snapshots);
    let windows: Vec<ProviderUsageWindowDto> = quotas
        .iter()
        .filter(|quota| !quota.unlimited)
        .map(|quota| ProviderUsageWindowDto {
            label: format!(
                "{} · {} / {}",
                if quota.kind == zvs_copilot::QUOTA_PREMIUM {
                    QUOTA_PREMIUM_LABEL
                } else {
                    QUOTA_CHAT_LABEL
                },
                count(quota.used),
                count(quota.entitlement)
            ),
            used_percent: 100.0 - quota.remaining_percent,
            resets_at: quota.resets_at,
        })
        .collect();
    let overage: f64 = quotas.iter().map(|quota| quota.overage).sum();
    let detail = if overage > 0.0 {
        Some(format!("{QUOTA_OVERAGE}: {}", count(overage)))
    } else if windows.is_empty() && !quotas.is_empty() {
        Some(QUOTA_UNLIMITED.to_owned())
    } else if quotas.is_empty() {
        Some(QUOTA_PENDING.to_owned())
    } else {
        None
    };
    ProviderUsageDto {
        source: USAGE_SOURCE.to_owned(),
        windows,
        detail,
        checked_at,
    }
}

fn tokens(value: &Value) -> Option<i64> {
    value.as_i64().filter(|count| *count >= 0)
}

fn call_usage(data: &Value) -> TurnUsage {
    TurnUsage {
        input_tokens: tokens(&data["inputTokens"]),
        output_tokens: tokens(&data["outputTokens"]),
        context_window: None,
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
        Some("assistant.usage") => Ok(TurnEvent::Delta(TextDelta::Usage(call_usage(
            &event["data"],
        )))),
        Some("session.usage_info") => Ok(
            match tokens(&event["data"]["tokenLimit"]).filter(|limit| *limit > 0) {
                Some(limit) => TurnEvent::Delta(TextDelta::Usage(TurnUsage {
                    context_window: Some(limit),
                    ..TurnUsage::default()
                })),
                None => TurnEvent::Ignore,
            },
        ),
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

    async fn usage(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Option<ProviderUsageDto>, AppError> {
        let live = tokio::time::timeout(QUOTA_TIMEOUT, async {
            let mut client = Client::start(cancel).await?;
            client.auth(Some(&self.identity)).await?;
            client.call("account.getQuota", json!({})).await
        })
        .await;
        let now = now_millis();
        if let Ok(Ok(result)) = &live {
            remember_quota(&self.identity, &result["quotaSnapshots"], now);
        }
        let (snapshots, checked_at) =
            remembered_quota(&self.identity).unwrap_or((Value::Null, now));
        Ok(Some(usage_report(&snapshots, checked_at)))
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
            Ok::<_, AppError>((client, session, identity))
        };
        futures::stream::once(start)
            .flat_map(|result| {
                let state = match result {
                    Ok(state) => Ok(state),
                    Err(error) => Err(Some(error)),
                };
                futures::stream::unfold(Some(state), |state| async move {
                    let state = state?;
                    let (mut client, session, identity) = match state {
                        Ok(state) => state,
                        Err(mut error) => return error.take().map(|error| (Err(error), None)),
                    };
                    loop {
                        match client.event(&session).await.and_then(|event| {
                            if event["type"] == "assistant.usage" {
                                remember_quota(
                                    &identity,
                                    &event["data"]["quotaSnapshots"],
                                    now_millis(),
                                );
                            }
                            decode(&event)
                        }) {
                            Ok(TurnEvent::Delta(delta)) => {
                                return Some((Ok(delta), Some(Ok((client, session, identity)))));
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
        let mut usage = TurnUsage::default();
        while let Some(delta) = stream.next().await {
            match delta? {
                TextDelta::Content {
                    text: delta,
                    kind: DeltaKind::Text,
                } => text.push_str(&delta),
                TextDelta::Content {
                    text: delta,
                    kind: DeltaKind::Reasoning,
                } => reasoning.push_str(&delta),
                TextDelta::Usage(reported) => usage.merge(reported),
                TextDelta::ToolCall(_) | TextDelta::WebSources(_) => {}
            }
        }
        Ok(GenerateResult {
            model: request.model.clone(),
            text,
            reasoning: (!reasoning.is_empty()).then_some(reasoning),
            finish_reason: FinishReason::Stop,
            usage: usage.counted().then(|| TokenUsage {
                prompt_tokens: usage.input_tokens,
                completion_tokens: usage.output_tokens,
                total_tokens: usage
                    .input_tokens
                    .zip(usage.output_tokens)
                    .map(|(input, output)| input.saturating_add(output)),
            }),
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

    fn usage_of(event: &Value) -> TurnUsage {
        match decode(event).expect("decodes") {
            TurnEvent::Delta(TextDelta::Usage(usage)) => usage,
            _ => panic!("a usage delta"),
        }
    }

    #[test]
    fn call_usage_reports_tokens_and_the_window_comes_from_usage_info() {
        let usage = usage_of(&json!({"type":"assistant.usage", "data":{
            "model":"gpt-6.1-sol", "isAuto":true, "inputTokens":1200, "outputTokens":300}}));
        assert_eq!(usage.input_tokens, Some(1200));
        assert_eq!(usage.output_tokens, Some(300));
        assert_eq!(usage.context_window, None);

        let window = usage_of(&json!({"type":"session.usage_info", "data":{
            "currentTokens":900, "tokenLimit":128000, "messagesLength":2}}));
        assert_eq!(window.context_window, Some(128_000));
        assert!(!window.counted());
        assert!(matches!(
            decode(&json!({"type":"session.usage_info", "data":{"tokenLimit":0}})).unwrap(),
            TurnEvent::Ignore
        ));
    }

    #[test]
    fn quota_snapshots_become_provider_limit_windows() {
        let report = usage_report(
            &json!({
                "chat": {"isUnlimitedEntitlement": true, "entitlementRequests": -1,
                    "usedRequests": 0, "remainingPercentage": 100, "overage": 0},
                "premium_interactions": {"isUnlimitedEntitlement": false,
                    "entitlementRequests": 1500, "usedRequests": 1200, "remainingPercentage": 20,
                    "overage": 0, "resetDate": "2026-11-01T00:00:00Z"}
            }),
            7,
        );
        assert_eq!(report.source, USAGE_SOURCE);
        assert_eq!(report.checked_at, 7);
        assert_eq!(
            report.windows,
            [ProviderUsageWindowDto {
                label: format!("{QUOTA_PREMIUM_LABEL} · 1\u{a0}200 / 1\u{a0}500"),
                used_percent: 80.0,
                resets_at: Some(1_793_491_200_000),
            }]
        );
        assert_eq!(report.detail, None);
        zvs_contract::Validate::validate(&report).expect("a valid report");

        let unlimited = usage_report(
            &json!({"chat": {"isUnlimitedEntitlement": true, "entitlementRequests": -1}}),
            1,
        );
        assert!(unlimited.windows.is_empty());
        assert_eq!(unlimited.detail.as_deref(), Some(QUOTA_UNLIMITED));

        let over = usage_report(
            &json!({"premium_interactions": {"entitlementRequests": 50, "usedRequests": 50,
                "remainingPercentage": 0, "overage": 3}}),
            1,
        );
        assert_eq!(over.windows[0].used_percent, 100.0);
        assert_eq!(over.detail, Some(format!("{QUOTA_OVERAGE}: 3")));
        assert_eq!(
            usage_report(&Value::Null, 1).detail.as_deref(),
            Some(QUOTA_PENDING)
        );
    }

    #[test]
    fn a_turn_remembers_the_latest_quota_for_its_identity() {
        let identity = "github.com/quota-test";
        assert!(remembered_quota(identity).is_none());
        remember_quota(identity, &json!({}), 1);
        assert!(remembered_quota(identity).is_none());
        remember_quota(identity, &json!({"chat": {"entitlementRequests": 10}}), 2);
        assert_eq!(remembered_quota(identity).map(|(_, at)| at), Some(2));
    }

    #[test]
    fn usage_from_several_calls_adds_up_and_keeps_the_latest_window() {
        let mut total = TurnUsage::default();
        total.merge(TurnUsage {
            input_tokens: Some(100),
            output_tokens: Some(10),
            context_window: Some(64_000),
        });
        total.merge(TurnUsage {
            input_tokens: Some(150),
            output_tokens: None,
            context_window: None,
        });
        assert_eq!(total.input_tokens, Some(250));
        assert_eq!(total.output_tokens, Some(10));
        assert_eq!(total.context_window, Some(64_000));
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
