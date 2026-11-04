use serde::{Deserialize, Serialize};

use crate::browser::BrowserAppearance;
use crate::chat::{CHAT_TEXT_MAX, ChatToolCallStatus};
use crate::errors::AppError;
use crate::folders::{FOLDER_PATH_MAX, FolderAccessAnswer, FolderGrantChatDto, FolderGrantScope};
use crate::ids::{RunId, is_wire_uuid};
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid};

pub const APPS_APPROVAL_KIND: &str = "apps";
pub const APP_NAME_MAX: usize = 128;
pub const APP_ICON_MAX: usize = 262_144;

fn uuid(field: &str, id: &str) -> Result<(), AppError> {
    if is_wire_uuid(id) {
        Ok(())
    } else {
        Err(invalid(field, "uuid"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerAppDto {
    pub app: String,
    pub path: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppGrantDto {
    pub id: String,
    pub app: String,
    pub path: String,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub icon: Option<String>,
    pub scope: FolderGrantScope,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub conversation: Option<FolderGrantChatDto>,
    pub granted_at: Timestamp,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub last_used_at: Option<Timestamp>,
}

impl Validate for AppGrantDto {
    fn validate(&self) -> Result<(), AppError> {
        uuid("id", &self.id)?;
        check::non_empty("app", &self.app)?;
        check::max_len("app", &self.app, APP_NAME_MAX)?;
        check::max_len("path", &self.path, FOLDER_PATH_MAX)?;
        if let Some(icon) = &self.icon {
            check::max_len("icon", icon, APP_ICON_MAX)?;
        }
        if (self.scope == FolderGrantScope::Conversation) != self.conversation.is_some() {
            return Err(invalid("conversation", "scope"));
        }
        check::safe_integer("grantedAt", self.granted_at.unsigned_abs())?;
        if let Some(used) = self.last_used_at {
            check::safe_integer("lastUsedAt", used.unsigned_abs())?;
        }
        Ok(())
    }
}

pub type AppGrantsListOutput = Vec<AppGrantDto>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppGrantRef {
    pub id: String,
}

impl Validate for AppGrantRef {
    fn validate(&self) -> Result<(), AppError> {
        uuid("id", &self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerAppRequestInput {
    pub id: RunId,
    pub approval_id: String,
    pub answer: FolderAccessAnswer,
}

impl Validate for AnswerAppRequestInput {
    fn validate(&self) -> Result<(), AppError> {
        uuid("approvalId", &self.approval_id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum ComputerCommand {
    State,
    Stop,
    Approve {
        id: String,
    },
    Deny {
        id: String,
    },
    Answer {
        id: String,
        answer: FolderAccessAnswer,
    },
    Say {
        text: String,
    },
    Engage,
    Release,
    Collapse {
        collapsed: bool,
    },
}

impl Validate for ComputerCommand {
    fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::Approve { id } | Self::Deny { id } | Self::Answer { id, .. } => uuid("id", id),
            Self::Say { text } => {
                check::non_empty("text", text.trim())?;
                check::max_len("text", text, CHAT_TEXT_MAX)
            }
            Self::State | Self::Stop | Self::Engage | Self::Release | Self::Collapse { .. } => {
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComputerStatus {
    #[default]
    Idle,
    Starting,
    Working,
    Awaiting,
    Stopping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComputerActionState {
    Running,
    Waiting,
    Done,
    Failed,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerAction {
    pub id: u64,
    pub tool: String,
    pub label: String,
    pub state: ComputerActionState,
    pub started_at: Timestamp,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerApproval {
    pub id: String,
    pub kind: String,
    pub tool: String,
    pub action: String,
    pub apps: Vec<ComputerAppDto>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerDisplay {
    pub x: i64,
    pub y: i64,
    pub width: u64,
    pub height: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerState {
    pub active: bool,
    pub status: ComputerStatus,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub conversation_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub title: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub display: Option<ComputerDisplay>,
    pub apps: Vec<String>,
    pub actions: Vec<ComputerAction>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub approval: Option<ComputerApproval>,
    pub collapsed: bool,
    pub appearance: BrowserAppearance,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub started_at: Option<Timestamp>,
    #[serde(default)]
    pub reply: String,
    #[serde(default)]
    pub calls: Vec<ComputerCall>,
    #[serde(default)]
    pub requests: Vec<ComputerRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerCall {
    pub id: String,
    pub key: String,
    pub status: ChatToolCallStatus,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerRequest {
    pub id: String,
    pub kind: String,
    pub tool: String,
    pub action: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub reason: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerPointer {
    pub x: i64,
    pub y: i64,
    pub down: bool,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    #[test]
    fn panel_commands_are_tagged_by_action_and_validated() {
        let say = parse::<ComputerCommand>(json!({ "action": "say", "text": "Открой отчёт" }))
            .expect("a correction parses");
        assert_eq!(
            say,
            ComputerCommand::Say {
                text: "Открой отчёт".to_owned()
            }
        );
        assert!(parse::<ComputerCommand>(json!({ "action": "say", "text": "  " })).is_err());
        assert!(parse::<ComputerCommand>(json!({ "action": "approve", "id": "nope" })).is_err());
        assert!(parse::<ComputerCommand>(json!({ "action": "launch" })).is_err());
    }

    #[test]
    fn the_state_omits_absent_fields() {
        let value = serde_json::to_value(ComputerState::default()).expect("serialises");
        assert_eq!(
            value,
            json!({
                "active": false,
                "status": "idle",
                "apps": [],
                "actions": [],
                "collapsed": false,
                "appearance": { "theme": "system", "darkPalette": "zvs-dark", "lightPalette": "zvs-light" },
                "reply": "",
                "calls": [],
                "requests": []
            })
        );
    }
}
