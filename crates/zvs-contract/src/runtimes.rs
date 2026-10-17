use serde::{Deserialize, Serialize};

use crate::downloads::check_item_ref;
use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::system::DeviceGpuDto;
use crate::validate::{Validate, check, invalid, nested, no_refinements};

wire_enum!(ModelFormat {
    Gguf => "gguf",
    Onnx => "onnx",
});

wire_enum!(Accelerator {
    Cpu => "cpu",
    Cuda => "cuda",
    Vulkan => "vulkan",
    Metal => "metal",
});

wire_enum!(RuntimeEngine {
    LlamaCpp => "llama-cpp",
    Onnxruntime => "onnxruntime",
});

wire_enum!(RuntimeState {
    Unsupported => "unsupported",
    Available => "available",
    Installing => "installing",
    Installed => "installed",
    Starting => "starting",
    Ready => "ready",
    Failed => "failed",
});

fn check_runtime_id(field: &str, value: &str) -> Result<(), AppError> {
    if value.encode_utf16().count() < 3 {
        return Err(invalid(field, "minLength"));
    }
    check::max_len(field, value, 64)
}

fn check_optional_len(field: &str, value: Option<&String>, max: usize) -> Result<(), AppError> {
    value.map_or(Ok(()), |value| check::max_len(field, value, max))
}

fn check_refs(field: &str, refs: &[String], max: usize) -> Result<(), AppError> {
    if refs.len() > max {
        return Err(invalid(field, "maxItems"));
    }
    refs.iter()
        .enumerate()
        .try_for_each(|(index, item)| check_item_ref(&format!("{field}.{index}"), item))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDto {
    pub id: String,
    pub engine: RuntimeEngine,
    pub accelerator: Accelerator,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    pub formats: Vec<ModelFormat>,
    pub state: RuntimeState,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub installed_version: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub install_path: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub executable_path: Option<String>,
    pub size_bytes: u64,
    pub recommended: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub blocked_reason: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub error: Option<String>,
    #[serde(default)]
    pub refs: Vec<String>,
}

impl Validate for RuntimeDto {
    fn validate(&self) -> Result<(), AppError> {
        check_runtime_id("id", &self.id)?;
        check::non_empty("displayName", &self.display_name)?;
        check::max_len("displayName", &self.display_name, 120)?;
        check::max_len("description", &self.description, 600)?;
        if self.formats.is_empty() {
            return Err(invalid("formats", "minItems"));
        }
        check_optional_len("installedVersion", self.installed_version.as_ref(), 64)?;
        check_optional_len("installPath", self.install_path.as_ref(), 4096)?;
        check_optional_len("executablePath", self.executable_path.as_ref(), 4096)?;
        check::safe_integer("sizeBytes", self.size_bytes)?;
        check_optional_len("blockedReason", self.blocked_reason.as_ref(), 400)?;
        check_optional_len("error", self.error.as_ref(), 2000)?;
        check_refs("refs", &self.refs, 8)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimePlanDto {
    pub accelerator: Accelerator,
    pub reason: String,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub gpu: Option<DeviceGpuDto>,
    pub ready: bool,
}

impl Validate for RuntimePlanDto {
    fn validate(&self) -> Result<(), AppError> {
        check::max_len("reason", &self.reason, 400)?;
        nested("gpu", self.gpu.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeOverviewDto {
    pub plan: RuntimePlanDto,
    pub runtimes: Vec<RuntimeDto>,
    #[serde(default)]
    pub recommended_models: Vec<String>,
}

impl Validate for RuntimeOverviewDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("plan", self.plan.validate())?;
        nested("runtimes", self.runtimes.validate())?;
        check_refs("recommendedModels", &self.recommended_models, 16)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRef {
    pub id: String,
}

impl Validate for RuntimeRef {
    fn validate(&self) -> Result<(), AppError> {
        check_runtime_id("id", &self.id)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRuntimeInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub id: Option<String>,
}

impl Validate for InstallRuntimeInput {
    fn validate(&self) -> Result<(), AppError> {
        self.id
            .as_deref()
            .map_or(Ok(()), |id| check_runtime_id("id", id))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimesStopOutput {
    pub stopped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimesRemoveOutput {
    pub id: String,
    pub removed: bool,
}

impl Validate for RuntimesRemoveOutput {
    fn validate(&self) -> Result<(), AppError> {
        check_runtime_id("id", &self.id)?;
        check::literal_true("removed", self.removed)
    }
}

no_refinements!(RuntimesStopOutput);

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    #[test]
    fn a_runtime_id_is_between_3_and_64_characters() {
        assert!(parse::<RuntimeRef>(json!({ "id": "llama-cpp-cpu" })).is_ok());
        assert!(parse::<RuntimeRef>(json!({ "id": "ab" })).is_err());
        assert!(parse::<RuntimeRef>(json!({ "id": "a".repeat(65) })).is_err());
        assert!(parse::<InstallRuntimeInput>(json!({})).is_ok());
        assert!(parse::<InstallRuntimeInput>(json!({ "id": null })).is_err());
    }

    #[test]
    fn a_runtime_needs_a_format_and_writes_its_defaults() {
        let runtime = json!({
            "id": "llama-cpp-cpu",
            "engine": "llama-cpp",
            "accelerator": "cpu",
            "displayName": "llama.cpp · CPU",
            "formats": ["gguf"],
            "state": "available",
            "sizeBytes": 19000000,
            "recommended": false,
        });
        let parsed = parse::<RuntimeDto>(runtime.clone()).expect("parses");
        let written = serde_json::to_value(&parsed).expect("serialises");
        assert_eq!(written["description"], json!(""));
        assert_eq!(written["refs"], json!([]));
        let mut empty = runtime;
        empty["formats"] = json!([]);
        assert!(parse::<RuntimeDto>(empty).is_err());
    }
}
