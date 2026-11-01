use serde::{Deserialize, Serialize};

use crate::artifacts::ArtifactDto;
use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::{BackgroundShellId, StreamId, is_wire_uuid};
use crate::primitives::Timestamp;
use crate::process::{PROCESS_ARG_MAX, PROCESS_ARGV_MAX, PROCESS_PROGRAM_MAX};
use crate::validate::{Validate, check, invalid, nested, no_refinements};

pub const SHELL_STEP_DOMAIN: &str = "shell";
pub const SHELLS_PER_OWNER_MAX: usize = 4;
pub const SHELLS_TOTAL_MAX: usize = 16;
pub const SHELL_LIFETIME_SECS: u64 = 3600;
pub const SHELL_BUFFER_MAX: usize = 262_144;
pub const SHELL_TAIL_MAX: usize = 16_000;
pub const SHELL_ARTIFACTS_MAX: usize = 64;

wire_enum!(BackgroundShellStatus {
    Running => "running",
    Exited => "exited",
    Stopped => "stopped",
    Failed => "failed",
    TimedOut => "timed-out",
});

impl BackgroundShellStatus {
    pub const fn running(self) -> bool {
        matches!(self, Self::Running)
    }
}

fn check_owner(owner: &str) -> Result<(), AppError> {
    if is_wire_uuid(owner) {
        Ok(())
    } else {
        Err(invalid("ownerId", "uuid"))
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundShellDto {
    pub id: BackgroundShellId,
    pub owner_id: String,
    pub program: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub status: BackgroundShellStatus,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub exit_code: Option<i32>,
    pub stream_id: StreamId,
    pub started_at: Timestamp,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub ended_at: Option<Timestamp>,
}

impl Validate for BackgroundShellDto {
    fn validate(&self) -> Result<(), AppError> {
        check_owner(&self.owner_id)?;
        check::non_empty("program", &self.program)?;
        check::max_len("program", &self.program, PROCESS_PROGRAM_MAX)?;
        if self.argv.len() > PROCESS_ARGV_MAX {
            return Err(invalid("argv", "maxItems"));
        }
        for (index, argument) in self.argv.iter().enumerate() {
            check::max_len(&format!("argv.{index}"), argument, PROCESS_ARG_MAX)?;
        }
        check::non_empty("cwd", &self.cwd)?;
        check::timestamp("startedAt", self.started_at)?;
        if let Some(ended) = self.ended_at {
            check::timestamp("endedAt", ended)?;
        }
        if self.status.running() != self.ended_at.is_none() {
            return Err(invalid("endedAt", "status"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundShellStatusDto {
    pub domain: String,
    pub id: BackgroundShellId,
    pub owner_id: String,
    pub status: BackgroundShellStatus,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub exit_code: Option<i32>,
}

impl Validate for BackgroundShellStatusDto {
    fn validate(&self) -> Result<(), AppError> {
        if self.domain != SHELL_STEP_DOMAIN {
            return Err(invalid("domain", "literal"));
        }
        check_owner(&self.owner_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellsListInput {
    pub owner_id: String,
}

impl Validate for ShellsListInput {
    fn validate(&self) -> Result<(), AppError> {
        check_owner(&self.owner_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellRef {
    pub id: BackgroundShellId,
}

no_refinements!(ShellRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellsOutputInput {
    pub id: BackgroundShellId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub since_seq: Option<u64>,
}

impl Validate for ShellsOutputInput {
    fn validate(&self) -> Result<(), AppError> {
        self.since_seq
            .map_or(Ok(()), |seq| check::safe_integer("sinceSeq", seq))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellOutputDto {
    pub shell: BackgroundShellDto,
    pub text: String,
    pub next_seq: u64,
    pub truncated: bool,
    pub event_seq: u64,
}

impl Validate for ShellOutputDto {
    fn validate(&self) -> Result<(), AppError> {
        self.shell.validate()?;
        check::max_len("text", &self.text, SHELL_BUFFER_MAX)?;
        check::safe_integer("nextSeq", self.next_seq)?;
        check::safe_integer("eventSeq", self.event_seq)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::parse;
    use serde_json::json;

    const SHELL: &str = "0199aa11-4444-7111-8111-111111111111";
    const OWNER: &str = "0199aa11-1111-7111-8111-111111111111";
    const STREAM: &str = "0199aa11-5555-7111-8111-111111111111";

    fn shell(status: &str, ended: serde_json::Value) -> serde_json::Value {
        json!({
            "id": SHELL,
            "ownerId": OWNER,
            "program": "C:/node/npx.cmd",
            "argv": ["npx", "http-server"],
            "cwd": "C:/work",
            "status": status,
            "exitCode": null,
            "streamId": STREAM,
            "startedAt": 1_000,
            "endedAt": ended,
        })
    }

    #[test]
    fn a_running_shell_has_no_end_and_an_ended_one_has_one() {
        assert!(parse::<BackgroundShellDto>(shell("running", json!(null))).is_ok());
        assert!(parse::<BackgroundShellDto>(shell("stopped", json!(2_000))).is_ok());
        assert!(parse::<BackgroundShellDto>(shell("running", json!(2_000))).is_err());
        assert!(parse::<BackgroundShellDto>(shell("exited", json!(null))).is_err());
    }

    #[test]
    fn the_owner_is_a_uuid() {
        assert!(parse::<ShellsListInput>(json!({ "ownerId": OWNER })).is_ok());
        assert!(parse::<ShellsListInput>(json!({ "ownerId": "dialog" })).is_err());
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndedShellDto {
    pub shell: BackgroundShellDto,
    pub tail: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactDto>,
}

impl Validate for EndedShellDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("shell", self.shell.validate())?;
        if self.shell.status.running() {
            return Err(invalid("shell.status", "ended"));
        }
        check::max_len("tail", &self.tail, SHELL_TAIL_MAX)?;
        if self.artifacts.len() > SHELL_ARTIFACTS_MAX {
            return Err(invalid("artifacts", "maxItems"));
        }
        nested("artifacts", self.artifacts.validate())
    }
}
