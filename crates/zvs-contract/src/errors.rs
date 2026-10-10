use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub type ErrorDetails = Map<String, Value>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppErrorCode {
    Unknown,
    ValidationFailed,
    NotFound,
    Conflict,
    PermissionDenied,
    SecretMissing,
    SecretDecryptFailed,
    ProviderUnreachable,
    ProviderAuthFailed,
    ProviderSessionExpired,
    RateLimited,
    RunCancelled,
    RunFailed,
    ApprovalDenied,
    SidecarUnavailable,
    McpUnavailable,
    McpTimeout,
    NativeError,
    DbError,
    UnsupportedFormat,
    LimitExceeded,
    NotImplemented,
}

impl AppErrorCode {
    pub const ALL: &[Self] = &[
        Self::Unknown,
        Self::ValidationFailed,
        Self::NotFound,
        Self::Conflict,
        Self::PermissionDenied,
        Self::SecretMissing,
        Self::SecretDecryptFailed,
        Self::ProviderUnreachable,
        Self::ProviderAuthFailed,
        Self::ProviderSessionExpired,
        Self::RateLimited,
        Self::RunCancelled,
        Self::RunFailed,
        Self::ApprovalDenied,
        Self::SidecarUnavailable,
        Self::McpUnavailable,
        Self::McpTimeout,
        Self::NativeError,
        Self::DbError,
        Self::UnsupportedFormat,
        Self::LimitExceeded,
        Self::NotImplemented,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "UNKNOWN",
            Self::ValidationFailed => "VALIDATION_FAILED",
            Self::NotFound => "NOT_FOUND",
            Self::Conflict => "CONFLICT",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::SecretMissing => "SECRET_MISSING",
            Self::SecretDecryptFailed => "SECRET_DECRYPT_FAILED",
            Self::ProviderUnreachable => "PROVIDER_UNREACHABLE",
            Self::ProviderAuthFailed => "PROVIDER_AUTH_FAILED",
            Self::ProviderSessionExpired => "PROVIDER_SESSION_EXPIRED",
            Self::RateLimited => "RATE_LIMITED",
            Self::RunCancelled => "RUN_CANCELLED",
            Self::RunFailed => "RUN_FAILED",
            Self::ApprovalDenied => "APPROVAL_DENIED",
            Self::SidecarUnavailable => "SIDECAR_UNAVAILABLE",
            Self::McpUnavailable => "MCP_UNAVAILABLE",
            Self::McpTimeout => "MCP_TIMEOUT",
            Self::NativeError => "NATIVE_ERROR",
            Self::DbError => "DB_ERROR",
            Self::UnsupportedFormat => "UNSUPPORTED_FORMAT",
            Self::LimitExceeded => "LIMIT_EXCEEDED",
            Self::NotImplemented => "NOT_IMPLEMENTED",
        }
    }
}

impl fmt::Display for AppErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{code}: {message}")]
pub struct AppError {
    pub code: AppErrorCode,
    pub message: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub details: Option<ErrorDetails>,
}

impl AppError {
    pub fn new(code: AppErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }

    #[must_use]
    pub fn with_details(mut self, details: ErrorDetails) -> Self {
        self.details = Some(details);
        self
    }

    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.details
            .get_or_insert_with(Map::new)
            .insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codes_serialise_in_screaming_snake_case() {
        for code in AppErrorCode::ALL {
            assert_eq!(
                serde_json::to_value(code).expect("serialises"),
                json!(code.as_str())
            );
            assert_eq!(
                serde_json::from_value::<AppErrorCode>(json!(code.as_str())).expect("parses"),
                *code
            );
        }
        assert_eq!(
            serde_json::to_value(AppErrorCode::NotImplemented).expect("serialises"),
            json!("NOT_IMPLEMENTED")
        );
    }

    #[test]
    fn an_unknown_code_is_rejected() {
        assert!(serde_json::from_value::<AppErrorCode>(json!("NOPE")).is_err());
        assert!(serde_json::from_value::<AppErrorCode>(json!("not_found")).is_err());
    }

    #[test]
    fn an_error_without_details_omits_the_key() {
        let error = AppError::new(AppErrorCode::NotFound, "Не найдено");
        assert_eq!(
            serde_json::to_value(&error).expect("serialises"),
            json!({ "code": "NOT_FOUND", "message": "Не найдено" })
        );
        assert_eq!(error.to_string(), "NOT_FOUND: Не найдено");
    }

    #[test]
    fn details_are_an_optional_json_object() {
        let error = AppError::new(AppErrorCode::Conflict, "Конфликт").with_detail("id", "a");
        assert_eq!(
            serde_json::to_value(&error).expect("serialises"),
            json!({ "code": "CONFLICT", "message": "Конфликт", "details": { "id": "a" } })
        );
        for details in [json!(null), json!([])] {
            let raw = json!({ "code": "CONFLICT", "message": "", "details": details });
            assert!(serde_json::from_value::<AppError>(raw).is_err());
        }
    }
}
