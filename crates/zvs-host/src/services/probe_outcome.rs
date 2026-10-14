use serde_json::Value;
use zvs_contract::{AppError, AppErrorCode, DiscoveredModelDto, ErrorDetails, ProbeOutcomeDto};

use crate::drivers::ai::ports::DiscoveredModel;
use crate::drivers::ai::registry::ACCOUNT_NOT_LINKED;

pub const PROBE_FAILED_DETAIL: &str = "Не удалось выполнить проверку";
pub const UNREACHABLE_DETAIL: &str = "Провайдер недоступен или не ответил вовремя";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    Ok {
        latency_ms: u64,
        models: Vec<DiscoveredModel>,
        live: bool,
    },
    OkEmpty {
        latency_ms: u64,
    },
    AuthFailed,
    AccountNotLinked,
    SessionExpired,
    Unreachable {
        detail: String,
    },
    RateLimited {
        retry_after: Option<u64>,
    },
    Error {
        code: AppErrorCode,
        detail: String,
    },
}

impl ProbeOutcome {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Ok { .. } => "ok",
            Self::OkEmpty { .. } => "ok-empty",
            Self::AuthFailed => "auth-failed",
            Self::AccountNotLinked => "account-not-linked",
            Self::SessionExpired => "session-expired",
            Self::Unreachable { .. } => "unreachable",
            Self::RateLimited { .. } => "rate-limited",
            Self::Error { .. } => "error",
        }
    }

    pub const fn succeeded(&self) -> bool {
        matches!(self, Self::Ok { .. } | Self::OkEmpty { .. })
    }

    pub const fn latency_ms(&self) -> Option<u64> {
        match self {
            Self::Ok { latency_ms, .. } | Self::OkEmpty { latency_ms } => Some(*latency_ms),
            _ => None,
        }
    }

    pub fn models(&self) -> &[DiscoveredModel] {
        match self {
            Self::Ok { models, .. } => models,
            _ => &[],
        }
    }

    pub fn to_dto(&self) -> ProbeOutcomeDto {
        match self {
            Self::Ok {
                latency_ms,
                models,
                live,
            } => ProbeOutcomeDto::Ok {
                latency_ms: *latency_ms,
                models: models.iter().map(discovered_model_dto).collect(),
                live: *live,
            },
            Self::OkEmpty { latency_ms } => ProbeOutcomeDto::OkEmpty {
                latency_ms: *latency_ms,
            },
            Self::AuthFailed => ProbeOutcomeDto::AuthFailed,
            Self::AccountNotLinked => ProbeOutcomeDto::AccountNotLinked,
            Self::SessionExpired => ProbeOutcomeDto::SessionExpired,
            Self::Unreachable { detail } => ProbeOutcomeDto::Unreachable {
                detail: detail.clone(),
            },
            Self::RateLimited { retry_after } => ProbeOutcomeDto::RateLimited {
                retry_after: *retry_after,
            },
            Self::Error { code, detail } => ProbeOutcomeDto::Error {
                code: *code,
                detail: detail.clone(),
            },
        }
    }
}

pub fn non_negative(value: Option<i64>) -> Option<u64> {
    value.and_then(|value| u64::try_from(value).ok())
}

pub fn discovered_model_dto(model: &DiscoveredModel) -> DiscoveredModelDto {
    DiscoveredModelDto {
        is_free: Some(model.is_free),
        no_training: Some(model.no_training),
        external_id: model.external_id.clone(),
        display_name: model.display_name.clone(),
        family: model.family.clone(),
        context_window: non_negative(model.context_window),
        max_output: non_negative(model.max_output),
        size_bytes: non_negative(model.size_bytes),
        capabilities: model.capabilities.clone(),
    }
}

pub fn discovery_outcome(
    latency_ms: u64,
    models: Vec<DiscoveredModel>,
    live: bool,
) -> ProbeOutcome {
    if models.is_empty() {
        ProbeOutcome::OkEmpty { latency_ms }
    } else {
        ProbeOutcome::Ok {
            latency_ms,
            models,
            live,
        }
    }
}

pub fn failure_outcome(error: &AppError) -> ProbeOutcome {
    match error.code {
        AppErrorCode::ProviderAuthFailed => ProbeOutcome::AuthFailed,
        AppErrorCode::ProviderSessionExpired if not_linked(error.details.as_ref()) => {
            ProbeOutcome::AccountNotLinked
        }
        AppErrorCode::ProviderSessionExpired => ProbeOutcome::SessionExpired,
        AppErrorCode::ProviderUnreachable => ProbeOutcome::Unreachable {
            detail: UNREACHABLE_DETAIL.to_owned(),
        },
        AppErrorCode::RateLimited => ProbeOutcome::RateLimited {
            retry_after: retry_after_of(error.details.as_ref()),
        },
        code => ProbeOutcome::Error {
            code,
            detail: PROBE_FAILED_DETAIL.to_owned(),
        },
    }
}

fn not_linked(details: Option<&ErrorDetails>) -> bool {
    details
        .and_then(|details| details.get("reason"))
        .and_then(Value::as_str)
        == Some(ACCOUNT_NOT_LINKED)
}

fn retry_after_of(details: Option<&ErrorDetails>) -> Option<u64> {
    details
        .and_then(|details| details.get("retryAfter"))
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| (value + 0.5).floor() as u64)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn error(code: AppErrorCode, details: Value) -> AppError {
        let mut error = AppError::new(code, "private payload");
        if let Value::Object(details) = details {
            error = error.with_details(details);
        }
        error
    }

    #[test]
    fn each_failure_code_maps_to_its_outcome_without_the_vendor_message() {
        for (code, details, expected) in [
            (
                AppErrorCode::ProviderAuthFailed,
                json!({}),
                ProbeOutcome::AuthFailed,
            ),
            (
                AppErrorCode::ProviderSessionExpired,
                json!({}),
                ProbeOutcome::SessionExpired,
            ),
            (
                AppErrorCode::ProviderSessionExpired,
                json!({ "reason": "account-not-linked" }),
                ProbeOutcome::AccountNotLinked,
            ),
            (
                AppErrorCode::ProviderUnreachable,
                json!({}),
                ProbeOutcome::Unreachable {
                    detail: UNREACHABLE_DETAIL.to_owned(),
                },
            ),
            (
                AppErrorCode::RateLimited,
                json!({ "retryAfter": 12 }),
                ProbeOutcome::RateLimited {
                    retry_after: Some(12),
                },
            ),
            (
                AppErrorCode::RateLimited,
                json!({ "retryAfter": 2.5 }),
                ProbeOutcome::RateLimited {
                    retry_after: Some(3),
                },
            ),
            (
                AppErrorCode::RateLimited,
                json!({ "retryAfter": -1 }),
                ProbeOutcome::RateLimited { retry_after: None },
            ),
            (
                AppErrorCode::RateLimited,
                json!({ "retryAfter": "soon" }),
                ProbeOutcome::RateLimited { retry_after: None },
            ),
            (
                AppErrorCode::Unknown,
                json!({}),
                ProbeOutcome::Error {
                    code: AppErrorCode::Unknown,
                    detail: PROBE_FAILED_DETAIL.to_owned(),
                },
            ),
            (
                AppErrorCode::SecretMissing,
                json!({}),
                ProbeOutcome::Error {
                    code: AppErrorCode::SecretMissing,
                    detail: PROBE_FAILED_DETAIL.to_owned(),
                },
            ),
        ] {
            let outcome = failure_outcome(&error(code, details));
            assert_eq!(outcome, expected);
            assert!(!outcome.succeeded());
            assert!(
                !serde_json::to_string(&outcome.to_dto())
                    .expect("serialises")
                    .contains("private payload")
            );
        }
    }

    #[test]
    fn discovery_with_no_models_is_ok_empty_and_models_keep_their_null_flags() {
        assert_eq!(
            discovery_outcome(7, Vec::new(), true),
            ProbeOutcome::OkEmpty { latency_ms: 7 }
        );
        let outcome = discovery_outcome(3, vec![DiscoveredModel::named("m")], false);
        assert!(outcome.succeeded());
        assert_eq!(outcome.latency_ms(), Some(3));
        assert_eq!(
            serde_json::to_value(outcome.to_dto()).expect("serialises"),
            json!({
                "kind": "ok",
                "latencyMs": 3,
                "models": [{
                    "isFree": null,
                    "noTraining": null,
                    "externalId": "m",
                    "displayName": "m",
                    "family": null,
                    "contextWindow": null,
                    "maxOutput": null,
                    "sizeBytes": null,
                    "capabilities": [],
                }],
                "live": false,
            })
        );
        assert_eq!(ProbeOutcome::AuthFailed.latency_ms(), None);
        assert!(ProbeOutcome::AuthFailed.models().is_empty());
    }
}
