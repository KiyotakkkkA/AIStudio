use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::validate::{Validate, check, invalid, nested};

pub const EXTENSION_ID_MAX: usize = 128;
pub const EXTENSION_TEXT_MAX: usize = 1024;
pub const EXTENSIONS_MAX: usize = 64;

pub fn is_extension_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= EXTENSION_ID_MAX
        && !id.starts_with('.')
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
}

fn extension_id(id: &str) -> Result<(), AppError> {
    if is_extension_id(id) {
        Ok(())
    } else {
        Err(invalid("id", "extensionId"))
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserExtensionDto {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
}

impl Validate for BrowserExtensionDto {
    fn validate(&self) -> Result<(), AppError> {
        extension_id(&self.id)?;
        check::non_empty("name", &self.name)?;
        for (field, value) in [
            ("name", &self.name),
            ("version", &self.version),
            ("description", &self.description),
        ] {
            check::max_len(field, value, EXTENSION_TEXT_MAX)?;
        }
        Ok(())
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserExtensionsDto {
    pub items: Vec<BrowserExtensionDto>,
}

impl Validate for BrowserExtensionsDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.items.len() > EXTENSIONS_MAX {
            return Err(invalid("items", "max"));
        }
        nested("items", self.items.validate())
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserExtensionRef {
    pub id: String,
}

impl Validate for BrowserExtensionRef {
    fn validate(&self) -> Result<(), AppError> {
        extension_id(&self.id)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserExtensionToggleInput {
    pub id: String,
    pub enabled: bool,
}

impl Validate for BrowserExtensionToggleInput {
    fn validate(&self) -> Result<(), AppError> {
        extension_id(&self.id)
    }
}
