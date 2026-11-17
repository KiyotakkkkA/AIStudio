use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::validate::{Validate, check, invalid};
use crate::{AppError, Timestamp};

pub const TRANSFER_MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;

wire_enum!(TransferCategory {
    Conversations => "conversations",
    Stores => "stores",
    Scenarios => "scenarios",
    Skills => "skills",
    Memory => "memory",
    Connections => "connections",
    Settings => "settings",
});

wire_enum!(TransferWarning {
    Credentials => "credentials",
    Indexes => "indexes",
    Inactive => "inactive",
    Settings => "settings",
    MissingFiles => "missingFiles",
});

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferSelection {
    pub categories: Vec<TransferCategory>,
}

fn selection(categories: &[TransferCategory]) -> Result<(), AppError> {
    if categories.is_empty()
        || categories.len() > 7
        || categories
            .iter()
            .enumerate()
            .any(|(index, item)| categories[..index].contains(item))
    {
        return Err(invalid("categories", "selection"));
    }
    Ok(())
}

impl Validate for TransferSelection {
    fn validate(&self) -> Result<(), AppError> {
        selection(&self.categories)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferCategoryDto {
    pub category: TransferCategory,
    pub count: u64,
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<TransferCategory>,
}

impl Validate for TransferCategoryDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("count", self.count)?;
        check::safe_integer("bytes", self.bytes)?;
        if self.names.len() > 20 || self.requires.len() > 7 {
            return Err(invalid("names", "length"));
        }
        for name in &self.names {
            check::max_len("names", name, 120)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferPreviewDto {
    pub created_at: Timestamp,
    pub categories: Vec<TransferCategoryDto>,
    pub warnings: Vec<TransferWarning>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_categories: Vec<TransferCategory>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imported_categories: Vec<TransferCategory>,
}

impl Validate for TransferPreviewDto {
    fn validate(&self) -> Result<(), AppError> {
        check::timestamp("createdAt", self.created_at)?;
        if self.categories.len() > 7 {
            return Err(invalid("categories", "length"));
        }
        if self.blocked_categories.len() > 7 || self.imported_categories.len() > 7 {
            return Err(invalid("blockedCategories", "length"));
        }
        self.categories.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferUploadInput {
    pub name: String,
    pub bytes: u64,
}

impl Validate for TransferUploadInput {
    fn validate(&self) -> Result<(), AppError> {
        if self.name.encode_utf16().count() > 255
            || !self.name.to_ascii_lowercase().ends_with(".dbox")
            || self.bytes == 0
            || self.bytes > TRANSFER_MAX_BYTES
        {
            return Err(invalid("file", "format"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferRef {
    pub id: String,
}

impl Validate for TransferRef {
    fn validate(&self) -> Result<(), AppError> {
        if crate::ProviderId::parse(&self.id).is_none() {
            return Err(invalid("id", "uuid"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferChunkInput {
    pub id: String,
    pub offset: u64,
    pub data: String,
}

impl Validate for TransferChunkInput {
    fn validate(&self) -> Result<(), AppError> {
        TransferRef {
            id: self.id.clone(),
        }
        .validate()?;
        let unpadded = self.data.trim_end_matches('=');
        if self.data.len() - unpadded.len() > 2
            || unpadded.contains('=')
            || self.offset > TRANSFER_MAX_BYTES
            || !(4..=699_052).contains(&self.data.len())
            || !self
                .data
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"+/=".contains(&byte))
        {
            return Err(invalid("data", "chunk"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferImportInput {
    pub id: String,
    pub categories: Vec<TransferCategory>,
}

impl Validate for TransferImportInput {
    fn validate(&self) -> Result<(), AppError> {
        TransferRef {
            id: self.id.clone(),
        }
        .validate()?;
        selection(&self.categories)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferExportDto {
    pub path: String,
}

impl Validate for TransferExportDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.path.is_empty() {
            Err(invalid("path", "empty"))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferImportDto {
    pub categories: Vec<TransferCategoryDto>,
}

impl Validate for TransferImportDto {
    fn validate(&self) -> Result<(), AppError> {
        self.categories.validate()
    }
}
