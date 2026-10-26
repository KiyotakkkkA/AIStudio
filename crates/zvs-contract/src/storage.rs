use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, nested, no_refinements};

wire_enum!(StorageCategory {
    Conversations => "conversations",
    Dashboards => "dashboards",
    Skills => "skills",
    Vectors => "vectors",
    Downloads => "downloads",
    Toolchain => "toolchain",
    Browser => "browser",
    Cache => "cache",
    Logs => "logs",
    Backups => "backups",
    Database => "database",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageCategoryUsage {
    pub category: StorageCategory,
    pub bytes: u64,
}

impl Validate for StorageCategoryUsage {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("bytes", self.bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsageDto {
    pub total_bytes: u64,
    pub categories: Vec<StorageCategoryUsage>,
    pub measured_at: Timestamp,
}

impl Validate for StorageUsageDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("totalBytes", self.total_bytes)?;
        nested("categories", self.categories.validate())?;
        let sum = self
            .categories
            .iter()
            .try_fold(0_u64, |sum, usage| sum.checked_add(usage.bytes));
        if sum != Some(self.total_bytes) {
            return Err(invalid("totalBytes", "sum"));
        }
        check::timestamp("measuredAt", self.measured_at)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsageInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub refresh: Option<bool>,
}

impl StorageUsageInput {
    pub fn refresh(&self) -> bool {
        self.refresh.unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageClearCacheOutput {
    pub removed_bytes: u64,
}

impl Validate for StorageClearCacheOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("removedBytes", self.removed_bytes)
    }
}

no_refinements!(StorageUsageInput);

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    #[test]
    fn usage_totals_must_match_the_categories() {
        let usage = |total: u64| {
            parse::<StorageUsageDto>(json!({
                "totalBytes": total,
                "categories": [
                    { "category": "conversations", "bytes": 3 },
                    { "category": "database", "bytes": 4 },
                ],
                "measuredAt": 1_760_000_000_000_i64,
            }))
        };
        assert!(usage(7).is_ok());
        assert!(usage(8).is_err());
    }
}
