use serde::{Deserialize, Serialize};

use crate::chat::ConversationDto;
use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::files::{FileDiffDto, check_optional_changes};
use crate::ids::{ConversationId, FolderGrantId, RunId, is_wire_uuid};
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, nested, no_refinements};

pub const FOLDER_PATH_MAX: usize = 32_767;
pub const FOLDER_REQUEST_MAX: usize = 64;
pub const FOLDER_ACTION_MAX: usize = 2000;
pub const FOLDER_APPROVAL_KIND: &str = "folder";

wire_enum!(FolderGrantScope {
    Always => "always",
    Conversation => "conversation",
});

wire_enum!(WorkingFolderAccess {
    Granted => "granted",
    Missing => "missing",
});

wire_enum!(FolderAccessAnswer {
    Once => "once",
    Conversation => "conversation",
    Always => "always",
    Deny => "deny",
});

pub fn check_folder_path(field: &str, value: &str) -> Result<(), AppError> {
    check::non_empty(field, value)?;
    check::max_len(field, value, FOLDER_PATH_MAX)
}

fn check_int(field: &str, value: i64) -> Result<(), AppError> {
    check::safe_integer(field, value.unsigned_abs())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderGrantChatDto {
    pub id: ConversationId,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderGrantDto {
    pub id: FolderGrantId,
    pub path: String,
    pub scope: FolderGrantScope,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub conversation: Option<FolderGrantChatDto>,
    pub bound_conversations: Vec<FolderGrantChatDto>,
    pub granted_at: Timestamp,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub last_used_at: Option<Timestamp>,
}

impl Validate for FolderGrantDto {
    fn validate(&self) -> Result<(), AppError> {
        check_folder_path("path", &self.path)?;
        if (self.scope == FolderGrantScope::Conversation) != self.conversation.is_some() {
            return Err(invalid("conversation", "scope"));
        }
        check_int("grantedAt", self.granted_at)?;
        if let Some(used) = self.last_used_at {
            check_int("lastUsedAt", used)?;
        }
        Ok(())
    }
}

pub type FoldersListOutput = Vec<FolderGrantDto>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderGrantRef {
    pub id: FolderGrantId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderRequestEntryDto {
    pub path: String,
    pub creates: Vec<String>,
}

impl Validate for FolderRequestEntryDto {
    fn validate(&self) -> Result<(), AppError> {
        check_folder_path("path", &self.path)?;
        for (index, name) in self.creates.iter().enumerate() {
            check_folder_path(&format!("creates.{index}"), name)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderApprovalDto {
    pub id: String,
    pub run_id: RunId,
    pub kind: String,
    pub subject: String,
    pub scope: String,
    pub expires_at: Timestamp,
    pub conversation_id: ConversationId,
    pub tool: String,
    pub action: String,
    pub folders: Vec<FolderRequestEntryDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub changes: Option<Vec<FileDiffDto>>,
}

impl Validate for FolderApprovalDto {
    fn validate(&self) -> Result<(), AppError> {
        if !is_wire_uuid(&self.id) {
            return Err(invalid("id", "uuid"));
        }
        if self.kind != FOLDER_APPROVAL_KIND {
            return Err(invalid("kind", "literal"));
        }
        check::non_empty("subject", &self.subject)?;
        check::non_empty("tool", &self.tool)?;
        check::max_len("action", &self.action, FOLDER_ACTION_MAX)?;
        check_int("expiresAt", self.expires_at)?;
        if self.folders.is_empty() {
            return Err(invalid("folders", "minItems"));
        }
        if self.folders.len() > FOLDER_REQUEST_MAX {
            return Err(invalid("folders", "maxItems"));
        }
        nested("folders", self.folders.validate())?;
        check_optional_changes(self.changes.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerFolderRequestInput {
    pub id: RunId,
    pub approval_id: String,
    pub answer: FolderAccessAnswer,
}

impl Validate for AnswerFolderRequestInput {
    fn validate(&self) -> Result<(), AppError> {
        if is_wire_uuid(&self.approval_id) {
            Ok(())
        } else {
            Err(invalid("approvalId", "uuid"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindFolderInput {
    pub id: ConversationId,
    pub path: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub grant: Option<FolderGrantScope>,
}

impl Validate for BindFolderInput {
    fn validate(&self) -> Result<(), AppError> {
        check_folder_path("path", &self.path)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BindFolderResultDto {
    Bound { conversation: Box<ConversationDto> },
    NeedsGrant { path: String },
}

impl Validate for BindFolderResultDto {
    fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::Bound { conversation } => nested("conversation", conversation.validate()),
            Self::NeedsGrant { path } => check_folder_path("path", path),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickFolderResultDto {
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub path: Option<String>,
}

impl Validate for PickFolderResultDto {
    fn validate(&self) -> Result<(), AppError> {
        self.path
            .as_deref()
            .map_or(Ok(()), |path| check_folder_path("path", path))
    }
}

no_refinements!(FolderGrantChatDto, FolderGrantRef);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::parse;
    use serde_json::json;

    const RUN: &str = "0199aa11-1111-7111-8111-111111111111";
    const APPROVAL: &str = "0199aa11-3333-7111-8111-111111111111";

    #[test]
    fn answers_use_the_wire_names() {
        assert_eq!(
            serde_json::to_value(FolderAccessAnswer::ALL).expect("serialises"),
            json!(["once", "conversation", "always", "deny"])
        );
    }

    #[test]
    fn a_bind_result_is_tagged_by_status() {
        let result = BindFolderResultDto::NeedsGrant {
            path: r"D:\work".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(&result).expect("serialises"),
            json!({ "status": "needsGrant", "path": r"D:\work" })
        );
    }

    #[test]
    fn an_answer_needs_a_uuid_approval() {
        assert!(
            parse::<AnswerFolderRequestInput>(
                json!({ "id": RUN, "approvalId": APPROVAL, "answer": "once" })
            )
            .is_ok()
        );
        assert!(
            parse::<AnswerFolderRequestInput>(
                json!({ "id": RUN, "approvalId": "nope", "answer": "once" })
            )
            .is_err()
        );
    }
}
