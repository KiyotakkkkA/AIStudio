use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::chat::CONVERSATION_TITLE_MAX;
use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::events::HostEvent;
use crate::exec_policy::{AllowRuleScope, ExecMode};
use crate::folders::{WorkingFolderAccess, check_folder_path};
use crate::ids::{ConversationId, is_wire_uuid};
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, nested, no_refinements};

pub const CLI_COMMAND: &str = "aistudio";
pub const CLI_PROTOCOL_VERSION: u32 = 2;
pub const CLI_LINE_MAX: usize = 8 * 1024 * 1024;
pub const CLI_PIPE_PREFIX: &str = r"\\.\pipe\zvs-ai-studio-cli-";
pub const CLI_LAUNCH_ARG: &str = "--cli";
pub const CLI_THEME_FILE: &str = "theme.json";
pub const CLI_STEP_DOMAIN: &str = "cli";
pub const CLI_IMAGE_MAX: usize = 4096;
pub const CLI_CONFIRM_SETTING_KEY: &str = "cli.confirmLoosening";

pub fn cli_pipe_name(user_sid: &str) -> String {
    format!("{CLI_PIPE_PREFIX}{user_sid}")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CliClientMessage {
    Hello {
        version: u32,
    },
    Call {
        id: u64,
        channel: String,
        #[serde(default)]
        payload: Value,
    },
    FocusVerification,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CliNotice {
    AccountVerification { origin: String, shown: bool },
    Consent { pending: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CliServerMessage {
    Welcome { version: u32 },
    Result { id: u64, envelope: Value },
    Event { event: HostEvent },
    Notice { notice: CliNotice },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliOpenInput {
    pub cwd: String,
}

impl Validate for CliOpenInput {
    fn validate(&self) -> Result<(), AppError> {
        check_folder_path("cwd", &self.cwd)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliSessionDto {
    pub folder: String,
    pub access: WorkingFolderAccess,
}

impl Validate for CliSessionDto {
    fn validate(&self) -> Result<(), AppError> {
        check_folder_path("folder", &self.folder)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliInstallDto {
    pub available: bool,
    pub installed: bool,
    pub command: String,
}

impl Validate for CliInstallDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("command", &self.command)?;
        if self.installed && !self.available {
            return Err(invalid("installed", "available"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CliChangeDto {
    Mode {
        from: ExecMode,
        to: ExecMode,
    },
    Judge,
    AllowRule {
        scope: AllowRuleScope,
    },
    AlwaysApproval,
    AlwaysFolder,
    OpenDialog {
        conversation_id: ConversationId,
        title: String,
    },
}

impl Validate for CliChangeDto {
    fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::OpenDialog { title, .. } => {
                check::max_len("title", title, CONVERSATION_TITLE_MAX)
            }
            _ => Ok(()),
        }
    }
}

fn check_image(image: Option<&String>) -> Result<(), AppError> {
    image.map_or(Ok(()), |image| {
        check::non_empty("image", image)?;
        check::max_len("image", image, CLI_IMAGE_MAX)
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliConnectionDto {
    pub id: u64,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub pid: Option<u32>,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub image: Option<String>,
    pub recognised: bool,
    pub connected_at: Timestamp,
    pub dialogs: u32,
}

impl Validate for CliConnectionDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("id", self.id)?;
        check::timestamp("connectedAt", self.connected_at)?;
        check_image(self.image.as_ref())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliConnectionRef {
    pub id: u64,
}

impl Validate for CliConnectionRef {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("id", self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliConsentDto {
    pub id: String,
    pub connection_id: u64,
    pub change: CliChangeDto,
    pub recognised: bool,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub image: Option<String>,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
}

impl Validate for CliConsentDto {
    fn validate(&self) -> Result<(), AppError> {
        if !is_wire_uuid(&self.id) {
            return Err(invalid("id", "uuid"));
        }
        check::safe_integer("connectionId", self.connection_id)?;
        nested("change", self.change.validate())?;
        check_image(self.image.as_ref())?;
        check::timestamp("createdAt", self.created_at)?;
        check::timestamp("expiresAt", self.expires_at)?;
        if self.expires_at < self.created_at {
            return Err(invalid("expiresAt", "order"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliConsentAnswerInput {
    pub id: String,
    pub confirm: bool,
}

impl Validate for CliConsentAnswerInput {
    fn validate(&self) -> Result<(), AppError> {
        if is_wire_uuid(&self.id) {
            Ok(())
        } else {
            Err(invalid("id", "uuid"))
        }
    }
}

wire_enum!(CliTheme {
    Ember => "ember",
    Graphite => "graphite",
    Moss => "moss",
    Dusk => "dusk",
    Frost => "frost",
});

#[allow(clippy::derivable_impls)]
impl Default for CliTheme {
    fn default() -> Self {
        Self::Ember
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CliThemeDto {
    pub theme: CliTheme,
}

no_refinements!(CliThemeDto);

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    #[test]
    fn the_pipe_is_named_after_the_user() {
        assert_eq!(
            cli_pipe_name("S-1-5-21-1"),
            r"\\.\pipe\zvs-ai-studio-cli-S-1-5-21-1"
        );
    }

    #[test]
    fn protocol_messages_are_tagged_by_type() {
        let call: CliClientMessage =
            serde_json::from_value(json!({ "type": "call", "id": 7, "channel": "system.ping" }))
                .expect("parses");
        assert_eq!(
            call,
            CliClientMessage::Call {
                id: 7,
                channel: "system.ping".to_owned(),
                payload: Value::Null,
            }
        );
        assert_eq!(
            serde_json::to_value(CliServerMessage::Welcome { version: 1 }).expect("serialises"),
            json!({ "type": "welcome", "version": 1 })
        );
    }

    #[test]
    fn a_verification_notice_names_the_origin() {
        let notice = CliServerMessage::Notice {
            notice: CliNotice::AccountVerification {
                origin: "https://chat.qwen.ai".to_owned(),
                shown: true,
            },
        };
        assert_eq!(
            serde_json::to_value(notice).expect("serialises"),
            json!({
                "type": "notice",
                "notice": { "kind": "accountVerification", "origin": "https://chat.qwen.ai", "shown": true }
            })
        );
        assert_eq!(
            serde_json::from_value::<CliClientMessage>(json!({ "type": "focusVerification" }))
                .expect("parses"),
            CliClientMessage::FocusVerification
        );
    }

    #[test]
    fn a_theme_is_named_on_the_wire_and_defaults_to_ember() {
        assert_eq!(
            serde_json::to_value(CliThemeDto {
                theme: CliTheme::Graphite
            })
            .expect("serialises"),
            json!({ "theme": "graphite" })
        );
        assert_eq!(CliThemeDto::default().theme, CliTheme::Ember);
        assert!(parse::<CliThemeDto>(json!({ "theme": "neon" })).is_err());
    }

    #[test]
    fn an_installed_cli_must_be_bundled() {
        assert!(
            parse::<CliInstallDto>(
                json!({ "available": true, "installed": true, "command": CLI_COMMAND })
            )
            .is_ok()
        );
        assert!(
            parse::<CliInstallDto>(
                json!({ "available": false, "installed": true, "command": CLI_COMMAND })
            )
            .is_err()
        );
    }
}
