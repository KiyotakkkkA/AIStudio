use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::events::HostEvent;
use crate::folders::{WorkingFolderAccess, check_folder_path};
use crate::validate::{Validate, check, invalid, no_refinements};

pub const CLI_COMMAND: &str = "aistudio";
pub const CLI_PROTOCOL_VERSION: u32 = 1;
pub const CLI_LINE_MAX: usize = 8 * 1024 * 1024;
pub const CLI_PIPE_PREFIX: &str = r"\\.\pipe\zvs-ai-studio-cli-";
pub const CLI_LAUNCH_ARG: &str = "--cli";
pub const CLI_THEME_FILE: &str = "theme.json";

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
