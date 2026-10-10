use std::mem::size_of;

use serde::de::{DeserializeOwned, Error as _};
use serde::ser::{Error as _, SerializeMap};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::errors::AppError;
use crate::validate::{Validate, nested};

#[derive(Debug, Clone, PartialEq)]
pub enum ResultEnvelope<T> {
    Ok(T),
    Err(AppError),
}

impl<T> ResultEnvelope<T> {
    pub const fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }

    pub fn into_result(self) -> Result<T, AppError> {
        match self {
            Self::Ok(data) => Ok(data),
            Self::Err(error) => Err(error),
        }
    }
}

impl<T> From<Result<T, AppError>> for ResultEnvelope<T> {
    fn from(result: Result<T, AppError>) -> Self {
        match result {
            Ok(data) => Self::Ok(data),
            Err(error) => Self::Err(error),
        }
    }
}

const fn is_void<T>() -> bool {
    size_of::<T>() == 0
}

impl<T: Serialize> Serialize for ResultEnvelope<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Ok(data) => {
                let data = serde_json::to_value(data).map_err(S::Error::custom)?;
                let omit = is_void::<T>() && data.is_null();
                let mut map = serializer.serialize_map(Some(if omit { 1 } else { 2 }))?;
                map.serialize_entry("ok", &true)?;
                if !omit {
                    map.serialize_entry("data", &data)?;
                }
                map.end()
            }
            Self::Err(error) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("ok", &false)?;
                map.serialize_entry("error", error)?;
                map.end()
            }
        }
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for ResultEnvelope<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut raw = Map::<String, Value>::deserialize(deserializer)?;
        match raw.remove("ok") {
            Some(Value::Bool(true)) => {
                let data = match raw.remove("data") {
                    Some(data) => data,
                    None if is_void::<T>() => Value::Null,
                    None => return Err(D::Error::missing_field("data")),
                };
                T::deserialize(data).map(Self::Ok).map_err(D::Error::custom)
            }
            Some(Value::Bool(false)) => {
                let error = raw
                    .remove("error")
                    .ok_or_else(|| D::Error::missing_field("error"))?;
                AppError::deserialize(error)
                    .map(Self::Err)
                    .map_err(D::Error::custom)
            }
            Some(_) => Err(D::Error::custom("`ok` must be a boolean")),
            None => Err(D::Error::missing_field("ok")),
        }
    }
}

impl<T: Validate> Validate for ResultEnvelope<T> {
    fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::Ok(data) => nested("data", data.validate()),
            Self::Err(_) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::AppErrorCode;
    use serde_json::json;

    #[test]
    fn success_carries_data() {
        let envelope = ResultEnvelope::Ok(json!({ "pong": true }));
        assert_eq!(
            serde_json::to_value(&envelope).expect("serialises"),
            json!({ "ok": true, "data": { "pong": true } })
        );
    }

    #[test]
    fn failure_carries_the_error() {
        let envelope: ResultEnvelope<Value> =
            ResultEnvelope::Err(AppError::new(AppErrorCode::NotImplemented, "Нет"));
        assert_eq!(
            serde_json::to_value(&envelope).expect("serialises"),
            json!({ "ok": false, "error": { "code": "NOT_IMPLEMENTED", "message": "Нет" } })
        );
    }

    #[test]
    fn void_success_omits_data() {
        let envelope = ResultEnvelope::Ok(());
        assert_eq!(
            serde_json::to_value(&envelope).expect("serialises"),
            json!({ "ok": true })
        );
        assert!(serde_json::from_value::<ResultEnvelope<()>>(json!({ "ok": true })).is_ok());
    }

    #[test]
    fn a_null_payload_is_kept_for_nullable_outputs() {
        let envelope: ResultEnvelope<Option<u64>> = ResultEnvelope::Ok(None);
        assert_eq!(
            serde_json::to_value(&envelope).expect("serialises"),
            json!({ "ok": true, "data": null })
        );
        assert!(
            serde_json::from_value::<ResultEnvelope<Option<u64>>>(json!({ "ok": true })).is_err()
        );
    }

    #[test]
    fn malformed_envelopes_are_rejected() {
        for raw in [
            json!({}),
            json!({ "ok": "yes", "data": 1 }),
            json!({ "ok": false }),
            json!({ "ok": false, "error": { "code": "NOPE", "message": "" } }),
            json!([true, 1]),
        ] {
            assert!(
                serde_json::from_value::<ResultEnvelope<Value>>(raw.clone()).is_err(),
                "{raw}"
            );
        }
    }
}
