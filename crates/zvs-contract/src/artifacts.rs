use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::validate::{Validate, check, invalid};

pub const ARTIFACT_ID_MAX: usize = 512;
pub const ARTIFACT_NAME_MAX: usize = 255;
pub const ARTIFACT_FORMAT_MAX: usize = 16;
pub const ARTIFACT_PATH_MAX: usize = 32_767;
pub const ARTIFACT_APPLICATION_MAX: usize = 256;

fn check_id(id: &str) -> Result<(), AppError> {
    check_artifact_id("id", id)
}

pub fn check_artifact_id(field: &str, id: &str) -> Result<(), AppError> {
    check::non_empty(field, id)?;
    check::max_len(field, id, ARTIFACT_ID_MAX)?;
    let mut parts = id.split('/');
    let valid = matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(run), Some(name), None)
            if !run.is_empty()
                && !name.is_empty()
                && ![run, name].iter().any(|part| *part == "." || *part == ".." || part.contains('\\'))
    );
    if valid {
        Ok(())
    } else {
        Err(invalid(field, "format"))
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactDto {
    pub id: String,
    pub name: String,
    pub format: String,
    pub size_bytes: u64,
}

impl Validate for ArtifactDto {
    fn validate(&self) -> Result<(), AppError> {
        check_id(&self.id)?;
        check::non_empty("name", &self.name)?;
        check::max_len("name", &self.name, ARTIFACT_NAME_MAX)?;
        check::non_empty("format", &self.format)?;
        check::max_len("format", &self.format, ARTIFACT_FORMAT_MAX)?;
        check::safe_integer("sizeBytes", self.size_bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRef {
    pub id: String,
}

impl Validate for ArtifactRef {
    fn validate(&self) -> Result<(), AppError> {
        check_id(&self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactSaveResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl Validate for ArtifactSaveResult {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(path) = &self.path {
            check::non_empty("path", path)?;
            check::max_len("path", path, ARTIFACT_PATH_MAX)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactApplicationDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Validate for ArtifactApplicationDto {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(name) = &self.name {
            check::non_empty("name", name)?;
            check::max_len("name", name, ARTIFACT_APPLICATION_MAX)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::parse;
    use serde_json::json;

    #[test]
    fn an_artifact_id_is_a_run_folder_and_a_file_name() {
        assert!(parse::<ArtifactRef>(json!({ "id": "run-1/report.docx" })).is_ok());
        for id in [
            "report.docx",
            "run/../x",
            "../x",
            "a/b/c",
            "run/",
            "run\\x/y",
            "/x",
        ] {
            assert!(parse::<ArtifactRef>(json!({ "id": id })).is_err(), "{id}");
        }
    }
}
