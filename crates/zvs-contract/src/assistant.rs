use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::validate::{Validate, check, invalid, nested};

pub const SUMMARIZER_MODELS_MAX: usize = 5;
pub const SUMMARIZER_MODEL_ID_MAX: usize = 200;
pub const AUTO_COMPACT_PERCENT: u64 = 80;

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummarizerModelDto {
    pub provider_id: String,
    pub model_id: String,
}

impl Validate for SummarizerModelDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("providerId", &self.provider_id)?;
        check::max_len("providerId", &self.provider_id, SUMMARIZER_MODEL_ID_MAX)?;
        check::non_empty("modelId", &self.model_id)?;
        check::max_len("modelId", &self.model_id, SUMMARIZER_MODEL_ID_MAX)
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantPreferencesDto {
    pub auto_compact: bool,
    pub recaps: bool,
    pub summarizer_models: Vec<SummarizerModelDto>,
}

impl Default for AssistantPreferencesDto {
    fn default() -> Self {
        Self {
            auto_compact: true,
            recaps: true,
            summarizer_models: Vec::new(),
        }
    }
}

impl Validate for AssistantPreferencesDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.summarizer_models.len() > SUMMARIZER_MODELS_MAX {
            return Err(invalid("summarizerModels", "maxLength"));
        }
        nested("summarizerModels", self.summarizer_models.validate())?;
        for (index, model) in self.summarizer_models.iter().enumerate() {
            if self.summarizer_models[..index].contains(model) {
                return Err(invalid(&format!("summarizerModels.{index}"), "unique"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str) -> SummarizerModelDto {
        SummarizerModelDto {
            provider_id: "p".to_owned(),
            model_id: id.to_owned(),
        }
    }

    #[test]
    fn duplicates_and_too_many_models_are_refused() {
        let base = AssistantPreferencesDto::default();
        assert!(base.validate().is_ok());
        let duplicated = AssistantPreferencesDto {
            summarizer_models: vec![model("a"), model("a")],
            ..base.clone()
        };
        assert!(duplicated.validate().is_err());
        let many = AssistantPreferencesDto {
            summarizer_models: (0..6).map(|index| model(&index.to_string())).collect(),
            ..base.clone()
        };
        assert!(many.validate().is_err());
        let empty_id = AssistantPreferencesDto {
            summarizer_models: vec![model("")],
            ..base
        };
        assert!(empty_id.validate().is_err());
    }
}
