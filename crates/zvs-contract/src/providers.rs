use serde::{Deserialize, Serialize};

use crate::ai::{AdapterCapabilitiesDto, AdapterFamily, AuthMode};
use crate::enums::wire_enum;
use crate::errors::{AppError, AppErrorCode};
use crate::ids::{AccountId, ModelId, ProviderId, SecretId};
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, nested, no_refinements};

wire_enum!(ProviderKind {
    Ollama => "ollama",
    Openrouter => "openrouter",
    Mistral => "mistral",
    OpenaiCompatible => "openai-compatible",
});

wire_enum!(ProviderCapability {
    Text => "text",
    Embedding => "embedding",
    Image => "image",
});

wire_enum!(ProviderStatus {
    Unknown => "unknown",
    Ok => "ok",
    Degraded => "degraded",
    Failed => "failed",
    NeedsRelink => "needs-relink",
});

wire_enum!(ModelCapability {
    Tools => "tools",
    Vision => "vision",
    Streaming => "streaming",
    Reasoning => "reasoning",
    AdaptiveReasoning => "adaptive-reasoning",
    Code => "code",
});

pub const PROVIDER_NAME_MAX: usize = 128;
pub const BASE_URL_MAX: usize = 2048;

fn check_name(field: &str, name: &str) -> Result<(), AppError> {
    check::trimmed_name(field, name, PROVIDER_NAME_MAX)
}

fn check_base_url(field: &str, base_url: &str) -> Result<(), AppError> {
    check::url(field, base_url, BASE_URL_MAX)
}

fn check_capabilities(capabilities: &[ProviderCapability]) -> Result<(), AppError> {
    if capabilities.is_empty() {
        Err(invalid("capabilities", "minItems"))
    } else {
        Ok(())
    }
}

fn check_optional_count(field: &str, value: Option<u64>) -> Result<(), AppError> {
    value.map_or(Ok(()), |value| check::safe_integer(field, value))
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSettingsDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub selected_model_ids: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub temperature: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub top_k: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub top_p: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub max_output_tokens: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub timeout_seconds: Option<u64>,
}

impl Validate for ProviderSettingsDto {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(ids) = &self.selected_model_ids {
            ids.iter().enumerate().try_for_each(|(index, id)| {
                check::non_empty(&format!("selectedModelIds.{index}"), id)
            })?;
        }
        if let Some(temperature) = self.temperature {
            check::number_range("temperature", temperature, 0.0, 2.0)?;
        }
        if let Some(top_k) = self.top_k {
            check::int_range("topK", top_k, 1, 4096)?;
        }
        if let Some(top_p) = self.top_p {
            check::number_range("topP", top_p, 0.0, 1.0)?;
        }
        if let Some(tokens) = self.max_output_tokens {
            check::int_range("maxOutputTokens", tokens, 1, 1_000_000)?;
        }
        if let Some(seconds) = self.timeout_seconds {
            check::int_range("timeoutSeconds", seconds, 1, 600)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredModelDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::optional_nullable"
    )]
    pub is_free: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::optional_nullable"
    )]
    pub no_training: Option<Option<bool>>,
    pub external_id: String,
    pub display_name: String,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub family: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub context_window: Option<u64>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub max_output: Option<u64>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub size_bytes: Option<u64>,
    pub capabilities: Vec<ModelCapability>,
}

fn check_model_shape(
    external_id: &str,
    context_window: Option<u64>,
    max_output: Option<u64>,
    size_bytes: Option<u64>,
) -> Result<(), AppError> {
    check::non_empty("externalId", external_id)?;
    check_optional_count("contextWindow", context_window)?;
    check_optional_count("maxOutput", max_output)?;
    check_optional_count("sizeBytes", size_bytes)
}

impl Validate for DiscoveredModelDto {
    fn validate(&self) -> Result<(), AppError> {
        check_model_shape(
            &self.external_id,
            self.context_window,
            self.max_output,
            self.size_bytes,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::optional_nullable"
    )]
    pub is_free: Option<Option<bool>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::optional_nullable"
    )]
    pub no_training: Option<Option<bool>>,
    pub id: ModelId,
    pub provider_id: ProviderId,
    pub external_id: String,
    pub display_name: String,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub family: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub context_window: Option<u64>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub max_output: Option<u64>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub size_bytes: Option<u64>,
    pub capabilities: Vec<ModelCapability>,
    pub available: bool,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub unavailable_reason: Option<String>,
    pub discovered_at: Timestamp,
}

impl Validate for ModelDto {
    fn validate(&self) -> Result<(), AppError> {
        check_model_shape(
            &self.external_id,
            self.context_window,
            self.max_output,
            self.size_bytes,
        )?;
        check::timestamp("discoveredAt", self.discovered_at)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummaryDto {
    pub id: ProviderId,
    pub kind: ProviderKind,
    pub adapter: AdapterFamily,
    pub auth_mode: AuthMode,
    pub name: String,
    pub capabilities: Vec<ProviderCapability>,
    pub enabled: bool,
    pub status: ProviderStatus,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub status_detail: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub last_probe_at: Option<Timestamp>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub last_latency_ms: Option<u64>,
    pub model_count: u64,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub default_model_id: Option<ModelId>,
    pub updated_at: Timestamp,
}

impl Validate for ProviderSummaryDto {
    fn validate(&self) -> Result<(), AppError> {
        check_name("name", &self.name)?;
        if let Some(at) = self.last_probe_at {
            check::timestamp("lastProbeAt", at)?;
        }
        check_optional_count("lastLatencyMs", self.last_latency_ms)?;
        check::safe_integer("modelCount", self.model_count)?;
        check::timestamp("updatedAt", self.updated_at)
    }
}

pub type ProvidersListOutput = Vec<ProviderSummaryDto>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDto {
    #[serde(flatten)]
    pub summary: ProviderSummaryDto,
    pub base_url: String,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub secret_id: Option<SecretId>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub secret_name: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub account_id: Option<AccountId>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub account_label: Option<String>,
    pub settings: ProviderSettingsDto,
    pub adapter_capabilities: AdapterCapabilitiesDto,
    pub models: Vec<ModelDto>,
}

impl Validate for ProviderDto {
    fn validate(&self) -> Result<(), AppError> {
        self.summary.validate()?;
        nested("settings", self.settings.validate())?;
        nested("models", self.models.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConnectionInput {
    pub kind: ProviderKind,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub adapter: Option<AdapterFamily>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub auth_mode: Option<AuthMode>,
    pub base_url: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::optional_nullable"
    )]
    pub secret_id: Option<Option<SecretId>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::optional_nullable"
    )]
    pub account_id: Option<Option<AccountId>>,
    pub capabilities: Vec<ProviderCapability>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub settings: Option<ProviderSettingsDto>,
}

impl ProviderConnectionInput {
    pub fn adapter(&self) -> AdapterFamily {
        self.adapter.unwrap_or(AdapterFamily::OpenaiCompatible)
    }

    pub fn auth_mode(&self) -> AuthMode {
        self.auth_mode.unwrap_or(AuthMode::Api)
    }

    pub fn secret_id(&self) -> Option<SecretId> {
        self.secret_id.flatten()
    }

    pub fn account_id(&self) -> Option<AccountId> {
        self.account_id.flatten()
    }

    pub fn settings(&self) -> ProviderSettingsDto {
        self.settings.clone().unwrap_or_default()
    }
}

impl Validate for ProviderConnectionInput {
    fn validate(&self) -> Result<(), AppError> {
        check_base_url("baseUrl", &self.base_url)?;
        check_capabilities(&self.capabilities)?;
        nested("settings", self.settings.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProviderInput {
    #[serde(flatten)]
    pub connection: ProviderConnectionInput,
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub enabled: Option<bool>,
}

impl CreateProviderInput {
    pub fn enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}

impl Validate for CreateProviderInput {
    fn validate(&self) -> Result<(), AppError> {
        self.connection.validate()?;
        check_name("name", &self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProviderInput {
    pub id: ProviderId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub name: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub adapter: Option<AdapterFamily>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub auth_mode: Option<AuthMode>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub base_url: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::optional_nullable"
    )]
    pub secret_id: Option<Option<SecretId>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::optional_nullable"
    )]
    pub account_id: Option<Option<AccountId>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub capabilities: Option<Vec<ProviderCapability>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub settings: Option<ProviderSettingsDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub enabled: Option<bool>,
}

impl UpdateProviderInput {
    pub const fn new(id: ProviderId) -> Self {
        Self {
            id,
            name: None,
            adapter: None,
            auth_mode: None,
            base_url: None,
            secret_id: None,
            account_id: None,
            capabilities: None,
            settings: None,
            enabled: None,
        }
    }
}

impl Validate for UpdateProviderInput {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(name) = &self.name {
            check_name("name", name)?;
        }
        if let Some(base_url) = &self.base_url {
            check_base_url("baseUrl", base_url)?;
        }
        if let Some(capabilities) = &self.capabilities {
            check_capabilities(capabilities)?;
        }
        nested("settings", self.settings.validate())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderListFilter {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub capability: Option<ProviderCapability>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRef {
    pub id: ProviderId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersGetInput {
    pub id: ProviderId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub selected_only: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetDefaultModelInput {
    pub id: ProviderId,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub model_id: Option<ModelId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersRemoveOutput {
    pub id: ProviderId,
    pub removed: bool,
}

impl Validate for ProvidersRemoveOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::literal_true("removed", self.removed)
    }
}

no_refinements!(
    ProviderListFilter,
    ProviderRef,
    ProvidersGetInput,
    SetDefaultModelInput
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum ProbeOutcomeDto {
    Ok {
        latency_ms: u64,
        models: Vec<DiscoveredModelDto>,
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
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "crate::wire::present"
        )]
        retry_after: Option<u64>,
    },
    Error {
        code: AppErrorCode,
        detail: String,
    },
}

impl ProbeOutcomeDto {
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
}

impl Validate for ProbeOutcomeDto {
    fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::Ok {
                latency_ms, models, ..
            } => {
                check::safe_integer("latencyMs", *latency_ms)?;
                nested("models", models.validate())
            }
            Self::OkEmpty { latency_ms } => check::safe_integer("latencyMs", *latency_ms),
            Self::RateLimited { retry_after } => check_optional_count("retryAfter", *retry_after),
            Self::AuthFailed
            | Self::AccountNotLinked
            | Self::SessionExpired
            | Self::Unreachable { .. }
            | Self::Error { .. } => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProbeRequest {
    Saved(ProviderRef),
    Draft { draft: ProviderConnectionInput },
}

impl Validate for ProbeRequest {
    fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::Saved(_) => Ok(()),
            Self::Draft { draft } => nested("draft", draft.validate()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResultDto {
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub provider_id: Option<ProviderId>,
    pub outcome: ProbeOutcomeDto,
    pub status: ProviderStatus,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub status_detail: Option<String>,
    pub checked_at: Timestamp,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub provider: Option<ProviderDto>,
}

impl Validate for ProbeResultDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("outcome", self.outcome.validate())?;
        check::timestamp("checkedAt", self.checked_at)?;
        nested("provider", self.provider.validate())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn provider_enums_round_trip_through_their_wire_names() {
        for (value, wire) in [
            (
                serde_json::to_value(ProviderKind::OpenaiCompatible),
                "openai-compatible",
            ),
            (
                serde_json::to_value(ProviderStatus::NeedsRelink),
                "needs-relink",
            ),
            (
                serde_json::to_value(ProviderCapability::Embedding),
                "embedding",
            ),
            (
                serde_json::to_value(ModelCapability::Reasoning),
                "reasoning",
            ),
        ] {
            assert_eq!(value.expect("serialises"), json!(wire));
        }
        assert_eq!(
            serde_json::from_value::<Vec<ModelCapability>>(json!(["tools", "vision"]))
                .expect("parses"),
            [ModelCapability::Tools, ModelCapability::Vision]
        );
        assert_eq!(
            ProviderKind::parse("openrouter"),
            Some(ProviderKind::Openrouter)
        );
        assert_eq!(ProviderStatus::parse("broken"), None);
    }
}
