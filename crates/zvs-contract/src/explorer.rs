use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::{ConversationId, StreamId};
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, nested, no_refinements};

pub const EXPLORER_PATH_MAX: usize = 32_767;
pub const EXPLORER_LIST_MAX: usize = 5000;
pub const EXPLORER_TEXT_MAX: usize = 1_048_576;
pub const EXPLORER_CHANGES_MAX: usize = 500;
pub const EXPLORER_STEP_DOMAIN: &str = "workspace";

wire_enum!(ExplorerEntryKind {
    File => "file",
    Folder => "folder",
});

wire_enum!(ExplorerChangeKind {
    Created => "created",
    Modified => "modified",
    Removed => "removed",
});

fn check_relative(field: &str, value: &str) -> Result<(), AppError> {
    check::max_len(field, value, EXPLORER_PATH_MAX)
}

fn check_modified(value: Option<Timestamp>) -> Result<(), AppError> {
    value.map_or(Ok(()), |value| check::timestamp("modifiedAt", value))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerEntryDto {
    pub name: String,
    pub path: String,
    pub kind: ExplorerEntryKind,
    pub size: u64,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub modified_at: Option<Timestamp>,
    pub ignored: bool,
}

impl Validate for ExplorerEntryDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("name", &self.name)?;
        check::non_empty("path", &self.path)?;
        check_relative("path", &self.path)?;
        check::safe_integer("size", self.size)?;
        check_modified(self.modified_at)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerListInput {
    pub id: ConversationId,
    pub path: String,
}

impl Validate for ExplorerListInput {
    fn validate(&self) -> Result<(), AppError> {
        check_relative("path", &self.path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerListingDto {
    pub path: String,
    pub entries: Vec<ExplorerEntryDto>,
    pub truncated: bool,
}

impl Validate for ExplorerListingDto {
    fn validate(&self) -> Result<(), AppError> {
        check_relative("path", &self.path)?;
        if self.entries.len() > EXPLORER_LIST_MAX {
            return Err(invalid("entries", "maxItems"));
        }
        nested("entries", self.entries.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerReadInput {
    pub id: ConversationId,
    pub path: String,
}

impl Validate for ExplorerReadInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("path", &self.path)?;
        check_relative("path", &self.path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerFileDto {
    pub path: String,
    pub size: u64,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub modified_at: Option<Timestamp>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub text: Option<String>,
    pub truncated: bool,
}

impl Validate for ExplorerFileDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("path", &self.path)?;
        check_relative("path", &self.path)?;
        check::safe_integer("size", self.size)?;
        check_modified(self.modified_at)?;
        if let Some(text) = &self.text {
            check::max_len("text", text, EXPLORER_TEXT_MAX)?;
        }
        if self.truncated && self.text.is_none() {
            return Err(invalid("truncated", "text"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerChangeDto {
    pub path: String,
    pub kind: ExplorerChangeKind,
}

impl Validate for ExplorerChangeDto {
    fn validate(&self) -> Result<(), AppError> {
        check_relative("path", &self.path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerChangesDto {
    pub domain: String,
    pub conversation_id: ConversationId,
    pub changes: Vec<ExplorerChangeDto>,
}

impl Validate for ExplorerChangesDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.domain != EXPLORER_STEP_DOMAIN {
            return Err(invalid("domain", "literal"));
        }
        if self.changes.is_empty() {
            return Err(invalid("changes", "minItems"));
        }
        if self.changes.len() > EXPLORER_CHANGES_MAX {
            return Err(invalid("changes", "maxItems"));
        }
        nested("changes", self.changes.validate())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerWatchDto {
    pub stream_id: StreamId,
}

no_refinements!(ExplorerWatchDto);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::parse;
    use serde_json::json;

    const CONVERSATION: &str = "0199aa11-1111-7111-8111-111111111111";

    #[test]
    fn the_root_is_an_empty_path() {
        assert!(parse::<ExplorerListInput>(json!({ "id": CONVERSATION, "path": "" })).is_ok());
        assert!(parse::<ExplorerReadInput>(json!({ "id": CONVERSATION, "path": "" })).is_err());
    }

    #[test]
    fn changes_name_their_domain() {
        let changes = ExplorerChangesDto {
            domain: EXPLORER_STEP_DOMAIN.to_owned(),
            conversation_id: ConversationId::parse(CONVERSATION).expect("uuid"),
            changes: vec![ExplorerChangeDto {
                path: "docs/PLAN.md".to_owned(),
                kind: ExplorerChangeKind::Created,
            }],
        };
        assert_eq!(
            serde_json::to_value(&changes).expect("serialises"),
            json!({
                "domain": "workspace",
                "conversationId": CONVERSATION,
                "changes": [{ "path": "docs/PLAN.md", "kind": "created" }],
            })
        );
        assert!(changes.validate().is_ok());
    }

    #[test]
    fn a_binary_file_is_never_truncated() {
        assert!(
            parse::<ExplorerFileDto>(json!({
                "path": "a.bin",
                "size": 4,
                "modifiedAt": null,
                "text": null,
                "truncated": true,
            }))
            .is_err()
        );
    }
}
