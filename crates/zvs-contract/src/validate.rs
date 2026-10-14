use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::errors::{AppError, AppErrorCode};

pub const VALIDATION_MESSAGE: &str = "Некорректные данные запроса";

pub trait Validate {
    fn validate(&self) -> Result<(), AppError>;
}

pub fn parse<T: DeserializeOwned + Validate>(value: Value) -> Result<T, AppError> {
    let parsed: T = serde_json::from_value(value).map_err(|error| {
        AppError::new(AppErrorCode::ValidationFailed, VALIDATION_MESSAGE)
            .with_detail("reason", error.to_string())
    })?;
    parsed.validate()?;
    Ok(parsed)
}

pub fn invalid(field: &str, rule: &str) -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, VALIDATION_MESSAGE)
        .with_detail("field", field)
        .with_detail("rule", rule)
}

pub fn nested(prefix: &str, result: Result<(), AppError>) -> Result<(), AppError> {
    result.map_err(|mut error| {
        if let Some(details) = error.details.as_mut() {
            let path = match details.get("field").and_then(Value::as_str) {
                Some(inner) if !inner.is_empty() => format!("{prefix}.{inner}"),
                _ => prefix.to_owned(),
            };
            details.insert("field".to_owned(), Value::String(path));
        }
        error
    })
}

impl<T: Validate> Validate for Option<T> {
    fn validate(&self) -> Result<(), AppError> {
        self.as_ref().map_or(Ok(()), Validate::validate)
    }
}

impl<T: Validate> Validate for Vec<T> {
    fn validate(&self) -> Result<(), AppError> {
        self.iter()
            .enumerate()
            .try_for_each(|(index, item)| nested(&index.to_string(), item.validate()))
    }
}

macro_rules! no_refinements {
    ($($type:ty),+ $(,)?) => {$(
        impl $crate::validate::Validate for $type {
            fn validate(&self) -> Result<(), $crate::errors::AppError> {
                Ok(())
            }
        }
    )+};
}
pub(crate) use no_refinements;

no_refinements!((), Value, String, bool, AppError, AppErrorCode);

pub mod check {
    use crate::errors::AppError;
    use crate::primitives::MAX_SAFE_INTEGER;

    use super::invalid;

    pub fn safe_integer(field: &str, value: u64) -> Result<(), AppError> {
        if value <= MAX_SAFE_INTEGER.unsigned_abs() {
            Ok(())
        } else {
            Err(invalid(field, "safeInteger"))
        }
    }

    pub fn timestamp(field: &str, value: i64) -> Result<(), AppError> {
        if (0..=MAX_SAFE_INTEGER).contains(&value) {
            Ok(())
        } else {
            Err(invalid(field, "timestamp"))
        }
    }

    pub fn int_range(field: &str, value: u64, min: u64, max: u64) -> Result<(), AppError> {
        if (min..=max).contains(&value) {
            Ok(())
        } else {
            Err(invalid(field, "range"))
        }
    }

    pub fn number_range(field: &str, value: f64, min: f64, max: f64) -> Result<(), AppError> {
        if value.is_finite() && value >= min && value <= max {
            Ok(())
        } else {
            Err(invalid(field, "range"))
        }
    }

    pub fn finite(field: &str, value: f64) -> Result<(), AppError> {
        if value.is_finite() {
            Ok(())
        } else {
            Err(invalid(field, "finite"))
        }
    }

    pub fn max_len(field: &str, value: &str, max: usize) -> Result<(), AppError> {
        if value.encode_utf16().count() <= max {
            Ok(())
        } else {
            Err(invalid(field, "maxLength"))
        }
    }

    pub fn non_empty(field: &str, value: &str) -> Result<(), AppError> {
        if value.is_empty() {
            Err(invalid(field, "nonEmpty"))
        } else {
            Ok(())
        }
    }

    pub fn literal_true(field: &str, value: bool) -> Result<(), AppError> {
        if value {
            Ok(())
        } else {
            Err(invalid(field, "literal"))
        }
    }

    pub fn trimmed_name(field: &str, value: &str, max: usize) -> Result<(), AppError> {
        let trimmed = super::js_trim(value);
        non_empty(field, trimmed)?;
        max_len(field, trimmed, max)
    }

    pub fn url(field: &str, value: &str, max: usize) -> Result<(), AppError> {
        let normalized = super::normalize_url(value);
        if url::Url::parse(super::js_trim(value)).is_err() {
            return Err(invalid(field, "url"));
        }
        max_len(field, &normalized, max)
    }
}

pub fn js_trim(value: &str) -> &str {
    value.trim_matches(|character: char| character.is_whitespace() || character == '\u{feff}')
}

pub fn normalize_url(value: &str) -> String {
    js_trim(value)
        .chars()
        .filter(|character| !matches!(character, '\t' | '\n' | '\r'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_failure_names_the_field_and_rule() {
        let error = invalid("limit", "range");
        assert_eq!(
            serde_json::to_value(&error).expect("serialises"),
            json!({
                "code": "VALIDATION_FAILED",
                "message": VALIDATION_MESSAGE,
                "details": { "field": "limit", "rule": "range" },
            })
        );
    }

    #[test]
    fn nesting_prefixes_the_field_path() {
        let error = nested("gpu", Err(invalid("vramBytes", "safeInteger"))).expect_err("fails");
        assert_eq!(
            error
                .details
                .and_then(|details| details.get("field").cloned()),
            Some(json!("gpu.vramBytes"))
        );
    }

    #[test]
    fn a_shape_error_becomes_validation_failed() {
        let error = parse::<String>(json!(1)).expect_err("fails");
        assert_eq!(error.code, AppErrorCode::ValidationFailed);
        assert!(
            error
                .details
                .is_some_and(|details| details.contains_key("reason"))
        );
    }

    #[test]
    fn lengths_count_utf16_code_units_like_javascript() {
        assert!(check::max_len("x", "😀", 2).is_ok());
        assert!(check::max_len("x", "😀", 1).is_err());
        assert!(check::max_len("x", "ж", 1).is_ok());
    }

    #[test]
    fn integers_stay_within_the_javascript_safe_range() {
        assert!(check::safe_integer("x", 9_007_199_254_740_991).is_ok());
        assert!(check::safe_integer("x", 9_007_199_254_740_992).is_err());
        assert!(check::timestamp("x", -1).is_err());
        assert!(check::timestamp("x", 0).is_ok());
    }
}
