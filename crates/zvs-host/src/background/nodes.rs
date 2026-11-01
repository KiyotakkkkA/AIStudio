use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zvs_contract::validate::check;
use zvs_contract::{AppError, AppErrorCode, BackgroundShellDto, BackgroundShellId, Validate};

use super::service::BackgroundShells;
use crate::kernel::{NodeDef, NodeRegistry, PermissionRequirement, StepContext};
use crate::toolchain::process::fenced;

pub const SHELL_OUTPUT: &str = "shell.output";
pub const SHELL_KILL: &str = "shell.kill";
pub const BACKGROUND_TOOLS: [&str; 2] = [SHELL_OUTPUT, SHELL_KILL];

pub const OUTPUT_DESCRIPTION: &str = "Reads a background process from shell.run: status, exit code and the output tail; with sinceSeq, only newer output (pass the returned nextSeq).";
pub const KILL_DESCRIPTION: &str = "Stops a background process and its child processes.";

pub const NO_OWNER: &str = "Фоновые процессы доступны только в диалоге";

const TAIL_DEFAULT: usize = 100;
const TAIL_MAX: usize = 500;
const SINCE_TEXT_MAX: usize = 16_000;

fn owner(conversation: &str) -> Result<&str, AppError> {
    if conversation.is_empty() {
        Err(AppError::new(AppErrorCode::ValidationFailed, NO_OWNER))
    } else {
        Ok(conversation)
    }
}

pub fn status_line(shell: &BackgroundShellDto) -> String {
    let code = shell
        .exit_code
        .map_or_else(|| "none".to_owned(), |code| code.to_string());
    format!(
        "id: {}\ncommand: {}\ncwd: {}\nstatus: {}\nexit code: {code}",
        shell.id,
        shell.argv.join(" "),
        shell.cwd,
        shell.status.as_str(),
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShellToolOutput {
    pub text: String,
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shells: Vec<BackgroundShellDto>,
}

impl Validate for ShellToolOutput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

fn shells_meta(output: &ShellToolOutput) -> Option<Value> {
    Some(json!({ "shells": output.shells }))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShellOutputInput {
    #[schemars(description = "Process id from shell.run")]
    pub id: BackgroundShellId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "nextSeq of a previous call")]
    pub since_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Lines of tail, 1-500")]
    pub tail: Option<usize>,
    #[serde(default)]
    #[schemars(skip)]
    pub conversation_id: String,
}

impl Validate for ShellOutputInput {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(seq) = self.since_seq {
            check::safe_integer("sinceSeq", seq)?;
        }
        if let Some(tail) = self.tail {
            check::int_range("tail", tail as u64, 1, TAIL_MAX as u64)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShellKillInput {
    #[schemars(description = "Process id from shell.run")]
    pub id: BackgroundShellId,
    #[serde(default)]
    #[schemars(skip)]
    pub conversation_id: String,
}

impl Validate for ShellKillInput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

fn clipped(text: &str) -> (&str, bool) {
    if text.len() <= SINCE_TEXT_MAX {
        return (text, false);
    }
    let mut cut = text.len() - SINCE_TEXT_MAX;
    while !text.is_char_boundary(cut) {
        cut += 1;
    }
    (&text[cut..], true)
}

pub struct ShellOutputNode(BackgroundShells);

#[async_trait]
impl NodeDef for ShellOutputNode {
    type Input = ShellOutputInput;
    type Output = ShellToolOutput;

    fn node_type(&self) -> &str {
        SHELL_OUTPUT
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(SHELL_OUTPUT)
    }

    fn side_effect(&self) -> bool {
        false
    }

    async fn run(
        &self,
        _context: &StepContext,
        input: ShellOutputInput,
    ) -> Result<ShellToolOutput, AppError> {
        let owner = owner(&input.conversation_id)?;
        let (shell, body, notes) = match input.since_seq {
            Some(since) => {
                let output = self.0.output(Some(owner), &input.id, Some(since))?;
                let (text, clipped) = clipped(&output.text);
                let mut notes = vec![format!("nextSeq: {}", output.next_seq)];
                if output.truncated || clipped {
                    notes.push("[earlier output is no longer kept]".to_owned());
                }
                (output.shell, text.to_owned(), notes)
            }
            None => {
                let lines = input.tail.unwrap_or(TAIL_DEFAULT);
                let (shell, tail, cut) = self.0.tail(Some(owner), &input.id, lines)?;
                let next = self.0.output(Some(owner), &input.id, None)?.next_seq;
                let mut notes = vec![format!("nextSeq: {next}")];
                if cut {
                    notes.push(format!("[last {lines} lines shown]"));
                }
                (shell, tail, notes)
            }
        };
        let text = format!(
            "{}\n{}\n{}",
            status_line(&shell),
            notes.join("\n"),
            fenced("output", &body)
        );
        Ok(ShellToolOutput {
            text,
            is_error: false,
            shells: vec![shell],
        })
    }

    fn tool_meta(&self, _input: &Value, output: &ShellToolOutput) -> Option<Value> {
        shells_meta(output)
    }
}

pub struct ShellKillNode(BackgroundShells);

#[async_trait]
impl NodeDef for ShellKillNode {
    type Input = ShellKillInput;
    type Output = ShellToolOutput;

    fn node_type(&self) -> &str {
        SHELL_KILL
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(SHELL_KILL)
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(
        &self,
        _context: &StepContext,
        input: ShellKillInput,
    ) -> Result<ShellToolOutput, AppError> {
        let owner = owner(&input.conversation_id)?;
        let shell = self.0.kill(Some(owner), &input.id).await?;
        Ok(ShellToolOutput {
            text: status_line(&shell),
            is_error: false,
            shells: vec![shell],
        })
    }

    fn tool_meta(&self, _input: &Value, output: &ShellToolOutput) -> Option<Value> {
        shells_meta(output)
    }
}

pub fn register_background_nodes(
    nodes: &NodeRegistry,
    shells: &BackgroundShells,
) -> Result<(), AppError> {
    nodes.register(ShellOutputNode(shells.clone()))?;
    nodes.register(ShellKillNode(shells.clone()))
}
