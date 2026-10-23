use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::{ConversationId, StreamId, TerminalId};
use crate::primitives::Timestamp;
use crate::validate::{Validate, check, invalid, nested, no_refinements};

pub const TERMINAL_STEP_DOMAIN: &str = "terminal";
pub const TERMINAL_INPUT_MAX: usize = 65_536;
pub const TERMINAL_SCROLLBACK_MAX: usize = 262_144;
pub const TERMINAL_TITLE_MAX: usize = 80;
pub const TERMINAL_COLS_MAX: u16 = 1000;
pub const TERMINAL_ROWS_MAX: u16 = 500;
pub const TERMINAL_SESSIONS_MAX: usize = 8;

wire_enum!(TerminalShell {
    PowerShell => "powershell",
    Pwsh => "pwsh",
    Cmd => "cmd",
    GitBash => "gitBash",
    Wsl => "wsl",
});

wire_enum!(TerminalStatus {
    Running => "running",
    Exited => "exited",
});

fn check_size(cols: u16, rows: u16) -> Result<(), AppError> {
    check::int_range("cols", u64::from(cols), 2, u64::from(TERMINAL_COLS_MAX))?;
    check::int_range("rows", u64::from(rows), 1, u64::from(TERMINAL_ROWS_MAX))
}

fn check_title(title: &str) -> Result<(), AppError> {
    check::non_empty("title", title)?;
    check::max_len("title", title, TERMINAL_TITLE_MAX)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSessionDto {
    pub id: TerminalId,
    pub conversation_id: ConversationId,
    pub shell: TerminalShell,
    pub title: String,
    pub cwd: String,
    pub status: TerminalStatus,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub exit_code: Option<i32>,
    pub stream_id: StreamId,
    pub cols: u16,
    pub rows: u16,
    pub created_at: Timestamp,
}

impl Validate for TerminalSessionDto {
    fn validate(&self) -> Result<(), AppError> {
        check_title(&self.title)?;
        check::non_empty("cwd", &self.cwd)?;
        check_size(self.cols, self.rows)?;
        check::timestamp("createdAt", self.created_at)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCreateInput {
    pub conversation_id: ConversationId,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub shell: Option<TerminalShell>,
    pub cols: u16,
    pub rows: u16,
}

impl Validate for TerminalCreateInput {
    fn validate(&self) -> Result<(), AppError> {
        check_size(self.cols, self.rows)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRef {
    pub id: TerminalId,
}

no_refinements!(TerminalRef);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalWriteInput {
    pub id: TerminalId,
    pub data: String,
}

impl Validate for TerminalWriteInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("data", &self.data)?;
        check::max_len("data", &self.data, TERMINAL_INPUT_MAX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalResizeInput {
    pub id: TerminalId,
    pub cols: u16,
    pub rows: u16,
}

impl Validate for TerminalResizeInput {
    fn validate(&self) -> Result<(), AppError> {
        check_size(self.cols, self.rows)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRenameInput {
    pub id: TerminalId,
    pub title: String,
}

impl Validate for TerminalRenameInput {
    fn validate(&self) -> Result<(), AppError> {
        check_title(&self.title)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalAttachDto {
    pub session: TerminalSessionDto,
    pub scrollback: String,
    pub next_seq: u64,
}

impl Validate for TerminalAttachDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("session", self.session.validate())?;
        check::max_len("scrollback", &self.scrollback, TERMINAL_SCROLLBACK_MAX)?;
        check::safe_integer("nextSeq", self.next_seq)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalStatusDto {
    pub domain: String,
    pub id: TerminalId,
    pub status: TerminalStatus,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub exit_code: Option<i32>,
}

impl Validate for TerminalStatusDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.domain != TERMINAL_STEP_DOMAIN {
            return Err(invalid("domain", "literal"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::parse;
    use serde_json::json;

    const TERMINAL: &str = "0199aa11-3333-7111-8111-111111111111";
    const CONVERSATION: &str = "0199aa11-1111-7111-8111-111111111111";

    #[test]
    fn the_shell_may_be_left_to_the_host() {
        let input = parse::<TerminalCreateInput>(json!({
            "conversationId": CONVERSATION,
            "shell": null,
            "cols": 80,
            "rows": 24,
        }))
        .expect("parses");
        assert_eq!(input.shell, None);
    }

    #[test]
    fn the_size_has_bounds() {
        let at = |cols: u16, rows: u16| {
            parse::<TerminalResizeInput>(json!({ "id": TERMINAL, "cols": cols, "rows": rows }))
        };
        assert!(at(2, 1).is_ok());
        assert!(at(TERMINAL_COLS_MAX, TERMINAL_ROWS_MAX).is_ok());
        assert!(at(1, 24).is_err());
        assert!(at(80, 0).is_err());
        assert!(at(TERMINAL_COLS_MAX + 1, 24).is_err());
    }

    #[test]
    fn empty_input_is_refused() {
        assert!(parse::<TerminalWriteInput>(json!({ "id": TERMINAL, "data": "" })).is_err());
        assert!(parse::<TerminalWriteInput>(json!({ "id": TERMINAL, "data": "\u{3}" })).is_ok());
    }

    #[test]
    fn a_status_names_its_domain() {
        let status = TerminalStatusDto {
            domain: TERMINAL_STEP_DOMAIN.to_owned(),
            id: TerminalId::parse(TERMINAL).expect("uuid"),
            status: TerminalStatus::Exited,
            exit_code: Some(1),
        };
        assert_eq!(
            serde_json::to_value(&status).expect("serialises"),
            json!({ "domain": "terminal", "id": TERMINAL, "status": "exited", "exitCode": 1 })
        );
        assert!(status.validate().is_ok());
    }
}
