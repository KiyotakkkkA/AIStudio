use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::{ProviderId, SkillId};
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, js_trim, nested, no_refinements};

wire_enum!(SkillSource {
    Local => "local",
    Imported => "imported",
});

wire_enum!(SkillStatus {
    Ready => "ready",
    Invalid => "invalid",
});

wire_enum!(SkillMatchMethod {
    Embedding => "embedding",
    Lexical => "lexical",
});

wire_enum!(SkillFileEncoding {
    Utf8 => "utf8",
    Base64 => "base64",
});

wire_enum!(SkillIssueRule {
    Required => "required",
    Pattern => "pattern",
    TooLong => "tooLong",
    Duplicate => "duplicate",
    UnknownTool => "unknownTool",
    ToolOff => "toolOff",
    ToolDisabled => "toolDisabled",
    ToolUnavailable => "toolUnavailable",
    Path => "path",
    FileTooLarge => "fileTooLarge",
    SkillTooLarge => "skillTooLarge",
    TooManyFiles => "tooManyFiles",
    Encoding => "encoding",
    Manifest => "manifest",
    Missing => "missing",
});

pub const SKILL_KEY_MAX: usize = 64;
pub const SKILL_NAME_MAX: usize = 120;
pub const SKILL_DESCRIPTION_MAX: usize = 1024;
pub const SKILL_INSTRUCTIONS_MAX: usize = 100_000;
pub const SKILL_ALLOWED_TOOLS_MAX: usize = 64;
pub const SKILL_PATH_MAX: usize = 240;
pub const SKILL_FILES_MAX: usize = 200;
pub const SKILL_FILE_MAX_BYTES: u64 = 1024 * 1024;
pub const SKILL_TOTAL_MAX_BYTES: u64 = 10 * 1024 * 1024;
pub const SKILL_MESSAGE_MAX: usize = 8000;
pub const SKILL_MODEL_MAX: usize = 256;
pub const SKILL_ISSUES_MAX: usize = 64;
pub const SKILL_FILE_DIRS: &[&str] = &["references", "scripts", "assets"];
pub const DEFAULT_SKILL_THRESHOLD: f64 = 0.5;
pub const DEFAULT_SKILL_CATALOGUE_LIMIT: u64 = 20;
pub const SKILL_CATALOGUE_MAX: u64 = 200;
pub const SKILL_TRIGGER_KEY: &str = "skills.trigger";

pub fn is_skill_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= SKILL_KEY_MAX
        && value.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
        && value.as_bytes()[0].is_ascii_lowercase()
}

pub fn is_skill_path(value: &str) -> bool {
    if value.is_empty() || value.chars().count() > SKILL_PATH_MAX {
        return false;
    }
    let segments: Vec<&str> = value.split('/').collect();
    segments.len() >= 2
        && SKILL_FILE_DIRS.contains(&segments[0])
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && !segment.starts_with('.')
                && !segment.ends_with(['.', ' '])
                && !segment.chars().any(|character| {
                    character.is_control()
                        || matches!(character, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
                })
        })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillModelDto {
    pub provider_id: ProviderId,
    pub model_id: String,
}

impl Validate for SkillModelDto {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("modelId", &self.model_id, SKILL_MODEL_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillIssueDto {
    pub field: String,
    pub rule: SkillIssueRule,
}

impl Validate for SkillIssueDto {
    fn validate(&self) -> Result<(), AppError> {
        check::max_len("field", &self.field, 300)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFileDto {
    pub path: String,
    pub size: u64,
}

impl Validate for SkillFileDto {
    fn validate(&self) -> Result<(), AppError> {
        if !is_skill_path(&self.path) {
            return Err(invalid("path", "pattern"));
        }
        check::safe_integer("size", self.size)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDto {
    pub id: SkillId,
    pub key: String,
    pub name: String,
    pub description: String,
    pub allowed_tools: Vec<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub model_override: Option<SkillModelDto>,
    pub enabled: bool,
    pub source: SkillSource,
    pub status: SkillStatus,
    pub issues: Vec<SkillIssueDto>,
    pub file_count: u64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Validate for SkillDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("key", &self.key)?;
        check::max_len("key", &self.key, SKILL_KEY_MAX)?;
        check::max_len("name", &self.name, SKILL_NAME_MAX)?;
        check::max_len("description", &self.description, SKILL_DESCRIPTION_MAX)?;
        check_tool_list("allowedTools", &self.allowed_tools)?;
        nested("modelOverride", self.model_override.validate())?;
        if self.issues.len() > SKILL_ISSUES_MAX {
            return Err(invalid("issues", "maxLength"));
        }
        nested("issues", self.issues.validate())?;
        check::safe_integer("fileCount", self.file_count)?;
        check::timestamp("createdAt", self.created_at)?;
        check::timestamp("updatedAt", self.updated_at)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDetailDto {
    #[serde(flatten)]
    pub skill: SkillDto,
    pub instructions: String,
    pub files: Vec<SkillFileDto>,
}

impl Validate for SkillDetailDto {
    fn validate(&self) -> Result<(), AppError> {
        self.skill.validate()?;
        check::max_len("instructions", &self.instructions, SKILL_INSTRUCTIONS_MAX)?;
        if self.files.len() > SKILL_FILES_MAX {
            return Err(invalid("files", "maxLength"));
        }
        nested("files", self.files.validate())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRef {
    pub id: SkillId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFileInput {
    pub path: String,
    pub content: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub encoding: Option<SkillFileEncoding>,
}

impl SkillFileInput {
    pub fn encoding(&self) -> SkillFileEncoding {
        self.encoding.unwrap_or(SkillFileEncoding::Utf8)
    }
}

impl Validate for SkillFileInput {
    fn validate(&self) -> Result<(), AppError> {
        check::max_len("path", &self.path, SKILL_PATH_MAX)
    }
}

fn check_tool_list(field: &str, tools: &[String]) -> Result<(), AppError> {
    if tools.len() > SKILL_ALLOWED_TOOLS_MAX {
        return Err(invalid(field, "maxLength"));
    }
    for (index, tool) in tools.iter().enumerate() {
        check::max_len(&format!("{field}.{index}"), tool, 128)?;
    }
    Ok(())
}

fn check_files(field: &str, files: &Vec<SkillFileInput>) -> Result<(), AppError> {
    if files.len() > SKILL_FILES_MAX {
        return Err(invalid(field, "maxLength"));
    }
    nested(field, files.validate())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSkillInput {
    pub key: String,
    pub name: String,
    pub description: String,
    pub instructions: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::optional_nullable"
    )]
    pub model_override: Option<Option<SkillModelDto>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub enabled: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub files: Option<Vec<SkillFileInput>>,
}

impl Validate for CreateSkillInput {
    fn validate(&self) -> Result<(), AppError> {
        check::max_len("key", &self.key, SKILL_KEY_MAX)?;
        check::max_len("name", &self.name, SKILL_NAME_MAX)?;
        check::max_len("description", &self.description, SKILL_DESCRIPTION_MAX)?;
        check::max_len("instructions", &self.instructions, SKILL_INSTRUCTIONS_MAX)?;
        if let Some(tools) = &self.allowed_tools {
            check_tool_list("allowedTools", tools)?;
        }
        if let Some(Some(model)) = &self.model_override {
            nested("modelOverride", model.validate())?;
        }
        if let Some(files) = &self.files {
            check_files("files", files)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSkillInput {
    pub id: SkillId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub key: Option<String>,
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
    pub description: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub instructions: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::optional_nullable"
    )]
    pub model_override: Option<Option<SkillModelDto>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub enabled: Option<bool>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub files: Option<Vec<SkillFileInput>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub remove_files: Option<Vec<String>>,
}

impl Validate for UpdateSkillInput {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(key) = &self.key {
            check::max_len("key", key, SKILL_KEY_MAX)?;
        }
        if let Some(name) = &self.name {
            check::max_len("name", name, SKILL_NAME_MAX)?;
        }
        if let Some(description) = &self.description {
            check::max_len("description", description, SKILL_DESCRIPTION_MAX)?;
        }
        if let Some(instructions) = &self.instructions {
            check::max_len("instructions", instructions, SKILL_INSTRUCTIONS_MAX)?;
        }
        if let Some(tools) = &self.allowed_tools {
            check_tool_list("allowedTools", tools)?;
        }
        if let Some(Some(model)) = &self.model_override {
            nested("modelOverride", model.validate())?;
        }
        if let Some(files) = &self.files {
            check_files("files", files)?;
        }
        if let Some(paths) = &self.remove_files {
            if paths.len() > SKILL_FILES_MAX {
                return Err(invalid("removeFiles", "maxLength"));
            }
            for (index, path) in paths.iter().enumerate() {
                check::max_len(&format!("removeFiles.{index}"), path, SKILL_PATH_MAX)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillTestInput {
    pub message: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub id: Option<SkillId>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub description: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub threshold: Option<f64>,
}

impl Validate for SkillTestInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("message", js_trim(&self.message))?;
        check::max_len("message", &self.message, SKILL_MESSAGE_MAX)?;
        if let Some(description) = &self.description {
            check::max_len("description", description, SKILL_DESCRIPTION_MAX)?;
        }
        if let Some(threshold) = self.threshold {
            check::number_range("threshold", threshold, 0.0, 1.0)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMatchDto {
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub id: Option<SkillId>,
    pub key: String,
    pub name: String,
    pub score: f64,
    pub triggered: bool,
}

impl Validate for SkillMatchDto {
    fn validate(&self) -> Result<(), AppError> {
        check::max_len("key", &self.key, SKILL_KEY_MAX)?;
        check::max_len("name", &self.name, SKILL_NAME_MAX)?;
        check::number_range("score", self.score, -1.0, 1.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillTestResultDto {
    pub method: SkillMatchMethod,
    pub threshold: f64,
    pub matches: Vec<SkillMatchDto>,
}

impl Validate for SkillTestResultDto {
    fn validate(&self) -> Result<(), AppError> {
        check::number_range("threshold", self.threshold, 0.0, 1.0)?;
        nested("matches", self.matches.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRunInput {
    pub id: SkillId,
    pub message: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub provider_id: Option<ProviderId>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub model_id: Option<String>,
}

impl Validate for SkillRunInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("message", js_trim(&self.message))?;
        check::max_len("message", &self.message, SKILL_MESSAGE_MAX)?;
        if let Some(model) = &self.model_id {
            check::trimmed_name("modelId", model, SKILL_MODEL_MAX)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillImportInput {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub path: Option<String>,
}

impl Validate for SkillImportInput {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(path) = &self.path {
            check::trimmed_name("path", path, 4096)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillImportResultDto {
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub skill: Option<SkillDetailDto>,
}

impl Validate for SkillImportResultDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("skill", self.skill.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillExportInput {
    pub id: SkillId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub path: Option<String>,
}

impl Validate for SkillExportInput {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(path) = &self.path {
            check::trimmed_name("path", path, 4096)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillExportResultDto {
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub path: Option<String>,
}

impl Validate for SkillExportResultDto {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(path) = &self.path {
            check::max_len("path", path, 4096)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRemoveOutput {
    pub id: SkillId,
    pub removed: bool,
}

impl Validate for SkillRemoveOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::literal_true("removed", self.removed)
    }
}

const fn default_threshold() -> f64 {
    DEFAULT_SKILL_THRESHOLD
}

const fn default_catalogue_limit() -> u64 {
    DEFAULT_SKILL_CATALOGUE_LIMIT
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillTriggerSettingsDto {
    #[serde(default = "default_threshold")]
    pub threshold: f64,
    #[serde(default = "default_catalogue_limit")]
    pub catalogue_limit: u64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub embedding: Option<SkillModelDto>,
}

impl Default for SkillTriggerSettingsDto {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_SKILL_THRESHOLD,
            catalogue_limit: DEFAULT_SKILL_CATALOGUE_LIMIT,
            embedding: None,
        }
    }
}

impl Validate for SkillTriggerSettingsDto {
    fn validate(&self) -> Result<(), AppError> {
        check::number_range("threshold", self.threshold, 0.0, 1.0)?;
        check::int_range(
            "catalogueLimit",
            self.catalogue_limit,
            1,
            SKILL_CATALOGUE_MAX,
        )?;
        nested("embedding", self.embedding.validate())
    }
}

no_refinements!(SkillRef);

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    #[test]
    fn a_skill_key_is_a_lowercase_slug() {
        for key in ["pdf", "release-notes", "a1-b2", "x"] {
            assert!(is_skill_key(key), "{key}");
        }
        for key in [
            "",
            "Pdf",
            "1pdf",
            "pdf-",
            "-pdf",
            "pdf--notes",
            "pdf_notes",
            "pdf notes",
            "пдф",
        ] {
            assert!(!is_skill_key(key), "{key}");
        }
        assert!(!is_skill_key(&"a".repeat(SKILL_KEY_MAX + 1)));
    }

    #[test]
    fn a_skill_path_stays_inside_a_resource_folder() {
        for path in [
            "references/guide.md",
            "scripts/run.py",
            "assets/logo.png",
            "references/deep/notes.txt",
        ] {
            assert!(is_skill_path(path), "{path}");
        }
        for path in [
            "SKILL.md",
            "guide.md",
            "references",
            "references/",
            "references/../SKILL.md",
            "references/./a.md",
            "other/a.md",
            "/references/a.md",
            "references\\a.md",
            "references/a:b.md",
            "references/.hidden",
            "references//a.md",
            "references/a.",
        ] {
            assert!(!is_skill_path(path), "{path}");
        }
    }

    #[test]
    fn the_trigger_settings_apply_their_defaults_and_bounds() {
        let settings = parse::<SkillTriggerSettingsDto>(json!({})).expect("parses");
        assert_eq!(settings, SkillTriggerSettingsDto::default());
        assert!(parse::<SkillTriggerSettingsDto>(json!({ "threshold": 1.5 })).is_err());
        assert!(parse::<SkillTriggerSettingsDto>(json!({ "catalogueLimit": 0 })).is_err());
        assert!(parse::<SkillTriggerSettingsDto>(json!({ "embedding": null })).is_err());
    }

    #[test]
    fn a_test_needs_a_message_and_a_threshold_inside_the_unit_range() {
        assert!(parse::<SkillTestInput>(json!({ "message": "  " })).is_err());
        assert!(parse::<SkillTestInput>(json!({ "message": "a", "threshold": -0.1 })).is_err());
        let input = parse::<SkillTestInput>(json!({ "message": "a" })).expect("parses");
        assert_eq!(input.threshold, None);
    }
}
