use serde::{Deserialize, Serialize};

use crate::downloads::{CatalogueItemState, check_item_ref};
use crate::errors::AppError;
use crate::validate::{Validate, check, invalid, nested};

pub const RECORDING_DEVICE_ID_MAX: usize = 512;
pub const RECORDING_DEVICE_NAME_MAX: usize = 200;
pub const SPEECH_MODEL_NAME_MAX: usize = 200;
pub const SPEECH_MODEL_DESCRIPTION_MAX: usize = 1000;
pub const SPEECH_REASON_MAX: usize = 500;
pub const VOICE_TRANSCRIPT_MAX: usize = 100_000;

fn check_device_id(field: &str, id: &str) -> Result<(), AppError> {
    check::non_empty(field, id)?;
    check::max_len(field, id, RECORDING_DEVICE_ID_MAX)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingDeviceDto {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

impl Validate for RecordingDeviceDto {
    fn validate(&self) -> Result<(), AppError> {
        check_device_id("id", &self.id)?;
        check::non_empty("name", &self.name)?;
        check::max_len("name", &self.name, RECORDING_DEVICE_NAME_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceDevicesDto {
    pub devices: Vec<RecordingDeviceDto>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub selected_id: Option<String>,
}

impl Validate for VoiceDevicesDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("devices", self.devices.validate())?;
        if self
            .devices
            .iter()
            .filter(|device| device.is_default)
            .count()
            > 1
        {
            return Err(invalid("devices", "single_default"));
        }
        match &self.selected_id {
            Some(id) => check_device_id("selectedId", id),
            None => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSelectInput {
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub device_id: Option<String>,
}

impl Validate for VoiceSelectInput {
    fn validate(&self) -> Result<(), AppError> {
        match &self.device_id {
            Some(id) => check_device_id("deviceId", id),
            None => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechModelDto {
    #[serde(rename = "ref")]
    pub model_ref: String,
    pub display_name: String,
    pub description: String,
    pub size_bytes: u64,
    pub state: CatalogueItemState,
    pub recommended: bool,
}

impl Validate for SpeechModelDto {
    fn validate(&self) -> Result<(), AppError> {
        check_item_ref("ref", &self.model_ref)?;
        check::non_empty("displayName", &self.display_name)?;
        check::max_len("displayName", &self.display_name, SPEECH_MODEL_NAME_MAX)?;
        check::max_len(
            "description",
            &self.description,
            SPEECH_MODEL_DESCRIPTION_MAX,
        )?;
        check::safe_integer("sizeBytes", self.size_bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceModelsDto {
    pub models: Vec<SpeechModelDto>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub selected_ref: Option<String>,
    pub engine_state: CatalogueItemState,
    pub recommended_ref: String,
    pub reason: String,
}

impl Validate for VoiceModelsDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("models", self.models.validate())?;
        if self.models.iter().filter(|model| model.recommended).count() > 1 {
            return Err(invalid("models", "single_recommended"));
        }
        if let Some(selected) = &self.selected_ref {
            check_item_ref("selectedRef", selected)?;
        }
        check_item_ref("recommendedRef", &self.recommended_ref)?;
        check::max_len("reason", &self.reason, SPEECH_REASON_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceModelInput {
    pub model_ref: String,
}

impl Validate for VoiceModelInput {
    fn validate(&self) -> Result<(), AppError> {
        check_item_ref("modelRef", &self.model_ref)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSelectModelInput {
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub model_ref: Option<String>,
}

impl Validate for VoiceSelectModelInput {
    fn validate(&self) -> Result<(), AppError> {
        match &self.model_ref {
            Some(model_ref) => check_item_ref("modelRef", model_ref),
            None => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceTranscriptDto {
    pub text: String,
}

impl Validate for VoiceTranscriptDto {
    fn validate(&self) -> Result<(), AppError> {
        check::max_len("text", &self.text, VOICE_TRANSCRIPT_MAX)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    #[test]
    fn a_device_list_names_at_most_one_default() {
        let device = |id: &str, is_default: bool| json!({ "id": id, "name": "Mic", "isDefault": is_default });
        assert!(
            parse::<VoiceDevicesDto>(
                json!({ "devices": [device("a", true), device("b", false)], "selectedId": "b" })
            )
            .is_ok()
        );
        assert!(
            parse::<VoiceDevicesDto>(
                json!({ "devices": [device("a", true), device("b", true)], "selectedId": null })
            )
            .is_err()
        );
    }

    #[test]
    fn the_system_default_is_selected_with_null() {
        assert_eq!(
            parse::<VoiceSelectInput>(json!({ "deviceId": null })).expect("parses"),
            VoiceSelectInput { device_id: None }
        );
        assert!(parse::<VoiceSelectInput>(json!({ "deviceId": "" })).is_err());
        assert!(parse::<VoiceSelectInput>(json!({})).is_err());
    }

    #[test]
    fn a_model_list_recommends_at_most_one_model() {
        let model = |model_ref: &str, recommended: bool| {
            json!({
                "ref": model_ref,
                "displayName": "Whisper",
                "description": "",
                "sizeBytes": 1,
                "state": "available",
                "recommended": recommended
            })
        };
        let list = |models: Vec<serde_json::Value>| {
            json!({
                "models": models,
                "selectedRef": null,
                "engineState": "available",
                "recommendedRef": "curated:speech:a",
                "reason": ""
            })
        };
        assert!(
            parse::<VoiceModelsDto>(list(vec![
                model("curated:speech:a", true),
                model("curated:speech:b", false)
            ]))
            .is_ok()
        );
        assert!(
            parse::<VoiceModelsDto>(list(vec![
                model("curated:speech:a", true),
                model("curated:speech:b", true)
            ]))
            .is_err()
        );
    }
}
