use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::primitives::{Json, Timestamp};
use crate::validate::{Validate, check, invalid};

pub const SETTING_KEY_MAX: usize = 128;

pub fn check_setting_key(field: &str, key: &str) -> Result<(), AppError> {
    check::non_empty(field, key)?;
    check::max_len(field, key, SETTING_KEY_MAX)?;
    let segment = |part: &str, first: bool| {
        !part.is_empty()
            && part.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_lowercase() || (byte.is_ascii_digit() && !(first && index == 0))
            })
    };
    let mut parts = key.split('.');
    let head = parts.next().is_some_and(|part| segment(part, true));
    if head && parts.all(|part| segment(part, false)) {
        Ok(())
    } else {
        Err(invalid(field, "pattern"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsGetInput {
    pub key: String,
}

impl Validate for SettingsGetInput {
    fn validate(&self) -> Result<(), AppError> {
        check_setting_key("key", &self.key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsGetOutput {
    pub key: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub value: Option<Json>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub updated_at: Option<Timestamp>,
}

impl Validate for SettingsGetOutput {
    fn validate(&self) -> Result<(), AppError> {
        check_setting_key("key", &self.key)?;
        if let Some(updated_at) = self.updated_at {
            check::timestamp("updatedAt", updated_at)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSetInput {
    pub key: String,
    pub value: Json,
}

impl Validate for SettingsSetInput {
    fn validate(&self) -> Result<(), AppError> {
        check_setting_key("key", &self.key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSetOutput {
    pub key: String,
    pub value: Json,
    pub updated_at: Timestamp,
}

impl Validate for SettingsSetOutput {
    fn validate(&self) -> Result<(), AppError> {
        check_setting_key("key", &self.key)?;
        check::timestamp("updatedAt", self.updated_at)
    }
}

impl From<SettingsSetOutput> for SettingsGetOutput {
    fn from(setting: SettingsSetOutput) -> Self {
        Self {
            key: setting.key,
            value: Some(setting.value),
            updated_at: Some(setting.updated_at),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_keys_follow_the_dotted_lowercase_shape() {
        for good in [
            "ui.theme",
            "window.main",
            "a",
            "a1.b2.3c",
            "health.check.interval",
        ] {
            assert!(check_setting_key("key", good).is_ok(), "{good}");
        }
        let too_long = "a".repeat(SETTING_KEY_MAX + 1);
        for bad in [
            "",
            "1ui",
            "Ui.theme",
            "ui..theme",
            "ui.",
            ".ui",
            "ui-theme",
            "ui_theme",
            "ui.Theme",
            "тема",
            too_long.as_str(),
        ] {
            assert!(check_setting_key("key", bad).is_err(), "{bad}");
        }
        assert!(check_setting_key("key", &"a".repeat(SETTING_KEY_MAX)).is_ok());
    }
}
