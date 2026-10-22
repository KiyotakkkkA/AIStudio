use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::folders::check_folder_path;
use crate::ids::{ConversationId, RunId, is_wire_uuid};
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, nested};

pub const FILES_APPROVAL_KIND: &str = "files";
pub const FILE_CHANGES_MAX: usize = 32;
pub const FILE_DIFF_MAX: usize = 20_000;
pub const FILES_ACTION_MAX: usize = 2000;

wire_enum!(FileChangeKind {
    Create => "create",
    Modify => "modify",
    Move => "move",
    Delete => "delete",
});

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiffDto {
    pub path: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub from: Option<String>,
    pub kind: FileChangeKind,
    pub added: u64,
    pub removed: u64,
    pub diff: String,
    pub truncated: bool,
}

impl Validate for FileDiffDto {
    fn validate(&self) -> Result<(), AppError> {
        check_folder_path("path", &self.path)?;
        match (&self.from, self.kind) {
            (Some(from), FileChangeKind::Move) => check_folder_path("from", from)?,
            (None, FileChangeKind::Move) | (Some(_), _) => return Err(invalid("from", "kind")),
            (None, _) => {}
        }
        check::safe_integer("added", self.added)?;
        check::safe_integer("removed", self.removed)?;
        check::max_len("diff", &self.diff, FILE_DIFF_MAX)
    }
}

fn check_changes(changes: &[FileDiffDto]) -> Result<(), AppError> {
    if changes.len() > FILE_CHANGES_MAX {
        return Err(invalid("changes", "maxItems"));
    }
    for (index, change) in changes.iter().enumerate() {
        nested(&format!("changes.{index}"), change.validate())?;
    }
    Ok(())
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChangesDto {
    pub changes: Vec<FileDiffDto>,
}

impl Validate for FileChangesDto {
    fn validate(&self) -> Result<(), AppError> {
        check_changes(&self.changes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesApprovalDto {
    pub id: String,
    pub run_id: RunId,
    pub kind: String,
    pub subject: String,
    pub scope: String,
    pub expires_at: Timestamp,
    pub conversation_id: ConversationId,
    pub tool: String,
    pub action: String,
    pub changes: Vec<FileDiffDto>,
}

impl Validate for FilesApprovalDto {
    fn validate(&self) -> Result<(), AppError> {
        if !is_wire_uuid(&self.id) {
            return Err(invalid("id", "uuid"));
        }
        if self.kind != FILES_APPROVAL_KIND {
            return Err(invalid("kind", "literal"));
        }
        check::non_empty("subject", &self.subject)?;
        check::non_empty("tool", &self.tool)?;
        check::max_len("action", &self.action, FILES_ACTION_MAX)?;
        check::safe_integer("expiresAt", self.expires_at.unsigned_abs())?;
        if self.changes.is_empty() {
            return Err(invalid("changes", "minItems"));
        }
        check_changes(&self.changes)
    }
}

pub(crate) fn check_optional_changes(changes: Option<&[FileDiffDto]>) -> Result<(), AppError> {
    changes.map_or(Ok(()), check_changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::parse;
    use serde_json::json;

    #[test]
    fn only_a_move_names_its_source() {
        let moved = json!({
            "path": r"D:\work\b.txt",
            "from": r"D:\work\a.txt",
            "kind": "move",
            "added": 0,
            "removed": 0,
            "diff": "",
            "truncated": false
        });
        assert!(parse::<FileDiffDto>(moved.clone()).is_ok());
        let mut created = moved;
        created["kind"] = json!("create");
        assert!(parse::<FileDiffDto>(created).is_err());
    }
}
