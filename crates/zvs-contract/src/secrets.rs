use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Number, Value};
use std::fmt;
use zeroize::Zeroize;

use crate::errors::AppError;
use crate::ids::SecretId;
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, nested, no_refinements};

pub const SECRET_NAME_MAX: usize = 128;
pub const SECRET_TAG_MAX: usize = 32;
pub const SECRET_TAGS_MAX: usize = 16;
pub const SECRET_NOTE_MAX: usize = 2000;
pub const SECRET_QUERY_MAX: usize = 128;
pub const ROTATION_DAYS_MAX: u64 = 3650;

#[derive(Clone, PartialEq, Eq)]
pub struct SecretInput(String);

impl SecretInput {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for SecretInput {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for SecretInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretInput([redacted])")
    }
}

impl Serialize for SecretInput {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SecretInput {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecretScope {
    Personal,
    Shared,
    Public,
}

impl SecretScope {
    pub const ALL: &[Self] = &[Self::Personal, Self::Shared, Self::Public];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Shared => "shared",
            Self::Public => "public",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|scope| scope.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RotationStatus {
    Ok,
    DueSoon,
    Overdue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Secret,
    Text,
    Url,
    Number,
    Boolean,
    Select,
}

no_refinements!(SecretScope, RotationStatus, FieldKind);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldOption {
    pub value: String,
    pub label: String,
}

impl Validate for FieldOption {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("value", &self.value)?;
        check::non_empty("label", &self.label)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldDefault {
    Text(String),
    Number(Number),
    Boolean(bool),
}

impl From<&FieldDefault> for Value {
    fn from(value: &FieldDefault) -> Self {
        match value {
            FieldDefault::Text(text) => Self::String(text.clone()),
            FieldDefault::Number(number) => Self::Number(number.clone()),
            FieldDefault::Boolean(flag) => Self::Bool(*flag),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDescriptor {
    pub key: String,
    pub label: String,
    pub kind: FieldKind,
    pub required: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub placeholder: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub help: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub options: Option<Vec<FieldOption>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub default: Option<FieldDefault>,
}

pub fn is_field_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_alphanumeric())
}

fn check_key(field: &str, key: &str, shape: fn(&str) -> bool) -> Result<(), AppError> {
    check::non_empty(field, key)?;
    check::max_len(field, key, 64)?;
    if shape(key) {
        Ok(())
    } else {
        Err(invalid(field, "pattern"))
    }
}

impl Validate for FieldDescriptor {
    fn validate(&self) -> Result<(), AppError> {
        check_key("key", &self.key, is_field_key)?;
        check::non_empty("label", &self.label)?;
        if let Some(options) = &self.options {
            if options.is_empty() {
                return Err(invalid("options", "minItems"));
            }
            nested("options", options.validate())?;
        }
        Ok(())
    }
}

fn check_name(field: &str, name: &str) -> Result<(), AppError> {
    check::non_empty(field, name)?;
    check::max_len(field, name, SECRET_NAME_MAX)
}

fn check_tags(tags: &[String]) -> Result<(), AppError> {
    if tags.len() > SECRET_TAGS_MAX {
        return Err(invalid("tags", "maxItems"));
    }
    tags.iter().enumerate().try_for_each(|(index, tag)| {
        let field = format!("tags.{index}");
        check::non_empty(&field, tag)?;
        check::max_len(&field, tag, SECRET_TAG_MAX)
    })
}

fn check_note(note: Option<&str>) -> Result<(), AppError> {
    note.map_or(Ok(()), |note| check::max_len("note", note, SECRET_NOTE_MAX))
}

fn check_rotation_days(days: Option<u64>) -> Result<(), AppError> {
    days.map_or(Ok(()), |days| {
        check::int_range("rotationDays", days, 1, ROTATION_DAYS_MAX)
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretFilter {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub scope: Option<SecretScope>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub query: Option<String>,
}

impl Validate for SecretFilter {
    fn validate(&self) -> Result<(), AppError> {
        self.query.as_ref().map_or(Ok(()), |query| {
            check::max_len("query", query, SECRET_QUERY_MAX)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretRef {
    pub id: SecretId,
}

no_refinements!(SecretRef);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSecretInput {
    pub name: String,
    pub scope: SecretScope,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub value: Option<SecretInput>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub tags: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub note: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub rotation_days: Option<u64>,
}

impl CreateSecretInput {
    pub fn tags(&self) -> Vec<String> {
        self.tags.clone().unwrap_or_default()
    }
}

impl Validate for CreateSecretInput {
    fn validate(&self) -> Result<(), AppError> {
        check_name("name", &self.name)?;
        if let Some(tags) = &self.tags {
            check_tags(tags)?;
        }
        check_note(self.note.as_deref())?;
        check_rotation_days(self.rotation_days)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSecretInput {
    pub id: SecretId,
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
    pub scope: Option<SecretScope>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub value: Option<SecretInput>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub tags: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub note: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::optional_nullable"
    )]
    pub rotation_days: Option<Option<u64>>,
}

impl Validate for UpdateSecretInput {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(name) = &self.name {
            check_name("name", name)?;
        }
        if let Some(value) = &self.value {
            check::non_empty("value", value.expose())?;
        }
        if let Some(tags) = &self.tags {
            check_tags(tags)?;
        }
        check_note(self.note.as_deref())?;
        check_rotation_days(self.rotation_days.flatten())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretSummaryDto {
    pub id: SecretId,
    pub name: String,
    pub scope: SecretScope,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub hint: Option<String>,
    pub tags: Vec<String>,
    pub usage_count: u64,
    pub rotation_status: RotationStatus,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub rotates_at: Option<Timestamp>,
    pub updated_at: Timestamp,
}

fn check_summary(summary: &SecretSummaryDto) -> Result<(), AppError> {
    check_name("name", &summary.name)?;
    summary
        .tags
        .iter()
        .enumerate()
        .try_for_each(|(index, tag)| check::non_empty(&format!("tags.{index}"), tag))?;
    check::safe_integer("usageCount", summary.usage_count)?;
    if let Some(rotates_at) = summary.rotates_at {
        check::timestamp("rotatesAt", rotates_at)?;
    }
    check::timestamp("updatedAt", summary.updated_at)
}

impl Validate for SecretSummaryDto {
    fn validate(&self) -> Result<(), AppError> {
        check_summary(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretDto {
    pub id: SecretId,
    pub name: String,
    pub scope: SecretScope,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub hint: Option<String>,
    pub tags: Vec<String>,
    pub usage_count: u64,
    pub rotation_status: RotationStatus,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub rotates_at: Option<Timestamp>,
    pub updated_at: Timestamp,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub note: Option<String>,
}

impl SecretDto {
    pub fn summary(&self) -> SecretSummaryDto {
        SecretSummaryDto {
            id: self.id,
            name: self.name.clone(),
            scope: self.scope,
            hint: self.hint.clone(),
            tags: self.tags.clone(),
            usage_count: self.usage_count,
            rotation_status: self.rotation_status,
            rotates_at: self.rotates_at,
            updated_at: self.updated_at,
        }
    }
}

impl Validate for SecretDto {
    fn validate(&self) -> Result<(), AppError> {
        check_summary(&self.summary())
    }
}

pub type SecretsListOutput = Vec<SecretSummaryDto>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretsRemoveOutput {
    pub id: SecretId,
    pub removed: bool,
}

impl Validate for SecretsRemoveOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::literal_true("removed", self.removed)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_secret_input_never_prints_its_value() {
        let input: CreateSecretInput = serde_json::from_value(json!({
            "name": "Ollama",
            "scope": "personal",
            "value": "osk_live_planted-value",
        }))
        .expect("parses");
        let printed = format!("{input:?}");
        assert!(!printed.contains("planted"));
        assert!(printed.contains("[redacted]"));
        assert_eq!(
            input.value.as_ref().map(SecretInput::expose),
            Some("osk_live_planted-value")
        );
    }

    #[test]
    fn key_shapes_follow_the_zod_patterns() {
        for good in ["apiKey", "a", "baseUrl2"] {
            assert!(is_field_key(good), "{good}");
        }
        for bad in ["", "ApiKey", "api_key", "1a", "api-key"] {
            assert!(!is_field_key(bad), "{bad}");
        }
    }
}
