use serde::Deserialize;
use serde_json::{Map, Value};
use zvs_contract::{
    AppError, AppErrorCode, FieldDefault, FieldDescriptor, FieldKind, FieldOption,
    IntegrationCheckDto, IntegrationConfig, IntegrationToolDto, IntegrationValue,
};

use super::IntegrationConfigView;

pub const ACCESS: &str = "access";
pub const READ_ONLY: &str = "read";
pub const READ_WRITE: &str = "readWrite";
pub const READ_ONLY_LABEL: &str = "Только чтение";
pub const READ_WRITE_LABEL: &str = "Чтение и запись";
pub const ACCESS_LABEL: &str = "Доступ";
pub const UNKNOWN_TOOL: &str = "Интеграция не предоставляет такой инструмент";

pub fn field(key: &str, label: &str, kind: FieldKind, required: bool) -> FieldDescriptor {
    FieldDescriptor {
        key: key.to_owned(),
        label: label.to_owned(),
        kind,
        required,
        placeholder: None,
        help: None,
        options: None,
        default: None,
    }
}

pub fn option(value: &str, label: &str) -> FieldOption {
    FieldOption {
        value: value.to_owned(),
        label: label.to_owned(),
    }
}

pub fn select(key: &str, label: &str, options: Vec<FieldOption>, default: &str) -> FieldDescriptor {
    FieldDescriptor {
        options: Some(options),
        default: Some(FieldDefault::Text(default.to_owned())),
        ..field(key, label, FieldKind::Select, false)
    }
}

pub fn number(key: &str, label: &str, default: Option<u64>) -> FieldDescriptor {
    FieldDescriptor {
        default: default.map(|value| FieldDefault::Number(value.into())),
        ..field(key, label, FieldKind::Number, false)
    }
}

pub fn with_help(field: FieldDescriptor, help: &str) -> FieldDescriptor {
    FieldDescriptor {
        help: Some(help.to_owned()),
        ..field
    }
}

pub fn with_placeholder(field: FieldDescriptor, placeholder: &str) -> FieldDescriptor {
    FieldDescriptor {
        placeholder: Some(placeholder.to_owned()),
        ..field
    }
}

pub fn access_field(help: &str) -> FieldDescriptor {
    with_help(
        select(
            ACCESS,
            ACCESS_LABEL,
            vec![
                option(READ_ONLY, READ_ONLY_LABEL),
                option(READ_WRITE, READ_WRITE_LABEL),
            ],
            READ_ONLY,
        ),
        help,
    )
}

pub fn can_write(config: &IntegrationConfig) -> bool {
    matches!(config.get(ACCESS), Some(IntegrationValue::Text(value)) if value == READ_WRITE)
}

pub fn optional_text(config: &IntegrationConfigView, key: &str) -> Option<String> {
    config
        .text(key)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub fn whole(config: &IntegrationConfigView, key: &str, fallback: u64, min: u64, max: u64) -> u64 {
    config
        .number(key)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or(fallback, |value| value.round() as u64)
        .clamp(min, max)
}

pub fn list(raw: Option<&str>) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    for item in raw.unwrap_or_default().split([',', '\n', ' ']) {
        let item = item.trim();
        if !item.is_empty() && !items.iter().any(|known| known == item) {
            items.push(item.to_owned());
        }
    }
    items
}

pub fn parse_input<T: for<'de> Deserialize<'de>>(
    input: &Map<String, Value>,
) -> Result<T, AppError> {
    serde_json::from_value(Value::Object(input.clone()))
        .map_err(|error| AppError::new(AppErrorCode::ValidationFailed, error.to_string()))
}

pub fn tool(name: &str, label: &str, description: &str, side_effect: bool) -> IntegrationToolDto {
    IntegrationToolDto {
        name: name.to_owned(),
        label: label.to_owned(),
        description: description.to_owned(),
        side_effect,
    }
}

pub fn check(label: &str, outcome: Result<String, AppError>) -> IntegrationCheckDto {
    match outcome {
        Ok(detail) => IntegrationCheckDto {
            label: label.to_owned(),
            ok: true,
            detail: Some(detail),
        },
        Err(error) => IntegrationCheckDto {
            label: label.to_owned(),
            ok: false,
            detail: Some(error.message),
        },
    }
}

pub fn unknown_tool(tool: &str) -> AppError {
    AppError::new(AppErrorCode::NotFound, UNKNOWN_TOOL).with_detail("tool", tool)
}

pub fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut clipped: String = text.chars().take(max.saturating_sub(1)).collect();
    clipped.push('…');
    clipped
}
