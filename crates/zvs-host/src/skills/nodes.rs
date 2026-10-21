use std::collections::HashMap;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zvs_contract::validate::check;
use zvs_contract::{
    AppError, AppErrorCode, ChatToolCallStatus, ToolMode, ToolTier, Validate, resolve_tool_mode,
};

use super::library::SkillLibrary;
use crate::drivers::ai::{ChatMessage, GenerateRequest, with_injected_tools};
use crate::kernel::nodes::{GenerateMessage, GenerateNodeInput, GenerateRole};
use crate::kernel::{NodeDef, NodeRegistry, PermissionRequirement, StepContext, ToolStepRequest};
use crate::services::chat::GenerateChatNode;
use crate::services::chat_tools::{
    CHAT_TOOL_CALLS, CHAT_TOOL_ROUNDS, OfferedTool, TOOL_BUDGET_NOTE, TOOL_RESULTS_INSTRUCTION,
    cap_text, fence_tool_result, parse_tool_arguments, tool_result_text, tool_specs,
};

pub const SKILL_LOAD: &str = "skill.load";
pub const SKILL_READ: &str = "skill.read";
pub const SKILL_GENERATE: &str = "skill.generate";
pub const SKILL_TOOLS: [&str; 2] = [SKILL_LOAD, SKILL_READ];

pub const LOAD_DESCRIPTION: &str = "Загрузка навыка по идентификатору из списка навыков в системном сообщении. Возвращает инструкции навыка и перечень его файлов; при выполнении запроса следуйте этим инструкциям. Вызывайте только для навыка, который соответствует запросу.";
pub const READ_DESCRIPTION: &str = "Чтение файла навыка из папок references/, scripts/ или assets/ по идентификатору навыка и пути из результата skill.load. Текстовый файл возвращается целиком, для двоичного сообщается только размер.";

pub const NO_PROVIDERS: &str = "Сервис провайдеров недоступен";
pub const NOT_OFFERED: &str = "Модель вызвала инструмент, который не был ей предложен";
pub const NOT_AN_OBJECT: &str = "Аргументы вызова не являются JSON-объектом";
pub const BUDGET_EXHAUSTED: &str = "Бюджет вызовов инструментов исчерпан";
pub const NO_TOOL_CALLS: &str = "Вызов инструментов недоступен в этом запуске";
pub const CALL_DENIED: &str = "Вызов отклонён пользователем";
pub const CALL_FORBIDDEN: &str = "Инструмент запрещён разрешением или не входит в набор навыка";
pub const TOOL_REPORTED_ERROR: &str = "Инструмент вернул ошибку";
pub const BINARY_FILE: &str = "Binary file; its contents are not returned as text.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillLoadInput {
    #[schemars(description = "Идентификатор навыка из списка навыков, например release-notes")]
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillLoadOutput {
    pub key: String,
    pub name: String,
    pub instructions: String,
    pub files: Vec<SkillFileEntry>,
}

impl Validate for SkillLoadOutput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillReadInput {
    #[schemars(description = "Идентификатор навыка")]
    pub key: String,
    #[schemars(description = "Путь файла из результата skill.load, например references/style.md")]
    pub path: String,
}

impl Validate for SkillReadInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("key", &self.key)?;
        check::max_len("key", &self.key, 128)?;
        check::non_empty("path", &self.path)?;
        check::max_len("path", &self.path, 512)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkillReadOutput {
    pub key: String,
    pub path: String,
    pub size: u64,
    pub text: String,
    pub binary: bool,
}

impl Validate for SkillReadOutput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

struct SkillLoadNode(SkillLibrary);
struct SkillReadNode(SkillLibrary);

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
        Ok(SkillLoadOutput {
            key: loaded.skill.key,
            name: loaded.skill.name,
            instructions: loaded.instructions,
            files: loaded
                .files
                .into_iter()
                .map(|file| SkillFileEntry {
                    path: file.path,
                    size: file.size,
                })
                .collect(),
        })
    }
}

#[async_trait]
impl NodeDef for SkillReadNode {
    type Input = SkillReadInput;
    type Output = SkillReadOutput;

    fn node_type(&self) -> &str {
        SKILL_READ
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool(SKILL_READ)
    }

    fn side_effect(&self) -> bool {
        false
    }

    async fn run(
        &self,
        context: &StepContext,
        input: SkillReadInput,
    ) -> Result<SkillReadOutput, AppError> {
        context.check_cancelled()?;
        let file = self.0.read(&input.key, &input.path)?;
        Ok(SkillReadOutput {
            key: input.key,
            path: file.path,
            size: file.size,
            binary: file.text.is_none(),
            text: file.text.unwrap_or_else(|| BINARY_FILE.to_owned()),
        })
    }
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
}

fn settled(status: ChatToolCallStatus, error: &str) -> Settled {
    Settled {
        status,
        error: Some(error.to_owned()),
        content: format!("Tool call {}: {error}", status.as_str()),
        step_id: None,
    }
}

impl SkillGenerateNode {
    async fn call(
        context: &StepContext,
        tool: Option<&OfferedTool>,
        arguments: &str,
        label: String,
        limit: usize,
    ) -> Result<Settled, AppError> {
        let Some(tool) = tool else {
            return Ok(settled(ChatToolCallStatus::Refused, NOT_OFFERED));
        };
        let Some(arguments) = parse_tool_arguments(arguments) else {
            return Ok(settled(ChatToolCallStatus::Failed, NOT_AN_OBJECT));
        };
        let request = ToolStepRequest {
            key: tool.key.clone(),
            input: Value::Object(arguments),
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
                let capped = cap_text(&text.text, limit);
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
        let raw = providers.text(&input.provider_id)?;
        let mode = if input.tools.is_empty() {
            ToolMode::None
        } else {
            resolve_tool_mode(raw.capabilities().tools, input.model_tools)
        };
        let offered: Vec<OfferedTool> = if mode == ToolMode::None {
            Vec::new()
        } else {
            input.tools.clone()
        };
        let driver = if mode == ToolMode::Injected {
            with_injected_tools(raw, None)
        } else {
            raw
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
                    Self::call(
                        context,
                        tool,
                        &call.arguments,
                        format!("call{}", calls.len() + 1),
                        limit,
                    )
                    .await?
                };
                let content = if mode == ToolMode::Native {
                    fence_tool_result(&call.name, outcome.status.as_str(), &outcome.content)
                } else {
                    outcome.content
                };
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
    nodes.register(SkillReadNode(library.clone()))?;
    nodes.register(SkillGenerateNode)
}
