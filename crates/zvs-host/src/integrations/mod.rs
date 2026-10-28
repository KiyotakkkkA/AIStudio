mod service;
mod tools;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;
use zvs_contract::validate::{check, invalid};
use zvs_contract::{
    AppError, AppErrorCode, FieldDescriptor, FieldKind, IntegrationAccountDto,
    IntegrationActivityLevel, IntegrationCheckDto, IntegrationConfig, IntegrationKind,
    IntegrationToolDto, IntegrationTriggerDto, IntegrationValue, Validate,
};

use crate::services::SecretService;
use crate::services::crypto::Plaintext;
use crate::workers::{WorkerContext, WorkerFault};

pub use service::{
    ACCOUNT_NOT_FOUND, Dispatched, EVENT_RETENTION_MS, IntegrationService,
    IntegrationServiceOptions, KIND_UNAVAILABLE, TriggerSink, UNKNOWN_TRIGGER, worker_key,
};
pub use tools::{IntegrationCallTarget, IntegrationToolCaller, account_tool_key};

pub const CREDENTIAL_NOT_A_SECRET: &str =
    "Учётные данные указываются только выбором секрета из хранилища секретов";
pub const SECRET_NOT_CONFIGURED: &str = "В настройках интеграции не выбран секрет";
pub const FIELD_REQUIRED: &str = "Заполните обязательное поле";

const CREDENTIAL_HINTS: &[&str] = &[
    "password",
    "passwd",
    "token",
    "secret",
    "apikey",
    "privatekey",
    "credential",
];

#[derive(Debug, Clone, PartialEq)]
pub struct IntegrationToolSpec {
    pub tool: IntegrationToolDto,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IntegrationEvent {
    pub trigger: String,
    pub event_id: String,
    pub payload: Value,
}

impl IntegrationEvent {
    pub fn new(trigger: impl Into<String>, event_id: impl Into<String>, payload: Value) -> Self {
        Self {
            trigger: trigger.into(),
            event_id: event_id.into(),
            payload,
        }
    }
}

#[derive(Clone)]
pub struct IntegrationConfigView {
    values: IntegrationConfig,
    secrets: Arc<SecretService>,
}

impl IntegrationConfigView {
    pub const fn new(values: IntegrationConfig, secrets: Arc<SecretService>) -> Self {
        Self { values, secrets }
    }

    pub fn text(&self, key: &str) -> Option<&str> {
        match self.values.get(key) {
            Some(IntegrationValue::Text(value)) => Some(value),
            _ => None,
        }
    }

    pub fn number(&self, key: &str) -> Option<f64> {
        match self.values.get(key) {
            Some(IntegrationValue::Number(value)) => value.as_f64(),
            _ => None,
        }
    }

    pub fn flag(&self, key: &str) -> Option<bool> {
        match self.values.get(key) {
            Some(IntegrationValue::Boolean(value)) => Some(*value),
            _ => None,
        }
    }

    pub fn secret(&self, key: &str) -> Result<Plaintext, AppError> {
        let id = self
            .values
            .get(key)
            .and_then(IntegrationValue::secret_id)
            .ok_or_else(|| {
                AppError::new(AppErrorCode::SecretMissing, SECRET_NOT_CONFIGURED)
                    .with_detail("field", key)
            })?;
        self.secrets.resolve(&id.to_string())
    }
}

pub struct IntegrationSession {
    pub account: IntegrationAccountDto,
    pub config: IntegrationConfigView,
    pub worker: WorkerContext,
    service: IntegrationService,
}

impl IntegrationSession {
    pub fn cancellation(&self) -> &CancellationToken {
        self.worker.cancellation()
    }

    pub fn emit(&self, event: IntegrationEvent) -> Result<Dispatched, AppError> {
        self.service.dispatch(&self.account.id.to_string(), &event)
    }

    pub fn activity(&self, level: IntegrationActivityLevel, message: &str) {
        self.service
            .note(&self.account.id.to_string(), level, message);
    }
}

pub struct IntegrationToolCall {
    pub account: IntegrationAccountDto,
    pub config: IntegrationConfigView,
    pub tool: String,
    pub input: Map<String, Value>,
    pub cancel: CancellationToken,
}

#[async_trait]
pub trait IntegrationProvider: Send + Sync {
    fn kind(&self) -> IntegrationKind;

    fn label(&self) -> String;

    fn describe(&self) -> Vec<FieldDescriptor>;

    fn triggers(&self) -> Vec<IntegrationTriggerDto>;

    fn tools(&self) -> Vec<IntegrationToolSpec>;

    async fn test(
        &self,
        config: &IntegrationConfigView,
    ) -> Result<Vec<IntegrationCheckDto>, AppError>;

    async fn start(&self, session: IntegrationSession) -> Result<(), WorkerFault>;

    async fn stop(&self, _account: &IntegrationAccountDto) {}

    async fn call_tool(&self, call: IntegrationToolCall) -> Result<Value, AppError>;
}

fn looks_like_credential(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    CREDENTIAL_HINTS.iter().any(|hint| lowered.contains(hint))
}

pub fn check_provider(provider: &dyn IntegrationProvider) -> Result<(), AppError> {
    check::trimmed_name("label", &provider.label(), 200)?;
    let fields = provider.describe();
    for (index, field) in fields.iter().enumerate() {
        let path = format!("fields.{index}");
        zvs_contract::validate::nested(&path, field.validate())?;
        if field.kind != FieldKind::Secret && looks_like_credential(&field.key) {
            return Err(
                AppError::new(AppErrorCode::ValidationFailed, CREDENTIAL_NOT_A_SECRET)
                    .with_detail("field", format!("{path}.kind")),
            );
        }
        if fields[..index].iter().any(|other| other.key == field.key) {
            return Err(invalid(&format!("{path}.key"), "unique"));
        }
    }
    zvs_contract::validate::nested("triggers", provider.triggers().validate())?;
    let tools: Vec<IntegrationToolDto> =
        provider.tools().into_iter().map(|spec| spec.tool).collect();
    zvs_contract::validate::nested("tools", tools.validate())
}

fn kind_error(path: &str) -> AppError {
    invalid(path, "kind")
}

fn check_value(
    field: &FieldDescriptor,
    path: &str,
    value: &IntegrationValue,
) -> Result<(), AppError> {
    match (field.kind, value) {
        (FieldKind::Secret, IntegrationValue::Secret(_)) => Ok(()),
        (FieldKind::Secret, _) => Err(AppError::new(
            AppErrorCode::ValidationFailed,
            CREDENTIAL_NOT_A_SECRET,
        )
        .with_detail("field", path)
        .with_detail("rule", "secret")),
        (_, IntegrationValue::Secret(_)) => Err(kind_error(path)),
        (FieldKind::Text, IntegrationValue::Text(text)) => {
            if field.required {
                check::non_empty(path, text.trim())
            } else {
                Ok(())
            }
        }
        (FieldKind::Url, IntegrationValue::Text(text)) => check::url(path, text, 2048),
        (FieldKind::Number, IntegrationValue::Number(_))
        | (FieldKind::Boolean, IntegrationValue::Boolean(_)) => Ok(()),
        (FieldKind::Select, IntegrationValue::Text(text)) => field
            .options
            .iter()
            .flatten()
            .any(|option| &option.value == text)
            .then_some(())
            .ok_or_else(|| invalid(path, "enum")),
        _ => Err(kind_error(path)),
    }
}

pub fn check_config(
    fields: &[FieldDescriptor],
    config: &IntegrationConfig,
) -> Result<(), AppError> {
    for (key, value) in config {
        let path = format!("config.{key}");
        let field = fields
            .iter()
            .find(|field| &field.key == key)
            .ok_or_else(|| invalid(&path, "unknown"))?;
        check_value(field, &path, value)?;
    }
    for field in fields.iter().filter(|field| field.required) {
        if !config.contains_key(&field.key) {
            return Err(
                AppError::new(AppErrorCode::ValidationFailed, FIELD_REQUIRED)
                    .with_detail("field", format!("config.{}", field.key))
                    .with_detail("rule", "required"),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
