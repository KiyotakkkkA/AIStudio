use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zvs_contract::skills::{
    SKILL_DESCRIPTION_MAX, SKILL_FILES_MAX, SKILL_INSTRUCTIONS_MAX, SKILL_KEY_MAX, SKILL_NAME_MAX,
    SKILL_PATH_MAX, SKILL_VERSION_MAX,
};
use zvs_contract::validate::check;
use zvs_contract::{
    AppError, AppErrorCode, ChatToolCallStatus, SkillIssueDto, ToolMode, ToolTier, Validate,
    VendorTurnOptions, resolve_tool_mode,
};

use super::library::{AuthoredSkill, SkillLibrary};
use super::turn::{SkillTurn, presented, result_limit};
use crate::commands::SHELL_RUN;
use crate::drivers::ai::{ChatMessage, GenerateRequest};
use crate::files::roots::SKILL_ROOT;
use crate::files::{FS_APPLY, FS_LIST, FS_READ};
use crate::kernel::nodes::{GenerateMessage, GenerateNodeInput, GenerateRole};
use crate::kernel::{NodeDef, NodeRegistry, PermissionRequirement, StepContext, ToolStepRequest};
use crate::services::chat::GenerateChatNode;
use crate::services::chat_tools::{
    CHAT_TOOL_CALLS, CHAT_TOOL_ROUNDS, OfferedTool, TOOL_BUDGET_NOTE, TOOL_RESULTS_INSTRUCTION,
    cap_text, fence_tool_result, parse_tool_arguments, tool_result_text, tool_specs,
};

pub const SKILL_LOAD: &str = "skill.load";
pub const SKILL_GENERATE: &str = "skill.generate";
pub const SKILL_SAVE: &str = "skill.save";
pub const SKILL_TOOLS: [&str; 5] = [SKILL_LOAD, FS_READ, FS_LIST, FS_APPLY, SHELL_RUN];

pub const LOAD_DESCRIPTION: &str = "Loads a skill by identifier from the skill list in the system message. Returns the skill instructions, its skill:<key>/ folder, the file list with each file's purpose, and the contents of the files that must be read; other files are read via fs.read, scripts from scripts/ are run via shell.run. Follow the skill instructions while fulfilling the request. Call only for a skill that matches the request.";
pub const SAVE_DESCRIPTION: &str = "Saves a skill into the user's skill library: identifier, name, description used for skill selection, Markdown instructions, and text files under references/, scripts/, and assets/. A skill with the same identifier is overwritten only with replace: true; built-in skills cannot be overwritten. Every call is confirmed by the user. Returns the skill state and any errors found.";

pub const NO_PROVIDERS: &str = "Сервис провайдеров недоступен";
pub const NOT_OFFERED: &str = "Модель вызвала инструмент, который не был ей предложен";
pub const NOT_AN_OBJECT: &str = "Аргументы вызова не являются JSON-объектом";
pub const BUDGET_EXHAUSTED: &str = "Бюджет вызовов инструментов исчерпан";
pub const NO_TOOL_CALLS: &str = "Вызов инструментов недоступен в этом запуске";
pub const CALL_DENIED: &str = "Вызов отклонён пользователем";
pub const CALL_FORBIDDEN: &str = "Инструмент запрещён разрешением или не входит в набор навыка";
pub const TOOL_REPORTED_ERROR: &str = "Инструмент вернул ошибку";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillLoadInput {
    #[schemars(description = "Skill identifier from the skill list, e.g. release-notes")]
    pub key: String,
}

impl Validate for SkillLoadInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("key", &self.key)?;
        check::max_len("key", &self.key, 128)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillFileEntry {
    pub path: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillLoadOutput {
    pub key: String,
    pub name: String,
    pub folder: String,
    pub files: Vec<SkillFileEntry>,
    #[serde(default)]
    pub inlined: Vec<String>,
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub requires: BTreeMap<String, Vec<String>>,
    pub text: String,
}

impl Validate for SkillLoadOutput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillSaveFile {
    #[schemars(description = "Path inside the skill: references/…, scripts/…, or assets/…")]
    pub path: String,
    #[schemars(description = "Text content of the file")]
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillSaveInput {
    #[schemars(
        description = "Идентификатор: строчные латинские буквы, цифры и дефисы, например release-notes"
    )]
    pub key: String,
    #[schemars(description = "Skill name shown to the user")]
    pub name: String,
    #[schemars(
        description = "Что делает навык и в каких запросах он нужен; по этому тексту навык выбирается"
    )]
    pub description: String,
    #[schemars(description = "Skill instructions in Markdown, without the frontmatter block")]
    pub instructions: String,
    #[serde(default)]
    #[schemars(description = "Skill files")]
    pub files: Vec<SkillSaveFile>,
    #[serde(default)]
    #[schemars(description = "Skill version, defaults to 1.0.0")]
    pub version: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Файлы из references/ или assets/, которые нужны при каждом запросе: их текст передаётся вместе с инструкциями"
    )]
    pub read_first: Vec<String>,
    #[serde(default)]
    #[schemars(
        description = "Скрипты и файлы, которые нужно прочитать до их запуска, например {\"scripts/build.py\": [\"references/format.md\"]}"
    )]
    pub requires: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    #[schemars(
        description = "true — перезаписать навык пользователя с тем же идентификатором целиком"
    )]
    pub replace: bool,
}

impl Validate for SkillSaveInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("key", &self.key)?;
        check::max_len("key", &self.key, SKILL_KEY_MAX)?;
        check::max_len("name", &self.name, SKILL_NAME_MAX)?;
        check::max_len("description", &self.description, SKILL_DESCRIPTION_MAX)?;
        check::max_len("instructions", &self.instructions, SKILL_INSTRUCTIONS_MAX)?;
        if self.files.len() > SKILL_FILES_MAX {
            return Err(zvs_contract::validate::invalid("files", "maxLength"));
        }
        for (index, file) in self.files.iter().enumerate() {
            check::max_len(&format!("files.{index}.path"), &file.path, SKILL_PATH_MAX)?;
        }
        if let Some(version) = &self.version {
            check::max_len("version", version, SKILL_VERSION_MAX)?;
        }
        if self.read_first.len() > SKILL_FILES_MAX {
            return Err(zvs_contract::validate::invalid("readFirst", "maxLength"));
        }
        for (index, path) in self.read_first.iter().enumerate() {
            check::max_len(&format!("readFirst.{index}"), path, SKILL_PATH_MAX)?;
        }
        if self.requires.len() > SKILL_FILES_MAX {
            return Err(zvs_contract::validate::invalid("requires", "maxLength"));
        }
        for (script, files) in &self.requires {
            check::max_len("requires", script, SKILL_PATH_MAX)?;
            if files.len() > SKILL_FILES_MAX {
                return Err(zvs_contract::validate::invalid("requires", "maxLength"));
            }
            for path in files {
                check::max_len("requires", path, SKILL_PATH_MAX)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillSaveOutput {
    pub key: String,
    pub name: String,
    pub status: String,
    pub version: Option<String>,
    pub files: Vec<String>,
    pub issues: Vec<String>,
}

impl Validate for SkillSaveOutput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

fn issue_text(issue: &SkillIssueDto) -> String {
    format!("{} ({})", issue.field, issue.rule.as_str())
}

pub fn explained(error: AppError) -> AppError {
    let issues: Vec<SkillIssueDto> = error
        .details
        .as_ref()
        .and_then(|details| details.get("issues"))
        .and_then(|issues| serde_json::from_value(issues.clone()).ok())
        .unwrap_or_default();
    if issues.is_empty() {
        return error;
    }
    let list: Vec<String> = issues.iter().map(issue_text).collect();
    AppError {
        message: format!("{}: {}", error.message, list.join(", ")),
        ..error
    }
}

struct SkillLoadNode(SkillLibrary);
struct SkillSaveNode(SkillLibrary);

#[async_trait]
impl NodeDef for SkillSaveNode {
    type Input = SkillSaveInput;
    type Output = SkillSaveOutput;

    fn node_type(&self) -> &str {
        SKILL_SAVE
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(SKILL_SAVE)
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(
        &self,
        context: &StepContext,
        input: SkillSaveInput,
    ) -> Result<SkillSaveOutput, AppError> {
        context.check_cancelled()?;
        let saved = self
            .0
            .author(&AuthoredSkill {
                key: input.key,
                name: input.name,
                description: input.description,
                instructions: input.instructions,
                files: input
                    .files
                    .into_iter()
                    .map(|file| (file.path, file.content))
                    .collect(),
                version: input.version,
                read_first: input.read_first,
                requires: input.requires.into_iter().collect(),
                replace: input.replace,
            })
            .map_err(explained)?;
        Ok(SkillSaveOutput {
            key: saved.skill.key,
            name: saved.skill.name,
            status: saved.skill.status.as_str().to_owned(),
            version: saved.skill.version,
            files: saved.files.into_iter().map(|file| file.path).collect(),
            issues: saved.skill.issues.iter().map(issue_text).collect(),
        })
    }
}

#[async_trait]
impl NodeDef for SkillLoadNode {
    type Input = SkillLoadInput;
    type Output = SkillLoadOutput;

    fn node_type(&self) -> &str {
        SKILL_LOAD
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(SKILL_LOAD)
    }

    fn side_effect(&self) -> bool {
        false
    }

    async fn run(
        &self,
        context: &StepContext,
        input: SkillLoadInput,
    ) -> Result<SkillLoadOutput, AppError> {
        context.check_cancelled()?;
        let loaded = self.0.load(&input.key)?;
        let mut remaining = READ_FIRST_BUDGET;
        let mut inlined: Vec<(String, String)> = Vec::new();
        let mut required = Vec::new();
        for path in &loaded.read_first {
            let Some(text) = self
                .0
                .read(&loaded.skill.key, path)
                .ok()
                .and_then(|content| content.text)
            else {
                continue;
            };
            let length = text.chars().count();
            if required.is_empty() && length <= remaining {
                remaining -= length;
                inlined.push((path.clone(), text));
            } else {
                required.push(path.clone());
            }
        }
        let output = SkillLoadOutput {
            folder: format!("{SKILL_ROOT}{}/", loaded.skill.key),
            files: loaded
                .files
                .iter()
                .map(|file| SkillFileEntry {
                    path: file.path.clone(),
                    size: file.size,
                    purpose: file.purpose.clone(),
                })
                .collect(),
            inlined: inlined.iter().map(|(path, _)| path.clone()).collect(),
            required,
            requires: loaded.requires.iter().cloned().collect(),
            text: String::new(),
            key: loaded.skill.key,
            name: loaded.skill.name,
        };
        Ok(SkillLoadOutput {
            text: load_text(&output, &loaded.instructions, &inlined),
            ..output
        })
    }
}

pub const READ_FIRST_BUDGET: usize = 16_000;

fn load_text(output: &SkillLoadOutput, instructions: &str, inlined: &[(String, String)]) -> String {
    let folder = &output.folder;
    let mut text = format!(
        "Skill \"{}\" ({}) is loaded. Its folder is {folder}: read its files with fs.read at {folder}<path> and run its scripts with shell.run as {folder}scripts/<file>. Follow the instructions below.\n\n<instructions>\n{}\n</instructions>",
        output.name,
        output.key,
        instructions.trim()
    );
    if !output.files.is_empty() {
        text.push_str("\n\nFiles:");
        for file in &output.files {
            text.push_str(&format!("\n- {} ({} B)", file.path, file.size));
            if let Some(purpose) = &file.purpose {
                text.push_str(&format!(": {purpose}"));
            }
            if output.inlined.contains(&file.path) {
                text.push_str(" [included below]");
            }
        }
    }
    if !output.required.is_empty() {
        let paths: Vec<String> = output
            .required
            .iter()
            .map(|path| format!("{folder}{path}"))
            .collect();
        text.push_str(&format!(
            "\n\nRequired reading, not included here: read {} with fs.read before you start.",
            paths.join(", ")
        ));
    }
    for (script, needed) in &output.requires {
        text.push_str(&format!(
            "\n{script} runs only after {} was read in this turn.",
            needed.join(", ")
        ));
    }
    for (path, content) in inlined {
        text.push_str(&format!(
            "\n\n<file path=\"{folder}{path}\">\n{}\n</file>",
            content.trim_end()
        ));
    }
    text
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillGenerateInput {
    pub provider_id: String,
    pub model: String,
    pub system: String,
    pub message: String,
    #[serde(default)]
    pub tools: Vec<OfferedTool>,
    #[serde(default)]
    pub model_tools: Option<bool>,
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    pub tool_result_chars: u64,
}

impl Validate for SkillGenerateInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("providerId", &self.provider_id)?;
        check::non_empty("model", &self.model)?;
        check::non_empty("message", &self.message)?;
        zvs_contract::validate::nested("tools", self.tools.validate())?;
        check::int_range("toolResultChars", self.tool_result_chars, 1, u64::MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillCallRecord {
    pub key: String,
    pub name: String,
    pub status: ChatToolCallStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillGenerateOutput {
    pub text: String,
    pub reasoning: String,
    pub calls: Vec<SkillCallRecord>,
}

impl Validate for SkillGenerateOutput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

struct SkillGenerateNode;

struct Settled {
    status: ChatToolCallStatus,
    error: Option<String>,
    content: String,
    step_id: Option<String>,
    refund: bool,
}

fn settled(status: ChatToolCallStatus, error: &str) -> Settled {
    Settled {
        status,
        error: Some(error.to_owned()),
        content: format!("Tool call {}: {error}", status.as_str()),
        step_id: None,
        refund: false,
    }
}

impl SkillGenerateNode {
    async fn call(
        context: &StepContext,
        tool: Option<&OfferedTool>,
        arguments: &str,
        label: String,
        limit: usize,
        offered: &[OfferedTool],
        turn: &mut SkillTurn,
    ) -> Result<Settled, AppError> {
        let Some(tool) = tool else {
            return Ok(settled(ChatToolCallStatus::Refused, NOT_OFFERED));
        };
        let Some(arguments) = parse_tool_arguments(arguments) else {
            return Ok(settled(ChatToolCallStatus::Failed, NOT_AN_OBJECT));
        };
        let arguments = Value::Object(arguments);
        if let Some(hint) = turn.hint(&tool.key, &arguments) {
            return Ok(Settled {
                refund: true,
                ..settled(ChatToolCallStatus::Refused, &hint)
            });
        }
        let limit = result_limit(&tool.key, limit);
        let request = ToolStepRequest {
            key: tool.key.clone(),
            input: arguments.clone(),
            label,
            minimum_tier: tool.destructive.then_some(ToolTier::Ask),
            on_approval: None,
        };
        let Some(invocation) = context.invoke_tool(request) else {
            return Ok(settled(ChatToolCallStatus::Refused, NO_TOOL_CALLS));
        };
        match invocation.await {
            Ok(result) => {
                let text = tool_result_text(&result.output);
                if !text.is_error {
                    turn.observe(&tool.key, &arguments, &result.output);
                }
                let shown = presented(&tool.key, text.text, offered);
                let capped = cap_text(&shown, limit);
                let content = if capped.truncated {
                    format!("{}\n[result truncated to {limit} characters]", capped.text)
                } else {
                    capped.text
                };
                Ok(Settled {
                    status: if text.is_error {
                        ChatToolCallStatus::Failed
                    } else {
                        ChatToolCallStatus::Succeeded
                    },
                    error: text.is_error.then(|| TOOL_REPORTED_ERROR.to_owned()),
                    content,
                    step_id: Some(result.step_id),
                    refund: false,
                })
            }
            Err(error) if context.is_cancelled() => Err(error),
            Err(error) if error.code == AppErrorCode::ApprovalDenied => {
                Ok(settled(ChatToolCallStatus::Denied, CALL_DENIED))
            }
            Err(error) if error.code == AppErrorCode::PermissionDenied => {
                Ok(settled(ChatToolCallStatus::Refused, CALL_FORBIDDEN))
            }
            Err(error) => Ok(settled(ChatToolCallStatus::Failed, &error.message)),
        }
    }
}

#[async_trait]
impl NodeDef for SkillGenerateNode {
    type Input = SkillGenerateInput;
    type Output = SkillGenerateOutput;

    fn node_type(&self) -> &str {
        SKILL_GENERATE
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool("llm.generate")
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(
        &self,
        context: &StepContext,
        input: SkillGenerateInput,
    ) -> Result<SkillGenerateOutput, AppError> {
        context.check_cancelled()?;
        let providers = context
            .services
            .providers
            .as_ref()
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, NO_PROVIDERS))?;
        let driver = providers.text(&input.provider_id)?;
        let mode = if input.tools.is_empty() {
            ToolMode::None
        } else {
            resolve_tool_mode(driver.capabilities().tools, input.model_tools)
        };
        let offered: Vec<OfferedTool> = if mode == ToolMode::None {
            Vec::new()
        } else {
            input.tools.clone()
        };
        let specs = tool_specs(&offered);
        let by_name: HashMap<&str, &OfferedTool> = offered
            .iter()
            .map(|tool| (tool.name.as_str(), tool))
            .collect();
        let system = if specs.is_empty() {
            input.system.clone()
        } else {
            format!("{}\n\n{TOOL_RESULTS_INSTRUCTION}", input.system)
        };
        let template = GenerateRequest {
            tools: specs.clone(),
            ..GenerateNodeInput {
                provider_id: input.provider_id.clone(),
                model: input.model.clone(),
                messages: vec![GenerateMessage {
                    role: GenerateRole::User,
                    content: input.message.clone(),
                }],
                system: Some(system),
                temperature: None,
                top_k: None,
                top_p: None,
                max_output_tokens: input.max_output_tokens,
                vendor: VendorTurnOptions::default(),
            }
            .request()
        };
        let limit = usize::try_from(input.tool_result_chars).unwrap_or(usize::MAX);
        let mut messages = template.messages.clone();
        let mut text = String::new();
        let mut reasoning = String::new();
        let mut calls: Vec<SkillCallRecord> = Vec::new();
        let mut executed = 0;
        let mut round = 0;
        let mut turn = SkillTurn::default();
        loop {
            let last = round >= CHAT_TOOL_ROUNDS || executed >= CHAT_TOOL_CALLS;
            if last && !specs.is_empty() {
                messages.push(ChatMessage::user(TOOL_BUDGET_NOTE));
            }
            let output = GenerateChatNode::generate_round(
                context,
                &driver,
                &GenerateRequest {
                    messages: messages.clone(),
                    ..template.clone()
                },
                &text,
            )
            .await?;
            text.push_str(&output.text);
            reasoning.push_str(&output.reasoning);
            if output.tool_calls.is_empty() || specs.is_empty() {
                break;
            }
            messages.push(
                ChatMessage::assistant(output.text.clone())
                    .with_tool_calls(output.tool_calls.clone()),
            );
            for call in output.tool_calls {
                let tool = by_name.get(call.name.as_str()).copied();
                let outcome = if last || executed >= CHAT_TOOL_CALLS {
                    settled(ChatToolCallStatus::Refused, BUDGET_EXHAUSTED)
                } else {
                    executed += 1;
                    let settled = Self::call(
                        context,
                        tool,
                        &call.arguments,
                        format!("call{}", calls.len() + 1),
                        limit,
                        &offered,
                        &mut turn,
                    )
                    .await?;
                    if settled.refund {
                        executed -= 1;
                    }
                    settled
                };
                let content =
                    fence_tool_result(&call.name, outcome.status.as_str(), &outcome.content);
                messages.push(ChatMessage::tool(call.id.clone(), content));
                calls.push(SkillCallRecord {
                    key: tool.map_or_else(|| call.name.clone(), |tool| tool.key.clone()),
                    name: call.name.clone(),
                    status: outcome.status,
                    error: outcome.error,
                    step_id: outcome.step_id,
                });
            }
            if last {
                break;
            }
            round += 1;
        }
        Ok(SkillGenerateOutput {
            text,
            reasoning,
            calls,
        })
    }
}

pub fn register_skill_nodes(nodes: &NodeRegistry, library: &SkillLibrary) -> Result<(), AppError> {
    nodes.register(SkillLoadNode(library.clone()))?;
    nodes.register(SkillSaveNode(library.clone()))?;
    nodes.register(SkillGenerateNode)
}
