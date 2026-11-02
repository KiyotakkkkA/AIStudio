mod common;
pub mod git;
pub mod mail;
pub mod oauth;
mod service;
#[cfg(test)]
mod stub;
pub mod telegram;
mod tools;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;
use zvs_contract::validate::{check, invalid};
use zvs_contract::{
    AppError, AppErrorCode, FieldDescriptor, FieldKind, IntegrationAccountDto,
    IntegrationActivityLevel, IntegrationCheckDto, IntegrationConfig, IntegrationKind,
    IntegrationToolDto, IntegrationTriggerDto, IntegrationValue, SecretId, Timestamp, Validate,
};

use crate::platform::external::UrlOpener;
use crate::services::crypto::Plaintext;
use crate::services::{SecretService, SecretUpdateInput};
use crate::workers::{WorkerContext, WorkerFault};

pub use service::{
    ACCOUNT_NOT_FOUND, Dispatched, EVENT_RETENTION_MS, IntegrationService,
    IntegrationServiceOptions, KIND_UNAVAILABLE, TriggerSink, UNKNOWN_TRIGGER, worker_key,
};
pub use tools::{
    APPROVAL_UNAVAILABLE, IntegrationCallTarget, IntegrationGateway, SharedGateway,
    account_tool_key,
};

pub const CREDENTIAL_NOT_A_SECRET: &str =
    "Учётные данные указываются только выбором секрета из хранилища секретов";
pub const SECRET_NOT_CONFIGURED: &str = "В настройках аккаунта не выбран секрет";
pub const FIELD_REQUIRED: &str = "Заполните обязательное поле";
pub const SIGN_IN_UNSUPPORTED: &str = "Для этой интеграции вход через браузер не используется";

/// Validation error that names the field, so the message stays clear wherever
/// it surfaces — the account form, the journal or a tool call in a dialog.
pub fn required_field(path: &str, label: &str) -> AppError {
    AppError::new(
        AppErrorCode::ValidationFailed,
        format!("{FIELD_REQUIRED} «{label}»."),
    )
    .with_detail("field", path.to_owned())
    .with_detail("rule", "required")
}

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
    pub per_call_approval: bool,
}

impl IntegrationToolSpec {
    pub const fn new(tool: IntegrationToolDto, input_schema: Value) -> Self {
        Self {
            tool,
            input_schema,
            per_call_approval: false,
        }
    }

    #[must_use]
    pub fn approved_per_call(mut self) -> Self {
        self.per_call_approval = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IntegrationApproval {
    pub kind: String,
    pub action: String,
    pub denied: String,
    pub fields: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IntegrationHandled {
    pub trigger: String,
    pub event_id: String,
    pub payload: Value,
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

    pub fn secret_id(&self, key: &str) -> Option<SecretId> {
        self.values.get(key).and_then(IntegrationValue::secret_id)
    }

    pub fn update_secret(&self, key: &str, value: Plaintext) -> Result<(), AppError> {
        let id = self.secret_id(key).ok_or_else(|| {
            AppError::new(AppErrorCode::SecretMissing, SECRET_NOT_CONFIGURED)
                .with_detail("field", key)
        })?;
        self.secrets.update(
            &id.to_string(),
            SecretUpdateInput {
                value: Some(value),
                ..SecretUpdateInput::default()
            },
        )?;
        Ok(())
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

    pub fn cursor(&self, key: &str) -> Result<Option<String>, AppError> {
        self.service.cursor(&self.account.id.to_string(), key)
    }

    pub fn set_cursor(&self, key: &str, value: &str) -> Result<(), AppError> {
        self.service
            .set_cursor(&self.account.id.to_string(), key, value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInPrompt {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_at: Timestamp,
}

pub struct SignInRequest<'a> {
    pub account: &'a IntegrationAccountDto,
    pub config: &'a IntegrationConfigView,
    pub opener: &'a UrlOpener,
    pub cancel: &'a CancellationToken,
    pub prompt: &'a (dyn Fn(&SignInPrompt) + Send + Sync),
}

pub struct SignedIn {
    pub field: String,
    pub value: Plaintext,
}

#[derive(Clone)]
pub struct CursorView {
    account_id: String,
    service: IntegrationService,
}

impl CursorView {
    pub(crate) const fn new(account_id: String, service: IntegrationService) -> Self {
        Self {
            account_id,
            service,
        }
    }

    pub fn get(&self, key: &str) -> Result<Option<String>, AppError> {
        self.service.cursor(&self.account_id, key)
    }

    pub fn set(&self, key: &str, value: &str) -> Result<(), AppError> {
        self.service.set_cursor(&self.account_id, key, value)
    }
}

pub struct IntegrationToolCall {
    pub account: IntegrationAccountDto,
    pub config: IntegrationConfigView,
    pub cursors: CursorView,
    pub tool: String,
    pub input: Map<String, Value>,
    pub approved: Option<Map<String, Value>>,
    pub cancel: CancellationToken,
}

#[async_trait]
pub trait IntegrationProvider: Send + Sync {
    fn kind(&self) -> IntegrationKind;

    fn label(&self) -> String;

    fn describe(&self) -> Vec<FieldDescriptor>;

    fn instructions(&self) -> String {
        String::new()
    }

    fn triggers(&self) -> Vec<IntegrationTriggerDto>;

    fn tools(&self) -> Vec<IntegrationToolSpec>;

    fn account_tools(&self, _config: &IntegrationConfig) -> Vec<IntegrationToolSpec> {
        self.tools()
    }

    async fn test(
        &self,
        config: &IntegrationConfigView,
    ) -> Result<Vec<IntegrationCheckDto>, AppError>;

    async fn start(&self, session: IntegrationSession) -> Result<(), WorkerFault>;

    async fn stop(&self, _account: &IntegrationAccountDto) {}

    async fn call_tool(&self, call: IntegrationToolCall) -> Result<Value, AppError>;

    async fn approval(
        &self,
        _call: &IntegrationToolCall,
    ) -> Result<Option<IntegrationApproval>, AppError> {
        Ok(None)
    }

    async fn handled(
        &self,
        _account: &IntegrationAccountDto,
        _config: &IntegrationConfigView,
        _event: &IntegrationHandled,
    ) -> Result<(), AppError> {
        Ok(())
    }

    async fn sign_in(&self, _request: SignInRequest<'_>) -> Result<SignedIn, AppError> {
        Err(AppError::new(
            AppErrorCode::NotImplemented,
            SIGN_IN_UNSUPPORTED,
        ))
    }
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
            return Err(required_field(
                &format!("config.{}", field.key),
                &field.label,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
