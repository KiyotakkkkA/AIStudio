use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::ids::{ConversationId, NoteId};
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid};

pub const NOTES_FOLDER: &str = "notes";
pub const NOTES_ROOT: &str = "chat:notes/";
pub const NOTE_EXTENSION: &str = ".md";
pub const NOTE_SCHEMA_VERSION: u32 = 1;
pub const NOTE_NAME_MAX: usize = 120;
pub const NOTE_TITLE_MAX: usize = 200;
pub const NOTE_CONTEXT_MAX: usize = 2_000;
pub const NOTE_READ_WHEN_MAX: usize = 600;
pub const NOTE_CONDITION_MAX: usize = 2_000;
pub const NOTE_FILE_MAX_BYTES: usize = 128 * 1024;
pub const NOTE_TIME_MAX: usize = 40;
pub const NOTE_HASH_LEN: usize = 64;
pub const NOTE_ERROR_MAX: usize = 500;
pub const NOTES_LIST_MAX: usize = 1_000;

pub fn is_note_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some(stem) = lower.strip_suffix(NOTE_EXTENSION) else {
        return false;
    };
    !stem.trim().is_empty()
        && name.encode_utf16().count() <= NOTE_NAME_MAX
        && !name.starts_with(['.', ' '])
        && !stem.ends_with(['.', ' '])
        && !name
            .chars()
            .any(|character| character.is_control() || "<>\"|?*:/\\".contains(character))
}

pub fn note_path(name: &str) -> String {
    format!("{NOTES_ROOT}{name}")
}

pub fn check_note_name(field: &str, name: &str) -> Result<(), AppError> {
    if is_note_name(name) {
        Ok(())
    } else {
        Err(invalid(field, "noteName"))
    }
}

fn check_note_path(field: &str, path: &str) -> Result<(), AppError> {
    match path.strip_prefix(NOTES_ROOT) {
        Some(name) if is_note_name(name) => Ok(()),
        _ => Err(invalid(field, "notePath")),
    }
}

pub fn check_note_hash(field: &str, hash: &str) -> Result<(), AppError> {
    if hash.len() == NOTE_HASH_LEN
        && hash
            .chars()
            .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
    {
        Ok(())
    } else {
        Err(invalid(field, "hash"))
    }
}

fn check_time(field: &str, value: &str) -> Result<(), AppError> {
    check::non_empty(field, value)?;
    check::max_len(field, value, NOTE_TIME_MAX)
}

fn check_error(error: Option<&str>) -> Result<(), AppError> {
    error.map_or(Ok(()), |error| {
        check::non_empty("error", error)?;
        check::max_len("error", error, NOTE_ERROR_MAX)
    })
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSourceDto {
    pub id: NoteId,
    pub title: String,
    pub path: String,
    pub hash: String,
}

impl Validate for NoteSourceDto {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("title", &self.title, NOTE_TITLE_MAX)?;
        check_note_path("path", &self.path)?;
        check_note_hash("hash", &self.hash)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteMetaDto {
    pub id: NoteId,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub context: String,
    pub read_when: String,
    pub update_when: String,
    pub delete_when: String,
}

impl Validate for NoteMetaDto {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("title", &self.title, NOTE_TITLE_MAX)?;
        check_time("createdAt", &self.created_at)?;
        check_time("updatedAt", &self.updated_at)?;
        check::trimmed_name("context", &self.context, NOTE_CONTEXT_MAX)?;
        check::trimmed_name("readWhen", &self.read_when, NOTE_READ_WHEN_MAX)?;
        check::trimmed_name("updateWhen", &self.update_when, NOTE_CONDITION_MAX)?;
        check::trimmed_name("deleteWhen", &self.delete_when, NOTE_CONDITION_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSummaryDto {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified_at: Timestamp,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub id: Option<NoteId>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub title: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub updated_at: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub error: Option<String>,
}

impl Validate for NoteSummaryDto {
    fn validate(&self) -> Result<(), AppError> {
        check_note_name("name", &self.name)?;
        check_note_path("path", &self.path)?;
        check::safe_integer("size", self.size)?;
        check::timestamp("modifiedAt", self.modified_at)?;
        if let Some(title) = &self.title {
            check::trimmed_name("title", title, NOTE_TITLE_MAX)?;
        }
        if let Some(updated_at) = &self.updated_at {
            check_time("updatedAt", updated_at)?;
        }
        if self.error.is_some() == self.id.is_some() {
            return Err(invalid("error", "noteState"));
        }
        check_error(self.error.as_deref())
    }
}

pub type NotesListOutput = Vec<NoteSummaryDto>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDto {
    pub name: String,
    pub path: String,
    pub hash: String,
    pub content: String,
    pub modified_at: Timestamp,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub meta: Option<NoteMetaDto>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub body: Option<String>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub error: Option<String>,
}

impl Validate for NoteDto {
    fn validate(&self) -> Result<(), AppError> {
        check_note_name("name", &self.name)?;
        check_note_path("path", &self.path)?;
        check_note_hash("hash", &self.hash)?;
        check::timestamp("modifiedAt", self.modified_at)?;
        if self.meta.is_some() != self.body.is_some() || self.meta.is_some() == self.error.is_some()
        {
            return Err(invalid("error", "noteState"));
        }
        if let Some(meta) = &self.meta {
            crate::validate::nested("meta", meta.validate())?;
        }
        check_error(self.error.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteRef {
    pub conversation_id: ConversationId,
    pub name: String,
}

impl Validate for NoteRef {
    fn validate(&self) -> Result<(), AppError> {
        check_note_name("name", &self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteReadInput {
    pub conversation_id: ConversationId,
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
    pub id: Option<NoteId>,
}

impl Validate for NoteReadInput {
    fn validate(&self) -> Result<(), AppError> {
        if self.name.is_some() == self.id.is_some() {
            return Err(invalid("name", "oneOf"));
        }
        self.name
            .as_deref()
            .map_or(Ok(()), |name| check_note_name("name", name))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSaveInput {
    pub conversation_id: ConversationId,
    pub name: String,
    pub content: String,
    pub expected_hash: String,
}

impl Validate for NoteSaveInput {
    fn validate(&self) -> Result<(), AppError> {
        check_note_name("name", &self.name)?;
        check::max_len("content", &self.content, NOTE_FILE_MAX_BYTES)?;
        check_note_hash("expectedHash", &self.expected_hash)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteRenameInput {
    pub conversation_id: ConversationId,
    pub name: String,
    pub to: String,
}

impl Validate for NoteRenameInput {
    fn validate(&self) -> Result<(), AppError> {
        check_note_name("name", &self.name)?;
        check_note_name("to", &self.to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_names_are_flat_markdown_topic_slugs() {
        for good in ["process-shutdown.md", "Отпуск.MD", "a b.md"] {
            assert!(is_note_name(good), "{good}");
        }
        for bad in [
            "",
            ".md",
            "notes.txt",
            "sub/topic.md",
            "sub\\topic.md",
            ".hidden.md",
            "topic..md",
            "a:b.md",
            " topic.md",
        ] {
            assert!(!is_note_name(bad), "{bad}");
        }
        assert!(!is_note_name(&format!("{}.md", "a".repeat(NOTE_NAME_MAX))));
    }

    #[test]
    fn a_read_names_exactly_one_target() {
        let conversation_id =
            ConversationId::parse("0199aa11-1111-7111-8111-111111111111").expect("an id");
        let both = NoteReadInput {
            conversation_id,
            name: Some("a.md".to_owned()),
            id: NoteId::parse("0199aa11-2222-7111-8111-000000000001"),
        };
        assert!(both.validate().is_err());
        let neither = NoteReadInput {
            conversation_id,
            name: None,
            id: None,
        };
        assert!(neither.validate().is_err());
    }
}
