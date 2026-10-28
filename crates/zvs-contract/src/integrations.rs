use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Number;

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::{IntegrationId, SecretId};
use crate::primitives::Timestamp;
use crate::secrets::{FieldDescriptor, is_field_key};
use crate::tools::is_tool_key;
use crate::validate::{Validate, check, invalid, js_trim, nested, no_refinements};

wire_enum!(IntegrationKind {
    Mail => "mail",
    Telegram => "telegram",
    Git => "git",
});

wire_enum!(IntegrationStatus {
    Disabled => "disabled",
    Stopped => "stopped",
    Starting => "starting",
    Running => "running",
    Retrying => "retrying",
    Failed => "failed",
});

wire_enum!(IntegrationActivityLevel {
    Info => "info",
    Warn => "warn",
    Error => "error",
});

wire_enum!(WorkerStatus {
    Starting => "starting",
    Running => "running",
    Retrying => "retrying",
    Stopped => "stopped",
    Failed => "failed",
});

pub const INTEGRATION_NAME_MAX: usize = 120;
pub const INTEGRATION_LABEL_MAX: usize = 200;
pub const INTEGRATION_DESCRIPTION_MAX: usize = 2000;
pub const INTEGRATION_VALUE_MAX: usize = 4096;
pub const INTEGRATION_DETAIL_MAX: usize = 2000;
pub const INTEGRATION_ACTIVITY_MAX: usize = 1000;
pub const INTEGRATION_ACTIVITY_LIMIT_MAX: u64 = 200;
pub const INTEGRATION_CONFIG_FIELDS_MAX: usize = 64;
pub const WORKER_KEY_MAX: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntegrationSecretRef {
    pub secret_id: SecretId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IntegrationValue {
    Secret(IntegrationSecretRef),
    Boolean(bool),
    Number(Number),
    Text(String),
}

impl IntegrationValue {
    pub const fn secret(secret_id: SecretId) -> Self {
        Self::Secret(IntegrationSecretRef { secret_id })
    }

    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    pub const fn secret_id(&self) -> Option<SecretId> {
        match self {
            Self::Secret(reference) => Some(reference.secret_id),
            _ => None,
        }
    }
}

impl Validate for IntegrationValue {
    fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::Text(value) => check::max_len("value", value, INTEGRATION_VALUE_MAX),
            Self::Number(value) => value
                .as_f64()
                .map_or(Err(invalid("value", "finite")), |number| {
                    check::finite("value", number)
                }),
            Self::Secret(_) | Self::Boolean(_) => Ok(()),
        }
    }
}

pub type IntegrationConfig = BTreeMap<String, IntegrationValue>;

pub fn check_config(field: &str, config: &IntegrationConfig) -> Result<(), AppError> {
    if config.len() > INTEGRATION_CONFIG_FIELDS_MAX {
        return Err(invalid(field, "maxItems"));
    }
    for (key, value) in config {
        let path = format!("{field}.{key}");
        if !is_field_key(key) {
            return Err(invalid(&path, "pattern"));
        }
        nested(&path, value.validate())?;
    }
    Ok(())
}

pub fn config_secret_ids(config: &IntegrationConfig) -> Vec<SecretId> {
    let mut ids: Vec<SecretId> = config
        .values()
        .filter_map(IntegrationValue::secret_id)
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

pub fn check_worker_key(field: &str, value: &str) -> Result<(), AppError> {
    check::non_empty(field, value)?;
    check::max_len(field, value, WORKER_KEY_MAX)?;
    let mut characters = value.chars();
    let shaped = characters
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | ':' | '-' | '_')
        });
    if shaped {
        Ok(())
    } else {
        Err(invalid(field, "pattern"))
    }
}

fn check_optional_len(field: &str, value: Option<&String>, max: usize) -> Result<(), AppError> {
    value.map_or(Ok(()), |value| check::max_len(field, value, max))
}

fn check_optional_timestamp(field: &str, value: Option<Timestamp>) -> Result<(), AppError> {
    value.map_or(Ok(()), |value| check::timestamp(field, value))
}

fn check_capability_name(field: &str, value: &str) -> Result<(), AppError> {
    check::max_len(field, value, 128)?;
    if is_tool_key(value) {
        Ok(())
    } else {
        Err(invalid(field, "pattern"))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationCountersDto {
    pub events: u64,
    pub duplicates: u64,
    pub runs: u64,
    pub tool_calls: u64,
    pub errors: u64,
}

impl Validate for IntegrationCountersDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("events", self.events)?;
        check::safe_integer("duplicates", self.duplicates)?;
        check::safe_integer("runs", self.runs)?;
        check::safe_integer("toolCalls", self.tool_calls)?;
        check::safe_integer("errors", self.errors)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationAccountDto {
    pub id: IntegrationId,
    pub kind: IntegrationKind,
    pub name: String,
    pub enabled: bool,
    pub config: IntegrationConfig,
    pub status: IntegrationStatus,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub status_detail: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub last_activity_at: Option<Timestamp>,
    pub counters: IntegrationCountersDto,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Validate for IntegrationAccountDto {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("name", &self.name, INTEGRATION_NAME_MAX)?;
        check_config("config", &self.config)?;
        check_optional_len(
            "statusDetail",
            self.status_detail.as_ref(),
            INTEGRATION_DETAIL_MAX,
        )?;
        check_optional_timestamp("lastActivityAt", self.last_activity_at)?;
        nested("counters", self.counters.validate())?;
        check::timestamp("createdAt", self.created_at)?;
        check::timestamp("updatedAt", self.updated_at)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationTriggerDto {
    pub name: String,
    pub label: String,
    pub description: String,
}

impl Validate for IntegrationTriggerDto {
    fn validate(&self) -> Result<(), AppError> {
        check_capability_name("name", &self.name)?;
        check::trimmed_name("label", &self.label, INTEGRATION_LABEL_MAX)?;
        check::max_len(
            "description",
            &self.description,
            INTEGRATION_DESCRIPTION_MAX,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationToolDto {
    pub name: String,
    pub label: String,
    pub description: String,
    pub side_effect: bool,
}

impl Validate for IntegrationToolDto {
    fn validate(&self) -> Result<(), AppError> {
        check_capability_name("name", &self.name)?;
        check::trimmed_name("label", &self.label, INTEGRATION_LABEL_MAX)?;
        check::max_len(
            "description",
            &self.description,
            INTEGRATION_DESCRIPTION_MAX,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationAccountToolDto {
    #[serde(flatten)]
    pub tool: IntegrationToolDto,
    pub key: String,
}

impl Validate for IntegrationAccountToolDto {
    fn validate(&self) -> Result<(), AppError> {
        self.tool.validate()?;
        check_capability_name("key", &self.key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationAccountDetailDto {
    #[serde(flatten)]
    pub account: IntegrationAccountDto,
    pub triggers: Vec<IntegrationTriggerDto>,
    pub tools: Vec<IntegrationAccountToolDto>,
}

impl Validate for IntegrationAccountDetailDto {
    fn validate(&self) -> Result<(), AppError> {
        self.account.validate()?;
        nested("triggers", self.triggers.validate())?;
        nested("tools", self.tools.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationKindDto {
    pub kind: IntegrationKind,
    pub label: String,
    pub fields: Vec<FieldDescriptor>,
    pub triggers: Vec<IntegrationTriggerDto>,
    pub tools: Vec<IntegrationToolDto>,
}

impl Validate for IntegrationKindDto {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("label", &self.label, INTEGRATION_LABEL_MAX)?;
        nested("fields", self.fields.validate())?;
        nested("triggers", self.triggers.validate())?;
        nested("tools", self.tools.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationCheckDto {
    pub label: String,
    pub ok: bool,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub detail: Option<String>,
}

impl Validate for IntegrationCheckDto {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("label", &self.label, INTEGRATION_LABEL_MAX)?;
        check_optional_len("detail", self.detail.as_ref(), INTEGRATION_DETAIL_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationTestResultDto {
    pub ok: bool,
    pub latency_ms: u64,
    pub checks: Vec<IntegrationCheckDto>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub error: Option<String>,
}

impl Validate for IntegrationTestResultDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("latencyMs", self.latency_ms)?;
        nested("checks", self.checks.validate())?;
        check_optional_len("error", self.error.as_ref(), INTEGRATION_DETAIL_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationActivityDto {
    pub at: Timestamp,
    pub level: IntegrationActivityLevel,
    pub message: String,
}

impl Validate for IntegrationActivityDto {
    fn validate(&self) -> Result<(), AppError> {
        check::timestamp("at", self.at)?;
        check::non_empty("message", &self.message)?;
        check::max_len("message", &self.message, INTEGRATION_ACTIVITY_MAX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationRef {
    pub id: IntegrationId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationActivityInput {
    pub id: IntegrationId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub limit: Option<u64>,
}

impl IntegrationActivityInput {
    pub fn limit(&self) -> u64 {
        self.limit.unwrap_or(50)
    }
}

impl Validate for IntegrationActivityInput {
    fn validate(&self) -> Result<(), AppError> {
        self.limit.map_or(Ok(()), |limit| {
            check::int_range("limit", limit, 1, INTEGRATION_ACTIVITY_LIMIT_MAX)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateIntegrationInput {
    pub kind: IntegrationKind,
    pub name: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub config: Option<IntegrationConfig>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub enabled: Option<bool>,
}

impl CreateIntegrationInput {
    pub fn new(kind: IntegrationKind, name: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            config: None,
            enabled: None,
        }
    }

    pub fn name(&self) -> &str {
        js_trim(&self.name)
    }

    pub fn config(&self) -> IntegrationConfig {
        self.config.clone().unwrap_or_default()
    }

    pub fn enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}

impl Validate for CreateIntegrationInput {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("name", &self.name, INTEGRATION_NAME_MAX)?;
        self.config
            .as_ref()
            .map_or(Ok(()), |config| check_config("config", config))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateIntegrationInput {
    pub id: IntegrationId,
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
    pub config: Option<IntegrationConfig>,
}

impl UpdateIntegrationInput {
    pub const fn new(id: IntegrationId) -> Self {
        Self {
            id,
            name: None,
            config: None,
        }
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref().map(js_trim)
    }
}

impl Validate for UpdateIntegrationInput {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(name) = &self.name {
            check::trimmed_name("name", name, INTEGRATION_NAME_MAX)?;
        }
        self.config
            .as_ref()
            .map_or(Ok(()), |config| check_config("config", config))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationRemoveOutput {
    pub id: IntegrationId,
    pub removed: bool,
}

impl Validate for IntegrationRemoveOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::literal_true("removed", self.removed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerDto {
    pub key: String,
    pub label: String,
    pub status: WorkerStatus,
    pub restarts: u64,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub detail: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub last_error: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub started_at: Option<Timestamp>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub heartbeat_at: Option<Timestamp>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub retry_at: Option<Timestamp>,
}

impl Validate for WorkerDto {
    fn validate(&self) -> Result<(), AppError> {
        check_worker_key("key", &self.key)?;
        check::trimmed_name("label", &self.label, INTEGRATION_LABEL_MAX)?;
        check::safe_integer("restarts", self.restarts)?;
        check_optional_len("detail", self.detail.as_ref(), INTEGRATION_DETAIL_MAX)?;
        check_optional_len(
            "lastError",
            self.last_error.as_ref(),
            INTEGRATION_DETAIL_MAX,
        )?;
        check_optional_timestamp("startedAt", self.started_at)?;
        check_optional_timestamp("heartbeatAt", self.heartbeat_at)?;
        check_optional_timestamp("retryAt", self.retry_at)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRef {
    pub key: String,
}

impl Validate for WorkerRef {
    fn validate(&self) -> Result<(), AppError> {
        check_worker_key("key", &self.key)
    }
}

no_refinements!(IntegrationRef, IntegrationSecretRef);

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    const SECRET: &str = "0199aa11-1111-7111-8111-111111111111";

    #[test]
    fn a_config_value_is_a_secret_reference_or_a_plain_value() {
        let config: IntegrationConfig = serde_json::from_value(json!({
            "token": { "secretId": SECRET },
            "pollSeconds": 30,
            "markRead": false,
            "host": "imap.example.com",
        }))
        .expect("parses");
        assert_eq!(
            config["token"].secret_id().map(|id| id.to_string()),
            Some(SECRET.to_owned())
        );
        assert_eq!(config["host"], IntegrationValue::text("imap.example.com"));
        assert_eq!(config_secret_ids(&config).len(), 1);
    }

    #[test]
    fn a_secret_reference_carries_nothing_but_the_id() {
        assert!(
            serde_json::from_value::<IntegrationSecretRef>(
                json!({ "secretId": SECRET, "value": "hunter2" })
            )
            .is_err()
        );
        assert_eq!(
            serde_json::from_value::<IntegrationValue>(json!({ "value": "hunter2" })).ok(),
            None
        );
    }

    #[test]
    fn config_keys_follow_the_field_key_shape() {
        assert!(
            parse::<CreateIntegrationInput>(json!({
                "kind": "telegram",
                "name": "Бот",
                "config": { "Bad-Key": "x" },
            }))
            .is_err()
        );
        let draft = parse::<CreateIntegrationInput>(json!({
            "kind": "telegram",
            "name": "  Бот ",
        }))
        .expect("parses");
        assert_eq!(draft.name(), "Бот");
        assert!(draft.enabled());
        assert!(draft.config().is_empty());
    }

    #[test]
    fn worker_keys_are_lowercase_dotted_paths() {
        assert!(check_worker_key("key", "integration:0199aa11").is_ok());
        assert!(check_worker_key("key", "Integration").is_err());
        assert!(check_worker_key("key", "a b").is_err());
    }
}
