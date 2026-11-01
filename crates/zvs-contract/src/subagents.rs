use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::ids::RunId;
use crate::validate::{Validate, check, invalid, nested};

pub const SUBAGENT_DOMAIN: &str = "subagent";
pub const SUBAGENT_MODELS_MAX: usize = 5;
pub const SUBAGENT_CONCURRENCY_MIN: u64 = 1;
pub const SUBAGENT_CONCURRENCY_MAX: u64 = 4;
pub const SUBAGENT_CONCURRENCY_DEFAULT: u64 = 2;
pub const SUBAGENT_MODEL_ID_MAX: usize = 200;

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentModelDto {
    pub provider_id: String,
    pub model_id: String,
}

impl Validate for SubagentModelDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("providerId", &self.provider_id)?;
        check::max_len("providerId", &self.provider_id, SUBAGENT_MODEL_ID_MAX)?;
        check::non_empty("modelId", &self.model_id)?;
        check::max_len("modelId", &self.model_id, SUBAGENT_MODEL_ID_MAX)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentPreferencesDto {
    pub enabled: bool,
    pub models: Vec<SubagentModelDto>,
    pub concurrency: u64,
}

impl Default for SubagentPreferencesDto {
    fn default() -> Self {
        Self {
            enabled: true,
            models: Vec::new(),
            concurrency: SUBAGENT_CONCURRENCY_DEFAULT,
        }
    }
}

impl Validate for SubagentPreferencesDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.models.len() > SUBAGENT_MODELS_MAX {
            return Err(invalid("models", "maxLength"));
        }
        nested("models", self.models.validate())?;
        for (index, model) in self.models.iter().enumerate() {
            if self.models[..index].contains(model) {
                return Err(invalid(&format!("models.{index}"), "unique"));
            }
        }
        check::int_range(
            "concurrency",
            self.concurrency,
            SUBAGENT_CONCURRENCY_MIN,
            SUBAGENT_CONCURRENCY_MAX,
        )
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliSubagentCancelInput {
    pub run_id: RunId,
}

impl Validate for CliSubagentCancelInput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str) -> SubagentModelDto {
        SubagentModelDto {
            provider_id: "p".to_owned(),
            model_id: id.to_owned(),
        }
    }

    #[test]
    fn duplicates_too_many_models_and_concurrency_out_of_range_are_refused() {
        let base = SubagentPreferencesDto::default();
        assert!(base.validate().is_ok());
        let duplicated = SubagentPreferencesDto {
            models: vec![model("a"), model("a")],
            ..base.clone()
        };
        assert!(duplicated.validate().is_err());
        let many = SubagentPreferencesDto {
            models: (0..6).map(|index| model(&index.to_string())).collect(),
            ..base.clone()
        };
        assert!(many.validate().is_err());
        for concurrency in [0, 5] {
            let out = SubagentPreferencesDto {
                concurrency,
                ..base.clone()
            };
            assert!(out.validate().is_err());
        }
    }
}
