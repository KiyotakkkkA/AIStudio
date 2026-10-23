use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use async_trait::async_trait;
use futures::StreamExt;
use futures::future::join_all;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zvs_contract::validate::{check, js_slice, nested};
use zvs_contract::vector_stores::DEFAULT_CHAT_MIN_SCORE;
use zvs_contract::{
    AppError, AppErrorCode, ChatCitationDto, ChatContextDto, ChatContextInput, ChatContextToolDto,
    ChatEvalCaseDto, ChatEvalInput, ChatEvalModeDto, ChatEvalRunDto, ChatSendInput, ChatSettings,
    ChatToolCallDto, ChatToolCallStatus, ConversationDetailDto, ConversationDto, ConversationId,
    CreateConversationInput, DEFAULT_CONVERSATION_TITLE, DeltaKind, DocumentId,
    GRAPH_EXPANSION_DEFAULT_MAX, GraphDirection, GraphExpansionDocument, GraphNeighborhood,
    GraphNeighborsInput, HostEvent, HostEventDraft, MessageDto, MessageId, MessageRole,
    ProviderCapability, ProviderId, RetrievalMode, RunGraph, RunHandleDto, RunId, RunKind,
    RunStatus, StartRunInput, ToolListFilter, ToolMode, ToolTier, Validate, VectorSearchHitDto,
    VectorStoreId, log_line, merge_graph_neighbors, model_tool_support, resolve_tool_mode,
};

use super::chat_history::{
    MESSAGE_OVERHEAD, RelatedSource, RetrievedSource, estimate_tokens, measure_context, tokens,
    window_chat_history, with_selections,
};
use super::chat_tools::{
    CHAT_TOOL_CALLS, CHAT_TOOL_ROUNDS, HYBRID_INSTRUCTION, KNOWLEDGE_INSTRUCTION, KnowledgeText,
    OfferedTool, TOOL_BUDGET_NOTE, TOOL_PREVIEW_CHARS, TOOL_RESULTS_INSTRUCTION, cap_text,
    fence_tool_result, knowledge_catalogue, offered_tools, parse_tool_arguments, read_result_text,
    search_result_text, tool_result_budget, tool_result_text, tool_specs, tool_specs_json,
};
use super::retrieval_merge::{CHAT_FETCH_K, MergeOptions, StoreHits, merge_retrieval};
use super::retrieval_query::retrieval_query;
use super::runs::RunService;
use super::vector_stores::VectorSearchOptions;
use crate::commands::PROCESS_TOOLS;
use crate::data::repositories::{ConversationPatch, MessagePatch, RunPatch};
use crate::data::schema::{ConversationRow, FolderGrantRow, MessageRow};
use crate::data::{DataError, Database, Repositories};
use crate::drivers::ai::ports::SharedTextDriver;
use crate::drivers::ai::with_injected_tools;
use crate::drivers::ai::{ChatMessage, ChatRole, GenerateRequest, TextDelta, ToolCall};
use crate::files::FILE_TOOLS;
use crate::kernel::knowledge_nodes::{
    KNOWLEDGE_READ, KNOWLEDGE_SEARCH, KNOWLEDGE_TOOLS, KnowledgeReadOutput, KnowledgeSearchOutput,
};
use crate::kernel::nodes::{GenerateMessage, GenerateNodeInput, GenerateOutput, GenerateRole};
use crate::kernel::{
    ApprovalDecision, NodeDef, NodeRegistry, PermissionRequirement, RunObserver, StepContext,
    ToolStepRequest,
};
use crate::platform::clock::{SharedClock, SystemClock};
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};
use crate::services::mcp::{RESOURCE_READER, tool_key};
use crate::skills::{SKILL_LOAD, SKILL_TOOLS, SkillCatalogue, catalogue_text};
use crate::tools::ToolRegistry;
use crate::workspace::{FolderAccessService, ForbiddenFolders, folder_access};
use zvs_contract::mcp::MCP_INSTRUCTIONS_MAX;
use zvs_contract::{BindFolderInput, BindFolderResultDto};
use zvs_contract::{ToolDto, ToolSource};

pub const CHAT_RELATED_MAX: usize = 3;
pub const HYBRID_SOURCES_MAX: usize = 3;
pub const KNOWLEDGE_COMPACTED: &str = "Older knowledge results compacted";

static SPACES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\s\x{feff}]+").expect("a valid pattern"));
pub const DEFAULT_CONTEXT_WINDOW: i64 = 8192;
pub const DEFAULT_MAX_OUTPUT: i64 = 1024;
pub const TITLE_MAX: usize = 80;
pub const DEFAULT_TOOL_RESULT_CHARS: u64 = 4000;
pub const CONVERSATION_NOT_FOUND: &str = "Диалог не найден";
pub const MESSAGE_NOT_FOUND: &str = "Сообщение не найдено";
pub const STORE_NOT_FOUND: &str = "Векторное хранилище не найдено";
pub const TOOL_NOT_FOUND: &str = "Инструмент не найден";
pub const ACTIVE_TURN: &str = "В диалоге уже выполняется ход";
pub const INVALID_TURN: &str = "Некорректный ход чата";
pub const NOT_A_CHAT_RUN: &str = "Запуск не относится к чату";
pub const FOREIGN_MESSAGE: &str = "Сообщение не принадлежит этому запуску";
pub const PROVIDER_UNAVAILABLE: &str = "Текстовый провайдер недоступен";
pub const MODEL_UNAVAILABLE: &str = "Текстовая модель недоступна";
pub const OUTPUT_BUDGET: &str = "Бюджет ответа превышает ограничения модели";
pub const PROVIDERS_UNAVAILABLE: &str = "Сервис провайдеров недоступен";
pub const TOOLS_NOT_OFFERED: &str =
    "The model does not support tool calling; tools were not offered";
pub const BUDGET_EXHAUSTED: &str = "Бюджет вызовов инструментов на этот ход исчерпан";
pub const NOT_OFFERED: &str = "Модель вызвала инструмент, который не был ей предложен";
pub const NOT_AN_OBJECT: &str = "Аргументы вызова не являются JSON-объектом";
pub const NO_TOOL_CALLS: &str = "Вызов инструментов недоступен в этом запуске";
pub const TOOL_REPORTED_ERROR: &str = "Инструмент вернул ошибку";
pub const CALL_DENIED: &str = "Вызов отклонён пользователем";
pub const CALL_FORBIDDEN: &str = "Инструмент запрещён разрешением";
pub const RETRIEVAL_ASSEMBLED: &str = "Retrieval context assembled";
pub const HISTORY_TRIMMED: &str = "Chat history trimmed";
pub const MCP_INSTRUCTIONS_HEADING: &str = "Instructions published by the MCP servers whose tools are offered. Follow them when you use those tools; when they point to a resource (for example testix://…), read it with that server's read_resource tool before calling the tool that needs it.";
pub const NO_EVAL_CASES: &str = "Добавьте хотя бы один вопрос для проверки";
pub const WORKING_FOLDER_HEADING: &str = "Working folder";
pub const NO_WORKING_FOLDER_NOTE: &str = "This conversation has no working folder. Every folder needs the user's approval before you can read or change files in it.";
pub const WORKING_FOLDER_RULES: &str = "Relative paths resolve against the working folder. Any other folder needs the user's approval before you can read or change files in it.";
pub const WORKING_FOLDER_CHANGED: &str = "The working folder changed since the previous turn";
pub const FILE_TOOLS_HEADING: &str = "File tools";
pub const PROCESS_TOOLS_HEADING: &str = "Process tools";
pub const PROCESS_TOOLS_RULES: &str = "Use the code tool for a quick computation or conversion, and the shell tool to run the project's own build and test commands, then read their output to check a change. The user approves every run after seeing the full source or command. Pass a command as an argument vector, not a shell string: there are no pipes, redirections or glob expansion. Servers, background processes and interactive input are not supported, and each run has a time limit.";
pub const FILE_TOOLS_RULES: &str = "Read a file with the read tool in this turn before you change it; a change to a file that was not read, or that changed since it was read, is refused. Put every change of one task into a single apply call, so the user reviews and applies it as one diff. Paths may be absolute; relative paths resolve against the working folder.";

pub struct ChatServiceOptions {
    pub database: Arc<Database>,
    pub runs: RunService,
    pub registry: Arc<NodeRegistry>,
    pub tools: Option<ToolRegistry>,
    pub skills: Option<Arc<dyn SkillCatalogue>>,
    pub folders: Option<FolderAccessService>,
    pub logger: SharedLogger,
    pub clock: SharedClock,
}

impl ChatServiceOptions {
    pub fn new(database: Arc<Database>, runs: RunService, registry: Arc<NodeRegistry>) -> Self {
        Self {
            database,
            runs,
            registry,
            tools: None,
            skills: None,
            folders: None,
            logger: silent_logger(),
            clock: Arc::new(SystemClock),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Model {
    external_id: String,
    context_window: i64,
    max_output_tokens: i64,
    supports_tools: Option<bool>,
}

struct Inner {
    database: Arc<Database>,
    tools: Option<ToolRegistry>,
    skills: Option<Arc<dyn SkillCatalogue>>,
    folders: FolderAccessService,
    logger: SharedLogger,
    clock: SharedClock,
}

#[derive(Clone)]
pub struct ChatService {
    inner: Arc<Inner>,
    runs: RunService,
}

fn malformed(field: &str) -> AppError {
    AppError::new(
        AppErrorCode::Unknown,
        format!("Сохранённый диалог повреждён: {field}"),
    )
}

fn conversation_dto(
    row: ConversationRow,
    grants: &[FolderGrantRow],
) -> Result<ConversationDto, AppError> {
    let working_folder_access = folder_access(row.working_folder.as_deref(), grants, &row.id);
    Ok(ConversationDto {
        id: ConversationId::parse(&row.id).ok_or_else(|| malformed("id"))?,
        title: row.title,
        provider_id: ProviderId::parse(&row.provider_id).ok_or_else(|| malformed("providerId"))?,
        model_id: row.model_id,
        settings: row.settings,
        system_prompt: row.system_prompt,
        attached_store_ids: row
            .attached_store_ids
            .iter()
            .map(|id| VectorStoreId::parse(id).ok_or_else(|| malformed("attachedStoreIds")))
            .collect::<Result<_, _>>()?,
        use_relations: row.use_relations,
        enabled_tools: row.enabled_tools,
        retrieval_mode: row.retrieval_mode,
        working_folder: row.working_folder,
        working_folder_access,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub fn working_folder_section(folder: Option<&str>, previous: Option<Option<&str>>) -> String {
    let mut section = match folder {
        Some(folder) => format!("{WORKING_FOLDER_HEADING}: {folder}\n{WORKING_FOLDER_RULES}"),
        None => NO_WORKING_FOLDER_NOTE.to_owned(),
    };
    if let Some(previous) = previous
        && previous != folder
    {
        section.push('\n');
        section.push_str(&match previous {
            Some(previous) => format!("{WORKING_FOLDER_CHANGED}; it was {previous}."),
            None => format!("{WORKING_FOLDER_CHANGED}; there was none."),
        });
    }
    section
}

pub fn file_tools_section(offered: &[OfferedTool]) -> String {
    let names: Vec<&str> = FILE_TOOLS
        .iter()
        .filter_map(|key| offered.iter().find(|tool| tool.key == *key))
        .map(|tool| tool.name.as_str())
        .collect();
    if names.is_empty() {
        return String::new();
    }
    format!(
        "{FILE_TOOLS_HEADING}: {}\n{FILE_TOOLS_RULES}",
        names.join(", ")
    )
}

pub fn process_tools_section(offered: &[OfferedTool]) -> String {
    let names: Vec<&str> = PROCESS_TOOLS
        .iter()
        .filter_map(|key| offered.iter().find(|tool| tool.key == *key))
        .map(|tool| tool.name.as_str())
        .collect();
    if names.is_empty() {
        return String::new();
    }
    format!(
        "{PROCESS_TOOLS_HEADING}: {}
{PROCESS_TOOLS_RULES}",
        names.join(", ")
    )
}

fn message_dto(row: MessageRow) -> Result<MessageDto, AppError> {
    let count = |value: i64| u64::try_from(value).unwrap_or(0);
    Ok(MessageDto {
        id: MessageId::parse(&row.id).ok_or_else(|| malformed("message.id"))?,
        conversation_id: ConversationId::parse(&row.conversation_id)
            .ok_or_else(|| malformed("message.conversationId"))?,
        role: row.role,
        content: row.content,
        reasoning: row.reasoning,
        citations: row.citations,
        tool_calls: row.tool_calls,
        selections: row.selections,
        tokens_in: count(row.tokens_in),
        tokens_out: count(row.tokens_out),
        usage_estimated: row.usage_estimated,
        duration_ms: count(row.duration_ms),
        run_id: row.run_id.as_deref().and_then(RunId::parse),
        partial: row.partial,
        created_at: row.created_at,
    })
}

fn unique<T: Clone + Eq + std::hash::Hash>(values: &[T]) -> Vec<T> {
    let mut seen = HashSet::new();
    values
        .iter()
        .filter(|value| seen.insert((*value).clone()))
        .cloned()
        .collect()
}

fn as_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

impl Inner {
    fn now(&self) -> i64 {
        self.clock.now()
    }

    fn read<T>(
        &self,
        work: impl FnOnce(&Repositories<'_>) -> Result<T, DataError>,
    ) -> Result<T, AppError> {
        self.database
            .with_connection(|connection| work(&Repositories::new(connection)))
    }

    fn conversation(&self, id: &str) -> Result<ConversationRow, AppError> {
        self.read(|repositories| Ok(repositories.chat.get(id)?))?
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, CONVERSATION_NOT_FOUND))
    }

    fn dto(&self, row: ConversationRow) -> Result<ConversationDto, AppError> {
        let grants = self.read(|repositories| Ok(repositories.folder_grants.list()?))?;
        conversation_dto(row, &grants)
    }

    fn get(&self, id: &str) -> Result<ConversationDetailDto, AppError> {
        let (row, messages, grants) = self.read(|repositories| {
            Ok((
                repositories.chat.get(id)?,
                repositories.chat.messages(id)?,
                repositories.folder_grants.list()?,
            ))
        })?;
        let row =
            row.ok_or_else(|| AppError::new(AppErrorCode::NotFound, CONVERSATION_NOT_FOUND))?;
        Ok(ConversationDetailDto {
            conversation: conversation_dto(row, &grants)?,
            messages: messages
                .into_iter()
                .map(message_dto)
                .collect::<Result<_, _>>()?,
        })
    }

    fn update_message(&self, id: &str, patch: &MessagePatch) -> Result<(), AppError> {
        self.read(|repositories| Ok(repositories.chat.update_message(id, patch)?))
    }

    fn resolve_model(
        &self,
        provider_id: &str,
        model_id: &str,
        settings: &ChatSettings,
    ) -> Result<Model, AppError> {
        let (provider, models) = self.read(|repositories| {
            let provider = repositories.providers.find_by_id(provider_id)?;
            let models = match &provider {
                Some(provider) => repositories.models.list_by_provider(&provider.id, false)?,
                None => Vec::new(),
            };
            Ok((provider, models))
        })?;
        if !provider.is_some_and(|provider| {
            provider.enabled && provider.capabilities.contains(&ProviderCapability::Text)
        }) {
            return Err(AppError::new(
                AppErrorCode::ValidationFailed,
                PROVIDER_UNAVAILABLE,
            ));
        }
        let Some(model) = models
            .iter()
            .find(|model| model.id == model_id || model.external_id == model_id)
            .filter(|model| model.available)
        else {
            return Err(AppError::new(
                AppErrorCode::ValidationFailed,
                MODEL_UNAVAILABLE,
            ));
        };
        let capabilities = |row: &crate::data::schema::ModelRow| -> Vec<&'static str> {
            row.capabilities
                .iter()
                .map(|capability| capability.as_str())
                .collect()
        };
        let siblings: Vec<Vec<&str>> = models.iter().map(capabilities).collect();
        let sibling_slices: Vec<&[&str]> = siblings.iter().map(Vec::as_slice).collect();
        let supports_tools = model_tool_support(&capabilities(model), &sibling_slices);
        let context_window = model.context_window.unwrap_or(DEFAULT_CONTEXT_WINDOW);
        let max_output_tokens = settings.max_output_tokens.map_or_else(
            || {
                model
                    .max_output
                    .unwrap_or(DEFAULT_MAX_OUTPUT)
                    .min((context_window / 4).max(1))
            },
            as_i64,
        );
        if max_output_tokens >= context_window
            || model
                .max_output
                .is_some_and(|limit| max_output_tokens > limit)
        {
            return Err(AppError::new(AppErrorCode::ValidationFailed, OUTPUT_BUDGET));
        }
        Ok(Model {
            external_id: model.external_id.clone(),
            context_window,
            max_output_tokens,
            supports_tools,
        })
    }

    fn require_idle(&self, id: &str) -> Result<(), AppError> {
        let active = self.read(|repositories| Ok(repositories.runs.unfinished()?))?;
        if active
            .iter()
            .any(|run| run.kind == RunKind::Chat && run.subject_id.as_deref() == Some(id))
        {
            return Err(AppError::new(AppErrorCode::Conflict, ACTIVE_TURN));
        }
        Ok(())
    }

    fn require_stores(&self, ids: &[VectorStoreId]) -> Result<(), AppError> {
        for id in ids {
            let id = id.to_string();
            if self
                .read(|repositories| Ok(repositories.vector_stores.find_by_id(&id)?))?
                .is_none()
            {
                return Err(AppError::new(AppErrorCode::NotFound, STORE_NOT_FOUND));
            }
        }
        Ok(())
    }

    fn require_tools(&self, keys: &[String]) -> Result<(), AppError> {
        if keys.is_empty() {
            return Ok(());
        }
        let known: HashSet<String> = self
            .read(|repositories| Ok(repositories.tools.list()?))?
            .into_iter()
            .map(|tool| tool.key)
            .collect();
        for key in keys {
            if !known.contains(key) {
                return Err(AppError::new(AppErrorCode::NotFound, TOOL_NOT_FOUND)
                    .with_detail("key", key.clone()));
            }
        }
        Ok(())
    }

    fn require_turn(
        &self,
        run_id: &RunId,
        conversation_id: &str,
        user_id: &str,
    ) -> Result<(), AppError> {
        let run_key = run_id.to_string();
        let (run, user) = self.read(|repositories| {
            Ok((
                repositories.runs.get(&run_key)?,
                repositories.chat.get_message(user_id)?,
            ))
        })?;
        let valid = run.is_some_and(|run| {
            run.kind == RunKind::Chat && run.subject_id.as_deref() == Some(conversation_id)
        }) && user.is_some_and(|user| {
            user.run_id.as_deref() == Some(run_key.as_str()) && user.role == MessageRole::User
        });
        if valid {
            Ok(())
        } else {
            Err(AppError::new(AppErrorCode::Conflict, INVALID_TURN))
        }
    }

    fn log(&self, level: LogLevel, message: &str, fields: &[(&str, Value)]) {
        self.logger.log(level, "chat", message, fields);
    }

    fn record(&self, assistant_id: &str, event: &HostEvent) -> Result<(), AppError> {
        let Some(message) =
            self.read(|repositories| Ok(repositories.chat.get_message(assistant_id)?))?
        else {
            return Ok(());
        };
        let duration = (self.now() - message.created_at).max(0);
        match &event.body {
            HostEventDraft::Token { delta, kind } => {
                let reasoning = *kind == Some(DeltaKind::Reasoning);
                let patch = MessagePatch {
                    content: (!reasoning).then(|| format!("{}{delta}", message.content)),
                    reasoning: reasoning.then(|| format!("{}{delta}", message.reasoning)),
                    tokens_out: Some(message.tokens_out + tokens(delta)),
                    duration_ms: Some(duration),
                    ..MessagePatch::default()
                };
                self.update_message(assistant_id, &patch)
            }
            HostEventDraft::End { .. } => {
                self.update_message(
                    assistant_id,
                    &MessagePatch {
                        duration_ms: Some(duration),
                        ..MessagePatch::default()
                    },
                )?;
                let conversation = self.get(&message.conversation_id)?;
                let renamed = conversation
                    .messages
                    .iter()
                    .find(|item| item.role == MessageRole::User)
                    .filter(|_| {
                        conversation.conversation.title == DEFAULT_CONVERSATION_TITLE
                            && !message.content.is_empty()
                    })
                    .map(|first| {
                        let collapsed = SPACES.replace_all(&first.content, " ");
                        js_slice(&collapsed, TITLE_MAX).to_owned()
                    });
                let id = message.conversation_id.clone();
                let run_id = message.run_id.clone();
                let now = self.now();
                self.database.transaction(|transaction| {
                    let repositories = Repositories::new(transaction);
                    repositories.chat.update(
                        &id,
                        &ConversationPatch {
                            title: renamed.clone(),
                            ..ConversationPatch::touched(now)
                        },
                    )?;
                    if let (Some(title), Some(run_id)) = (&renamed, &run_id) {
                        repositories.runs.update(
                            run_id,
                            &RunPatch {
                                title: Some(title.clone()),
                                ..RunPatch::default()
                            },
                        )?;
                    }
                    Ok(())
                })
            }
            _ => Ok(()),
        }
    }

    fn mcp_guidance(
        &self,
        enabled: &mut Vec<String>,
        registered: &[ToolDto],
    ) -> Result<String, AppError> {
        let offered: HashSet<String> = offered_tools(enabled, registered)
            .into_iter()
            .map(|tool| tool.key)
            .collect();
        let servers = unique(
            &registered
                .iter()
                .filter(|tool| tool.source == ToolSource::Mcp && offered.contains(&tool.key))
                .filter_map(|tool| tool.source_ref.clone())
                .collect::<Vec<_>>(),
        );
        let mut blocks = Vec::new();
        for server_id in servers {
            let Some(row) =
                self.read(|repositories| Ok(repositories.mcp_servers.find_by_id(&server_id)?))?
            else {
                continue;
            };
            let reader = tool_key(&row.namespace, RESOURCE_READER);
            if registered.iter().any(|tool| tool.key == reader) && !enabled.contains(&reader) {
                enabled.push(reader);
            }
            if let Some(text) = row
                .instructions
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
            {
                let name = row.server_name.clone().unwrap_or(row.name);
                blocks.push(format!(
                    "From the MCP server «{name}» (tools {}__*):\n{}",
                    row.namespace,
                    js_slice(text, MCP_INSTRUCTIONS_MAX)
                ));
            }
        }
        Ok(if blocks.is_empty() {
            String::new()
        } else {
            format!("{MCP_INSTRUCTIONS_HEADING}\n\n{}", blocks.join("\n\n"))
        })
    }

    fn save_tool_calls(
        &self,
        context: &StepContext,
        assistant_id: &str,
        calls: &Mutex<Vec<ChatToolCallDto>>,
    ) {
        let snapshot = calls.lock().unwrap_or_else(PoisonError::into_inner).clone();
        if snapshot.is_empty() {
            return;
        }
        if let Err(error) = self.update_message(
            assistant_id,
            &MessagePatch {
                tool_calls: Some(snapshot.clone()),
                ..MessagePatch::default()
            },
        ) {
            self.log(
                LogLevel::Warn,
                "Could not save tool calls",
                &[
                    ("runId", json!(context.run_id.to_string())),
                    ("error", json!(error.message)),
                ],
            );
        }
        let mut step = serde_json::Map::new();
        step.insert("domain".to_owned(), json!("tool-calls"));
        step.insert("calls".to_owned(), json!(snapshot));
        let _ = context.emit(HostEventDraft::Step { step });
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatPrepareInput {
    pub conversation_id: String,
    pub user_id: String,
    pub assistant_id: String,
    pub provider_id: String,
    pub model_id: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "zvs_contract::wire::present"
    )]
    pub query: Option<String>,
    pub settings: ChatSettings,
    #[serde(flatten)]
    pub stores: BTreeMap<String, Vec<VectorSearchHitDto>>,
}

impl Validate for ChatPrepareInput {
    fn validate(&self) -> Result<(), AppError> {
        for (key, hits) in &self.stores {
            nested(key, hits.validate())?;
        }
        Ok(())
    }
}

const fn default_tool_result_chars() -> u64 {
    DEFAULT_TOOL_RESULT_CHARS
}

const fn unlimited_budget() -> i64 {
    i64::MAX
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeContext {
    #[serde(default)]
    pub store_ids: Vec<VectorStoreId>,
    #[serde(default)]
    pub citations: Vec<ChatCitationDto>,
    #[serde(default = "unlimited_budget")]
    pub context_budget: i64,
}

impl Default for KnowledgeContext {
    fn default() -> Self {
        Self {
            store_ids: Vec::new(),
            citations: Vec::new(),
            context_budget: unlimited_budget(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatGenerateRequest {
    #[serde(flatten)]
    pub base: GenerateNodeInput,
    pub assistant_id: String,
    #[serde(default)]
    pub tools: Vec<OfferedTool>,
    #[serde(default)]
    pub model_tools: Option<bool>,
    #[serde(default = "default_tool_result_chars")]
    pub tool_result_chars: u64,
    #[serde(default)]
    pub knowledge: KnowledgeContext,
    #[serde(default)]
    pub conversation_id: String,
}

impl Validate for ChatGenerateRequest {
    fn validate(&self) -> Result<(), AppError> {
        self.base.validate()?;
        nested("tools", self.tools.validate())?;
        check::safe_integer("toolResultChars", self.tool_result_chars)?;
        check::int_range("toolResultChars", self.tool_result_chars, 1, u64::MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatGenerateInput {
    pub request: ChatGenerateRequest,
}

impl Validate for ChatGenerateInput {
    fn validate(&self) -> Result<(), AppError> {
        nested("request", self.request.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatPersistInput {
    pub assistant_id: String,
    pub result: GenerateOutput,
}

impl Validate for ChatPersistInput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

struct PrepareNode(Arc<Inner>);
pub(crate) struct GenerateChatNode(Arc<Inner>);
struct PersistNode(Arc<Inner>);

const fn chat_role(role: MessageRole) -> ChatRole {
    match role {
        MessageRole::User => ChatRole::User,
        MessageRole::Assistant => ChatRole::Assistant,
    }
}

fn model_message(message: &MessageDto) -> ChatMessage {
    ChatMessage::new(
        chat_role(message.role),
        with_selections(&message.content, &message.selections),
    )
}

const fn generate_role(role: ChatRole) -> Option<GenerateRole> {
    match role {
        ChatRole::System => Some(GenerateRole::System),
        ChatRole::User => Some(GenerateRole::User),
        ChatRole::Assistant => Some(GenerateRole::Assistant),
        ChatRole::Tool => None,
    }
}

struct AttachedStore {
    id: VectorStoreId,
    name: String,
    description: String,
    documents: u64,
    floor: f64,
}

impl PrepareNode {
    async fn search_up_front(
        &self,
        context: &StepContext,
        query: &str,
        store_hits: &mut [StoreHits],
    ) -> Result<(), AppError> {
        let Some(vectors) = context.services.vector_stores.as_ref() else {
            return Ok(());
        };
        for entry in store_hits.iter_mut() {
            context.check_cancelled()?;
            let options = VectorSearchOptions {
                k: Some(CHAT_FETCH_K),
                ..VectorSearchOptions::default()
            };
            match vectors
                .search(&entry.store_id, query, options, &context.cancel)
                .await
            {
                Ok(hits) => entry.hits = hits,
                Err(error) if context.is_cancelled() => return Err(error),
                Err(error) => self.0.log(
                    LogLevel::Warn,
                    "Could not search a store up front",
                    &[
                        ("runId", json!(context.run_id.to_string())),
                        ("storeId", json!(entry.store_id)),
                        ("error", json!(error.to_string())),
                    ],
                ),
            }
        }
        Ok(())
    }

    async fn with_neighbours(
        &self,
        context: &StepContext,
        ranked: Vec<RetrievedSource>,
    ) -> Result<Vec<RetrievedSource>, AppError> {
        let Some(vectors) = context
            .services
            .vector_stores
            .as_ref()
            .filter(|vectors| vectors.has_chunks())
        else {
            return Ok(ranked);
        };
        if ranked.is_empty() {
            return Ok(ranked);
        }
        let mut order: Vec<(VectorStoreId, String)> = Vec::new();
        let mut wanted: HashMap<(VectorStoreId, String), Vec<u64>> = HashMap::new();
        for source in &ranked {
            let key = (source.store_id, source.hit.document_id.clone());
            let indices = wanted.entry(key.clone()).or_insert_with(|| {
                order.push(key);
                Vec::new()
            });
            let index = source.hit.chunk_index;
            for neighbour in [index.checked_sub(1), index.checked_add(1)]
                .into_iter()
                .flatten()
            {
                if !indices.contains(&neighbour) {
                    indices.push(neighbour);
                }
            }
        }
        let mut found: HashMap<(VectorStoreId, String), Vec<zvs_contract::VectorChunkDto>> =
            HashMap::new();
        for key in order {
            context.check_cancelled()?;
            let indices = wanted.remove(&key).unwrap_or_default();
            match vectors.chunks(&key.0.to_string(), &key.1, &indices).await {
                Ok(chunks) => {
                    found.insert(key, chunks);
                }
                Err(error) => self.0.log(
                    LogLevel::Warn,
                    "Could not fetch neighbouring chunks",
                    &[
                        ("runId", json!(context.run_id.to_string())),
                        ("storeId", json!(key.0.to_string())),
                        ("error", json!(error.to_string())),
                    ],
                ),
            }
        }
        Ok(ranked
            .into_iter()
            .map(|source| {
                let neighbours = found
                    .get(&(source.store_id, source.hit.document_id.clone()))
                    .map(|chunks| {
                        chunks
                            .iter()
                            .filter(|chunk| chunk.chunk_index.abs_diff(source.hit.chunk_index) == 1)
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                RetrievedSource {
                    neighbours,
                    ..source
                }
            })
            .collect())
    }

    async fn related_sources(
        &self,
        context: &StepContext,
        query: &str,
        ranked: &[RetrievedSource],
    ) -> Result<Vec<RelatedSource>, AppError> {
        let (Some(graph), Some(vectors)) = (
            context.services.graph.as_ref(),
            context
                .services
                .vector_stores
                .as_ref()
                .filter(|vectors| vectors.has_related_chunks()),
        ) else {
            return Ok(Vec::new());
        };
        let mut related = Vec::new();
        let store_ids = unique(
            &ranked
                .iter()
                .map(|source| source.store_id)
                .collect::<Vec<_>>(),
        );
        for store_id in store_ids {
            context.check_cancelled()?;
            let hits: Vec<&VectorSearchHitDto> = ranked
                .iter()
                .filter(|source| source.store_id == store_id)
                .map(|source| &source.hit)
                .collect();
            let attempt = async {
                let documents = unique(
                    &hits
                        .iter()
                        .map(|hit| hit.document_id.clone())
                        .collect::<Vec<_>>(),
                );
                let mut neighborhoods = Vec::new();
                for document_id in documents {
                    let document = DocumentId::parse(&document_id).ok_or_else(|| {
                        AppError::new(AppErrorCode::ValidationFailed, "Некорректный документ")
                    })?;
                    neighborhoods.push(GraphNeighborhood {
                        neighbors: graph
                            .neighbors(&GraphNeighborsInput {
                                direction: Some(GraphDirection::Both),
                                ..GraphNeighborsInput::new(store_id, document)
                            })
                            .await?,
                        document_id,
                    });
                }
                let store_key = store_id.to_string();
                let documents: Vec<GraphExpansionDocument> = self
                    .0
                    .read(|repositories| {
                        Ok(repositories.vector_documents.list_by_store(&store_key)?)
                    })?
                    .into_iter()
                    .map(|document| GraphExpansionDocument {
                        id: document.id,
                        source_path: document.source_path,
                        chunk_count: u64::try_from(document.chunk_count).unwrap_or(0),
                    })
                    .collect();
                let ranked_ids: Vec<&str> =
                    hits.iter().map(|hit| hit.document_id.as_str()).collect();
                let expansions = merge_graph_neighbors(
                    &ranked_ids,
                    &neighborhoods,
                    &documents,
                    CHAT_RELATED_MAX.min(GRAPH_EXPANSION_DEFAULT_MAX),
                );
                if expansions.is_empty() {
                    return Ok(Vec::new());
                }
                let best = vectors
                    .related_chunks(
                        &store_key,
                        query,
                        &expansions
                            .iter()
                            .map(|expansion| expansion.document_id.clone())
                            .collect::<Vec<_>>(),
                        &context.cancel,
                    )
                    .await?;
                Ok::<_, AppError>(
                    expansions
                        .into_iter()
                        .filter_map(|expansion| {
                            best.iter()
                                .find(|candidate| candidate.document_id == expansion.document_id)
                                .map(|hit| RelatedSource {
                                    store_id,
                                    hit: hit.clone(),
                                    relation: expansion.relation,
                                })
                        })
                        .collect(),
                )
            }
            .await;
            match attempt {
                Ok(found) => related.extend(found),
                Err(error) if context.is_cancelled() => return Err(error),
                Err(error) => self.0.log(
                    LogLevel::Warn,
                    "Could not expand sources through the graph",
                    &[
                        ("runId", json!(context.run_id.to_string())),
                        ("storeId", json!(store_id.to_string())),
                        ("error", json!(error.to_string())),
                    ],
                ),
            }
        }
        Ok(related)
    }
}

#[async_trait]
impl NodeDef for PrepareNode {
    type Input = ChatPrepareInput;
    type Output = ChatGenerateRequest;

    fn node_type(&self) -> &str {
        "chat.prepare"
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::None
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(
        &self,
        context: &StepContext,
        input: ChatPrepareInput,
    ) -> Result<ChatGenerateRequest, AppError> {
        context.check_cancelled()?;
        let inner = &self.0;
        let conversation = inner.get(&input.conversation_id)?;
        inner.require_turn(&context.run_id, &input.conversation_id, &input.user_id)?;
        let position = conversation
            .messages
            .iter()
            .position(|message| message.id.to_string() == input.user_id)
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, INVALID_TURN))?;
        let user = &conversation.messages[position];
        let history: Vec<ChatMessage> = conversation.messages[..position]
            .iter()
            .map(model_message)
            .collect();
        let model = inner.resolve_model(&input.provider_id, &input.model_id, &input.settings)?;
        let details = &conversation.conversation;
        let attached: Vec<AttachedStore> = details
            .attached_store_ids
            .iter()
            .map(|store_id| {
                let id = store_id.to_string();
                let (row, documents) = inner
                    .read(|repositories| {
                        Ok((
                            repositories.vector_stores.find_by_id(&id)?,
                            repositories.vector_documents.list_by_store(&id)?.len(),
                        ))
                    })
                    .unwrap_or((None, 0));
                AttachedStore {
                    id: *store_id,
                    name: row
                        .as_ref()
                        .map_or_else(|| id.clone(), |row| row.name.clone()),
                    description: row
                        .as_ref()
                        .map(|row| row.description.clone())
                        .unwrap_or_default(),
                    documents: documents as u64,
                    floor: row.map_or(DEFAULT_CHAT_MIN_SCORE, |row| row.chat_min_score),
                }
            })
            .collect();
        let mode = details.retrieval_mode;
        let tools_usable = context
            .services
            .providers
            .as_ref()
            .and_then(|providers| providers.text(&input.provider_id).ok())
            .is_some_and(|driver| {
                resolve_tool_mode(driver.capabilities().tools, model.supports_tools)
                    != ToolMode::None
            });
        let registered = match &inner.tools {
            Some(tools) => tools.list(&ToolListFilter::default())?,
            None => Vec::new(),
        };
        let wants_knowledge = !attached.is_empty() && mode != RetrievalMode::Auto && tools_usable;
        let mut enabled = details.enabled_tools.clone();
        if wants_knowledge {
            for key in KNOWLEDGE_TOOLS {
                if !enabled.iter().any(|known| known == key) {
                    enabled.push(key.to_owned());
                }
            }
        }
        let skill_entries = match (&inner.skills, tools_usable) {
            (Some(skills), true) => skills.catalogue(&user.content, &context.cancel).await,
            _ => Vec::new(),
        };
        if !skill_entries.is_empty() {
            for key in SKILL_TOOLS {
                if !enabled.iter().any(|known| known == key) {
                    enabled.push(key.to_owned());
                }
            }
        }
        let guidance = inner.mcp_guidance(&mut enabled, &registered)?;
        let offered = offered_tools(&enabled, &registered);
        let skill_note =
            if !skill_entries.is_empty() && offered.iter().any(|tool| tool.key == SKILL_LOAD) {
                catalogue_text(&skill_entries, &offered)
            } else {
                String::new()
            };
        let knowledge_on =
            wants_knowledge && offered.iter().any(|tool| tool.key == KNOWLEDGE_SEARCH);
        let query = input.query.clone().unwrap_or_else(|| user.content.clone());
        let mut store_hits: Vec<StoreHits> = attached
            .iter()
            .enumerate()
            .map(|(index, store)| StoreHits {
                store_id: store.id.to_string(),
                hits: input
                    .stores
                    .get(&format!("store{index}"))
                    .cloned()
                    .unwrap_or_default(),
                floor: store.floor,
            })
            .collect();
        if mode == RetrievalMode::Agentic && !knowledge_on && input.stores.is_empty() {
            self.search_up_front(context, &query, &mut store_hits)
                .await?;
        }
        let merged = merge_retrieval(
            &store_hits,
            if mode == RetrievalMode::Hybrid && knowledge_on {
                MergeOptions {
                    limit: HYBRID_SOURCES_MAX,
                    ..MergeOptions::default()
                }
            } else {
                MergeOptions::default()
            },
        );
        let ranked: Vec<RetrievedSource> = merged
            .hits
            .iter()
            .filter_map(|merged| {
                VectorStoreId::parse(&merged.store_id)
                    .map(|store_id| RetrievedSource::new(store_id, merged.hit.clone()))
            })
            .collect();
        let sources = self.with_neighbours(context, ranked.clone()).await?;
        let related = if details.use_relations && !ranked.is_empty() {
            self.related_sources(context, &query, &ranked).await?
        } else {
            Vec::new()
        };
        let prompt = if knowledge_on {
            let catalogue = knowledge_catalogue(
                &attached
                    .iter()
                    .map(|store| {
                        (
                            store.name.clone(),
                            store.description.clone(),
                            store.documents,
                        )
                    })
                    .collect::<Vec<_>>(),
            );
            let hybrid = if mode == RetrievalMode::Hybrid && !ranked.is_empty() {
                format!(" {HYBRID_INSTRUCTION}")
            } else {
                String::new()
            };
            format!("{KNOWLEDGE_INSTRUCTION}{hybrid}\n{catalogue}")
        } else {
            String::new()
        };
        let row = inner.conversation(&input.conversation_id)?;
        let previous = (position > 0).then_some(row.turn_folder.as_deref());
        let changed = previous.is_some_and(|previous| previous != row.working_folder.as_deref());
        let workspace = if offered.is_empty() && row.working_folder.is_none() && !changed {
            String::new()
        } else {
            working_folder_section(row.working_folder.as_deref(), previous)
        };
        if row.turn_folder != row.working_folder {
            inner.read(|repositories| {
                Ok(repositories.chat.update(
                    &input.conversation_id,
                    &ConversationPatch {
                        turn_folder: Some(row.working_folder.clone()),
                        ..ConversationPatch::default()
                    },
                )?)
            })?;
        }
        let files = file_tools_section(&offered);
        let processes = process_tools_section(&offered);
        let prompt = [
            details.system_prompt.clone(),
            prompt,
            guidance,
            skill_note,
            workspace,
            files,
            processes,
        ]
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
        let system = if offered.is_empty() {
            prompt.clone()
        } else {
            format!(
                "{prompt}\n\n{TOOL_RESULTS_INSTRUCTION}\n{}",
                tool_specs_json(&offered)
            )
        };
        let prepared = window_chat_history(
            &system,
            &history,
            &ChatMessage::user(with_selections(&user.content, &user.selections)),
            model.context_window,
            model.max_output_tokens,
            &sources,
            &related,
        )?;
        let run_id = context.run_id.to_string();
        if !details.attached_store_ids.is_empty() {
            let stats = merged.stats;
            let fields = [
                ("message", json!(RETRIEVAL_ASSEMBLED)),
                ("conversationId", json!(details.id.to_string())),
                ("mode", json!(mode.as_str())),
                ("knowledgeTools", json!(knowledge_on)),
                ("query", json!(query)),
                ("fetched", json!(stats.fetched)),
                ("belowFloor", json!(stats.below_floor)),
                ("duplicates", json!(stats.duplicates)),
                ("overDocumentCap", json!(stats.over_cap)),
                ("overLimit", json!(stats.over_limit)),
                (
                    "injected",
                    json!(prepared.citations.len() - prepared.related),
                ),
                ("droppedByBudget", json!(prepared.dropped_by_budget)),
                ("neighbours", json!(prepared.neighbours)),
                ("related", json!(prepared.related)),
            ];
            context.emit(HostEventDraft::Log {
                line: log_line(&fields),
            })?;
            let mut logged = fields.to_vec();
            logged.push(("runId", json!(run_id)));
            inner.log(LogLevel::Info, RETRIEVAL_ASSEMBLED, &logged);
        }
        if prepared.trimmed > 0 {
            let fields = [
                ("message", json!(HISTORY_TRIMMED)),
                ("conversationId", json!(details.id.to_string())),
                ("trimmed", json!(prepared.trimmed)),
            ];
            context.emit(HostEventDraft::Log {
                line: log_line(&fields),
            })?;
            let mut logged = fields.to_vec();
            logged.push(("runId", json!(run_id)));
            inner.log(LogLevel::Info, HISTORY_TRIMMED, &logged);
        }
        let now = inner.now();
        let row = MessageRow {
            id: input.assistant_id.clone(),
            conversation_id: details.id.to_string(),
            role: MessageRole::Assistant,
            content: String::new(),
            reasoning: String::new(),
            citations: prepared.citations.clone(),
            tool_calls: Vec::new(),
            selections: Vec::new(),
            tokens_in: prepared.tokens_in,
            tokens_out: 0,
            usage_estimated: true,
            duration_ms: 0,
            run_id: Some(run_id),
            partial: true,
            created_at: now,
        };
        inner.read(|repositories| Ok(repositories.chat.add_message(&row)?))?;
        Ok(ChatGenerateRequest {
            base: GenerateNodeInput {
                provider_id: input.provider_id.clone(),
                model: model.external_id,
                messages: prepared
                    .messages
                    .iter()
                    .filter_map(|message| {
                        generate_role(message.role).map(|role| GenerateMessage {
                            role,
                            content: message.content.clone(),
                        })
                    })
                    .collect(),
                system: Some(prompt),
                temperature: input.settings.temperature,
                top_k: input.settings.top_k,
                top_p: input.settings.top_p,
                max_output_tokens: u64::try_from(model.max_output_tokens).ok(),
            },
            assistant_id: input.assistant_id,
            tools: offered,
            model_tools: model.supports_tools,
            tool_result_chars: tool_result_budget(
                model.context_window - model.max_output_tokens - prepared.tokens_in,
            ) as u64,
            knowledge: KnowledgeContext {
                store_ids: details.attached_store_ids.clone(),
                citations: prepared.citations,
                context_budget: model.context_window - model.max_output_tokens,
            },
            conversation_id: input.conversation_id.clone(),
        })
    }
}

pub(crate) struct RoundOutput {
    pub(crate) text: String,
    pub(crate) reasoning: String,
    pub(crate) tool_calls: Vec<ToolCall>,
}

struct CallOptions<'a> {
    tool: Option<&'a OfferedTool>,
    label: String,
    limit: usize,
    refuse: Option<&'static str>,
    scope: &'a [VectorStoreId],
    book: &'a Mutex<Vec<ChatCitationDto>>,
    conversation: &'a str,
}

struct PendingCall<'a> {
    call: ToolCall,
    tool: Option<&'a OfferedTool>,
    index: usize,
    refuse: Option<&'static str>,
}

struct CallOutcome {
    content: String,
    compact: Option<String>,
}

fn knowledge_text(
    key: &str,
    output: &Value,
    book: &mut Vec<ChatCitationDto>,
) -> Option<KnowledgeText> {
    match key {
        KNOWLEDGE_SEARCH => serde_json::from_value::<KnowledgeSearchOutput>(output.clone())
            .ok()
            .map(|output| search_result_text(&output, book)),
        KNOWLEDGE_READ => serde_json::from_value::<KnowledgeReadOutput>(output.clone())
            .ok()
            .map(|output| read_result_text(&output, book)),
        _ => None,
    }
}

struct Compactable {
    index: usize,
    round: usize,
    compact: String,
}

fn messages_cost(messages: &[ChatMessage]) -> i64 {
    messages
        .iter()
        .map(|message| tokens(&message.content) + MESSAGE_OVERHEAD)
        .sum()
}

fn compact_knowledge(
    messages: &mut [ChatMessage],
    pending: &mut Vec<Compactable>,
    round: usize,
    budget: i64,
) -> usize {
    let mut compacted = 0;
    while messages_cost(messages) > budget {
        let Some(position) = pending.iter().position(|entry| entry.round < round) else {
            break;
        };
        let entry = pending.remove(position);
        if let Some(message) = messages.get_mut(entry.index) {
            message.content = entry.compact;
            compacted += 1;
        }
    }
    compacted
}

impl GenerateChatNode {
    pub(crate) async fn generate_round(
        context: &StepContext,
        driver: &SharedTextDriver,
        request: &GenerateRequest,
        before: &str,
    ) -> Result<RoundOutput, AppError> {
        let mut output = RoundOutput {
            text: String::new(),
            reasoning: String::new(),
            tool_calls: Vec::new(),
        };
        let mut deltas = driver.stream(request, &context.cancel);
        while let Some(delta) = deltas.next().await {
            let delta = delta?;
            context.check_cancelled()?;
            match delta {
                TextDelta::ToolCall(call) => {
                    let id = if call.id.is_empty() {
                        format!("call_{}", output.tool_calls.len() + 1)
                    } else {
                        call.id
                    };
                    output.tool_calls.push(ToolCall { id, ..call });
                }
                TextDelta::Content { text, .. } if text.is_empty() => {}
                TextDelta::Content {
                    text,
                    kind: DeltaKind::Reasoning,
                } => {
                    output.reasoning.push_str(&text);
                    context.emit(HostEventDraft::Token {
                        delta: text,
                        kind: Some(DeltaKind::Reasoning),
                    })?;
                }
                TextDelta::Content {
                    text,
                    kind: DeltaKind::Text,
                } => {
                    if output.text.is_empty() && !before.is_empty() && !before.ends_with('\n') {
                        output.text.push_str("\n\n");
                        context.emit(HostEventDraft::Token {
                            delta: "\n\n".to_owned(),
                            kind: Some(DeltaKind::Text),
                        })?;
                    }
                    output.text.push_str(&text);
                    context.emit(HostEventDraft::Token {
                        delta: text,
                        kind: Some(DeltaKind::Text),
                    })?;
                }
            }
        }
        Ok(output)
    }

    async fn call_tool(
        &self,
        context: &StepContext,
        assistant_id: &str,
        calls: &Arc<Mutex<Vec<ChatToolCallDto>>>,
        index: usize,
        options: CallOptions<'_>,
    ) -> Result<CallOutcome, AppError> {
        let plain = |content: String| CallOutcome {
            content,
            compact: None,
        };
        let inner = &self.0;
        let started = inner.now();
        let limit = options.limit;
        let settle =
            |status: ChatToolCallStatus, error: Option<&str>, result: Option<String>| -> String {
                {
                    let mut guard = calls.lock().unwrap_or_else(PoisonError::into_inner);
                    let record = &mut guard[index];
                    let preview = result
                        .as_deref()
                        .map(|text| cap_text(text, TOOL_PREVIEW_CHARS));
                    record.status = status;
                    record.error = error.map(str::to_owned);
                    record.truncated = preview.as_ref().is_some_and(|preview| preview.truncated);
                    record.result = preview.map(|preview| preview.text);
                    record.duration_ms = u64::try_from(inner.now() - started).unwrap_or(0);
                }
                inner.save_tool_calls(context, assistant_id, calls);
                match result {
                    None => format!(
                        "Tool call {}: {}",
                        status.as_str(),
                        error.unwrap_or("no result")
                    ),
                    Some(result) => {
                        let capped = cap_text(&result, limit);
                        if capped.truncated {
                            format!("{}\n[result truncated to {limit} characters]", capped.text)
                        } else {
                            capped.text
                        }
                    }
                }
            };
        inner.save_tool_calls(context, assistant_id, calls);
        if let Some(reason) = options.refuse {
            return Ok(plain(settle(
                ChatToolCallStatus::Refused,
                Some(reason),
                None,
            )));
        }
        let Some(tool) = options.tool else {
            return Ok(plain(settle(
                ChatToolCallStatus::Refused,
                Some(NOT_OFFERED),
                None,
            )));
        };
        let mut arguments = calls.lock().unwrap_or_else(PoisonError::into_inner)[index]
            .arguments
            .clone();
        let Value::Object(fields) = &mut arguments else {
            return Ok(plain(settle(
                ChatToolCallStatus::Failed,
                Some(NOT_AN_OBJECT),
                None,
            )));
        };
        let knowledge = KNOWLEDGE_TOOLS.contains(&tool.key.as_str());
        if knowledge {
            fields.insert("scope".to_owned(), json!(options.scope));
        }
        if FILE_TOOLS.contains(&tool.key.as_str()) || PROCESS_TOOLS.contains(&tool.key.as_str()) {
            fields.insert("conversationId".to_owned(), json!(options.conversation));
        }
        let observer_inner = Arc::clone(inner);
        let observer_context = context.clone();
        let observer_calls = Arc::clone(calls);
        let observer_assistant = assistant_id.to_owned();
        let request = ToolStepRequest {
            key: tool.key.clone(),
            input: arguments,
            label: options.label,
            minimum_tier: tool.destructive.then_some(ToolTier::Ask),
            on_approval: Some(Arc::new(move |approval, decision| {
                {
                    let mut guard = observer_calls
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    let record = &mut guard[index];
                    record.approval_id = approval
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    record.status = match decision {
                        None => ChatToolCallStatus::AwaitingApproval,
                        Some(ApprovalDecision::Approved | ApprovalDecision::Denied) => {
                            ChatToolCallStatus::Running
                        }
                    };
                }
                observer_inner.save_tool_calls(
                    &observer_context,
                    &observer_assistant,
                    &observer_calls,
                );
            })),
        };
        let Some(invocation) = context.invoke_tool(request) else {
            return Ok(plain(settle(
                ChatToolCallStatus::Refused,
                Some(NO_TOOL_CALLS),
                None,
            )));
        };
        match invocation.await {
            Ok(result) => {
                {
                    let mut guard = calls.lock().unwrap_or_else(PoisonError::into_inner);
                    guard[index].step_id = Some(result.step_id);
                    guard[index].meta = result.meta;
                }
                if knowledge {
                    let formatted = {
                        let mut book = options.book.lock().unwrap_or_else(PoisonError::into_inner);
                        knowledge_text(&tool.key, &result.output, &mut book)
                            .map(|formatted| (formatted, book.clone()))
                    };
                    if let Some((formatted, citations)) = formatted {
                        if let Err(error) = inner.update_message(
                            assistant_id,
                            &MessagePatch {
                                citations: Some(citations),
                                ..MessagePatch::default()
                            },
                        ) {
                            inner.log(
                                LogLevel::Warn,
                                "Could not save knowledge citations",
                                &[
                                    ("runId", json!(context.run_id.to_string())),
                                    ("error", json!(error.message)),
                                ],
                            );
                        }
                        return Ok(CallOutcome {
                            content: settle(
                                ChatToolCallStatus::Succeeded,
                                None,
                                Some(formatted.text),
                            ),
                            compact: Some(formatted.compact),
                        });
                    }
                }
                let text = tool_result_text(&result.output);
                Ok(plain(if text.is_error {
                    settle(
                        ChatToolCallStatus::Failed,
                        Some(TOOL_REPORTED_ERROR),
                        Some(text.text),
                    )
                } else {
                    settle(ChatToolCallStatus::Succeeded, None, Some(text.text))
                }))
            }
            Err(error) if context.is_cancelled() => Err(error),
            Err(error) if error.code == AppErrorCode::ApprovalDenied => Ok(plain(settle(
                ChatToolCallStatus::Denied,
                Some(CALL_DENIED),
                None,
            ))),
            Err(error) if error.code == AppErrorCode::PermissionDenied && !knowledge => Ok(plain(
                settle(ChatToolCallStatus::Refused, Some(CALL_FORBIDDEN), None),
            )),
            Err(error) => Ok(plain(settle(
                ChatToolCallStatus::Failed,
                Some(&error.message),
                None,
            ))),
        }
    }

    async fn respond(
        &self,
        context: &StepContext,
        request: ChatGenerateRequest,
    ) -> Result<GenerateOutput, AppError> {
        let Some(providers) = &context.services.providers else {
            return Err(AppError::new(AppErrorCode::Conflict, PROVIDERS_UNAVAILABLE));
        };
        let ChatGenerateRequest {
            base,
            assistant_id,
            tools,
            model_tools,
            tool_result_chars,
            knowledge,
            conversation_id,
        } = request;
        let book = Mutex::new(knowledge.citations.clone());
        let mut compactable: Vec<Compactable> = Vec::new();
        let raw = providers.text(&base.provider_id)?;
        let mode = if tools.is_empty() {
            ToolMode::None
        } else {
            resolve_tool_mode(raw.capabilities().tools, model_tools)
        };
        let offered: Vec<OfferedTool> = if mode == ToolMode::None {
            Vec::new()
        } else {
            tools.clone()
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
        if !tools.is_empty() && offered.is_empty() {
            context.emit(HostEventDraft::Log {
                line: log_line(&[("message", json!(TOOLS_NOT_OFFERED))]),
            })?;
        }
        let prompt = base.system.clone();
        let system = if specs.is_empty() {
            prompt
        } else {
            Some(
                [
                    prompt.unwrap_or_default(),
                    TOOL_RESULTS_INSTRUCTION.to_owned(),
                ]
                .into_iter()
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n\n"),
            )
        };
        let template = GenerateRequest {
            system,
            tools: specs.clone(),
            ..base.clone().request()
        };
        let mut messages = template.messages.clone();
        let calls: Arc<Mutex<Vec<ChatToolCallDto>>> = Arc::new(Mutex::new(Vec::new()));
        let limit = usize::try_from(tool_result_chars).unwrap_or(usize::MAX);
        let mut text = String::new();
        let mut reasoning = String::new();
        let mut executed = 0;
        let mut round = 0;
        let outcome: Result<(), AppError> = async {
            loop {
                let last = round >= CHAT_TOOL_ROUNDS || executed >= CHAT_TOOL_CALLS;
                if last && !specs.is_empty() {
                    messages.push(ChatMessage::user(TOOL_BUDGET_NOTE));
                }
                let overhead = tokens(template.system.as_deref().unwrap_or_default())
                    + tokens(&tool_specs_json(&offered));
                let compacted = compact_knowledge(
                    &mut messages,
                    &mut compactable,
                    round,
                    knowledge.context_budget.saturating_sub(overhead),
                );
                if compacted > 0 {
                    context.emit(HostEventDraft::Log {
                        line: log_line(&[
                            ("message", json!(KNOWLEDGE_COMPACTED)),
                            ("compacted", json!(compacted)),
                            ("round", json!(round)),
                        ]),
                    })?;
                }
                let output = Self::generate_round(
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
                let mut pending = Vec::with_capacity(output.tool_calls.len());
                for call in output.tool_calls {
                    let tool = by_name.get(call.name.as_str()).copied();
                    let index = {
                        let mut guard = calls.lock().unwrap_or_else(PoisonError::into_inner);
                        guard.push(ChatToolCallDto {
                            id: call.id.clone(),
                            key: tool.map_or_else(|| call.name.clone(), |tool| tool.key.clone()),
                            name: call.name.clone(),
                            round: round as u64,
                            arguments: parse_tool_arguments(&call.arguments).map_or_else(
                                || Value::String(call.arguments.clone()),
                                Value::Object,
                            ),
                            status: ChatToolCallStatus::Running,
                            result: None,
                            truncated: false,
                            error: None,
                            approval_id: None,
                            step_id: None,
                            duration_ms: 0,
                            meta: None,
                        });
                        guard.len() - 1
                    };
                    let refuse = (last || executed >= CHAT_TOOL_CALLS).then_some(BUDGET_EXHAUSTED);
                    if refuse.is_none() {
                        executed += 1;
                    }
                    pending.push(PendingCall {
                        call,
                        tool,
                        index,
                        refuse,
                    });
                }
                let concurrent = |entry: &PendingCall<'_>| {
                    entry.refuse.is_none() && entry.tool.is_some_and(|tool| tool.read_only)
                };
                let mut position = 0;
                while position < pending.len() {
                    let mut end = position + 1;
                    if concurrent(&pending[position]) {
                        while end < pending.len() && concurrent(&pending[end]) {
                            end += 1;
                        }
                    }
                    let group = &pending[position..end];
                    let outcomes = join_all(group.iter().map(|entry| {
                        self.call_tool(
                            context,
                            &assistant_id,
                            &calls,
                            entry.index,
                            CallOptions {
                                tool: entry.tool,
                                label: format!("call{}", entry.index + 1),
                                limit,
                                refuse: entry.refuse,
                                scope: &knowledge.store_ids,
                                book: &book,
                                conversation: &conversation_id,
                            },
                        )
                    }))
                    .await;
                    for (entry, outcome) in group.iter().zip(outcomes) {
                        let outcome = outcome?;
                        let status = calls.lock().unwrap_or_else(PoisonError::into_inner)
                            [entry.index]
                            .status;
                        let shape = |content: String| {
                            if mode == ToolMode::Native {
                                fence_tool_result(&entry.call.name, status.as_str(), &content)
                            } else {
                                content
                            }
                        };
                        if let Some(compact) = outcome.compact {
                            compactable.push(Compactable {
                                index: messages.len(),
                                round,
                                compact: shape(compact),
                            });
                        }
                        messages.push(ChatMessage::tool(
                            entry.call.id.clone(),
                            shape(outcome.content),
                        ));
                    }
                    position = end;
                }
                if last {
                    break;
                }
                round += 1;
            }
            Ok(())
        }
        .await;
        if let Err(error) = outcome {
            {
                let mut guard = calls.lock().unwrap_or_else(PoisonError::into_inner);
                for call in guard.iter_mut() {
                    if matches!(
                        call.status,
                        ChatToolCallStatus::Running | ChatToolCallStatus::AwaitingApproval
                    ) {
                        call.status = ChatToolCallStatus::Cancelled;
                        call.error = Some(if context.is_cancelled() {
                            crate::kernel::types::CANCELLED_MESSAGE.to_owned()
                        } else {
                            error.message.clone()
                        });
                    }
                }
            }
            self.0.save_tool_calls(context, &assistant_id, &calls);
            return Err(error);
        }
        Ok(GenerateOutput { text, reasoning })
    }
}

#[async_trait]
impl NodeDef for GenerateChatNode {
    type Input = ChatGenerateInput;
    type Output = GenerateOutput;

    fn node_type(&self) -> &str {
        "chat.generate"
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
        input: ChatGenerateInput,
    ) -> Result<GenerateOutput, AppError> {
        self.respond(context, input.request).await
    }
}

#[async_trait]
impl NodeDef for PersistNode {
    type Input = ChatPersistInput;
    type Output = MessageDto;

    fn node_type(&self) -> &str {
        "chat.persist"
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::None
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(
        &self,
        context: &StepContext,
        input: ChatPersistInput,
    ) -> Result<MessageDto, AppError> {
        context.check_cancelled()?;
        let inner = &self.0;
        let run_id = context.run_id.to_string();
        let message = inner
            .read(|repositories| Ok(repositories.chat.get_message(&input.assistant_id)?))?
            .filter(|message| message.run_id.as_deref() == Some(run_id.as_str()))
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, FOREIGN_MESSAGE))?;
        let tokens_out =
            estimate_tokens(&format!("{}{}", input.result.text, input.result.reasoning));
        inner.update_message(
            &message.id,
            &MessagePatch {
                content: Some(input.result.text),
                reasoning: Some(input.result.reasoning),
                tokens_out: Some(as_i64(tokens_out)),
                partial: Some(false),
                duration_ms: Some((inner.now() - message.created_at).max(0)),
                ..MessagePatch::default()
            },
        )?;
        let saved = inner
            .read(|repositories| Ok(repositories.chat.get_message(&message.id)?))?
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, MESSAGE_NOT_FOUND))?;
        message_dto(saved)
    }
}

impl ChatService {
    pub fn new(options: ChatServiceOptions) -> Result<Self, AppError> {
        let folders = options.folders.unwrap_or_else(|| {
            FolderAccessService::new(
                Arc::clone(&options.database),
                options.runs.approvals(),
                ForbiddenFolders::from_environment(&[]),
                options.clock.clone(),
            )
        });
        let inner = Arc::new(Inner {
            database: options.database,
            tools: options.tools,
            skills: options.skills,
            folders,
            logger: options.logger,
            clock: options.clock,
        });
        options.registry.register(PrepareNode(Arc::clone(&inner)))?;
        options
            .registry
            .register(GenerateChatNode(Arc::clone(&inner)))?;
        options.registry.register(PersistNode(Arc::clone(&inner)))?;
        Ok(Self {
            inner,
            runs: options.runs,
        })
    }

    pub fn list(&self) -> Result<Vec<ConversationDto>, AppError> {
        let (rows, grants) = self.inner.read(|repositories| {
            Ok((
                repositories.chat.list()?,
                repositories.folder_grants.list()?,
            ))
        })?;
        rows.into_iter()
            .map(|row| conversation_dto(row, &grants))
            .collect()
    }

    pub fn folders(&self) -> FolderAccessService {
        self.inner.folders.clone()
    }

    pub fn get(&self, id: &ConversationId) -> Result<ConversationDetailDto, AppError> {
        self.inner.get(&id.to_string())
    }

    pub fn create(&self, input: &CreateConversationInput) -> Result<ConversationDto, AppError> {
        input.validate()?;
        let settings = input.settings();
        self.inner
            .resolve_model(&input.provider_id.to_string(), &input.model_id, &settings)?;
        let stores = input.attached_store_ids();
        self.inner.require_stores(&stores)?;
        let tools = input.enabled_tools();
        self.inner.require_tools(&tools)?;
        let now = self.inner.now();
        let row = ConversationRow {
            id: new_id().hyphenated().to_string(),
            title: input.title(),
            provider_id: input.provider_id.to_string(),
            model_id: input.model_id.clone(),
            settings,
            system_prompt: input.system_prompt(),
            attached_store_ids: unique(&stores).iter().map(ToString::to_string).collect(),
            use_relations: input.use_relations(),
            enabled_tools: unique(&tools),
            retrieval_mode: input.retrieval_mode(),
            working_folder: None,
            turn_folder: None,
            created_at: now,
            updated_at: now,
        };
        self.inner.dto(
            self.inner
                .read(|repositories| Ok(repositories.chat.create(&row)?))?,
        )
    }

    fn update(&self, id: &str, patch: &ConversationPatch) -> Result<ConversationDto, AppError> {
        self.inner
            .read(|repositories| Ok(repositories.chat.update(id, patch)?))?
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, CONVERSATION_NOT_FOUND))
            .and_then(|row| self.inner.dto(row))
    }

    pub fn bind_folder(&self, input: &BindFolderInput) -> Result<BindFolderResultDto, AppError> {
        input.validate()?;
        let id = input.id.to_string();
        self.inner.conversation(&id)?;
        self.inner.require_idle(&id)?;
        let folders = &self.inner.folders;
        let folder = folders.working_folder(std::path::Path::new(&input.path))?;
        if !folders.covered(&id, &folder)? {
            match input.grant {
                Some(scope) => {
                    folders.grant(&folder, scope, Some(&id))?;
                }
                None => {
                    return Ok(BindFolderResultDto::NeedsGrant {
                        path: folder.to_string_lossy().into_owned(),
                    });
                }
            }
        }
        let conversation = self.update(
            &id,
            &ConversationPatch {
                working_folder: Some(Some(folder.to_string_lossy().into_owned())),
                ..ConversationPatch::touched(self.inner.now())
            },
        )?;
        Ok(BindFolderResultDto::Bound {
            conversation: Box::new(conversation),
        })
    }

    pub fn unbind_folder(&self, id: &ConversationId) -> Result<ConversationDto, AppError> {
        let id = id.to_string();
        self.inner.conversation(&id)?;
        self.inner.require_idle(&id)?;
        self.update(
            &id,
            &ConversationPatch {
                working_folder: Some(None),
                ..ConversationPatch::touched(self.inner.now())
            },
        )
    }

    pub fn rename(&self, id: &ConversationId, title: &str) -> Result<ConversationDto, AppError> {
        let id = id.to_string();
        self.inner.conversation(&id)?;
        check::trimmed_name("title", title, zvs_contract::chat::CONVERSATION_TITLE_MAX)?;
        self.update(
            &id,
            &ConversationPatch {
                title: Some(zvs_contract::validate::js_trim(title).to_owned()),
                ..ConversationPatch::touched(self.inner.now())
            },
        )
    }

    pub fn update_stores(
        &self,
        id: &ConversationId,
        attached_store_ids: &[VectorStoreId],
    ) -> Result<ConversationDto, AppError> {
        let id = id.to_string();
        self.inner.conversation(&id)?;
        zvs_contract::chat::check_attached_stores("attachedStoreIds", attached_store_ids)?;
        self.inner.require_stores(attached_store_ids)?;
        self.update(
            &id,
            &ConversationPatch {
                attached_store_ids: Some(
                    unique(attached_store_ids)
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                ),
                ..ConversationPatch::touched(self.inner.now())
            },
        )
    }

    pub fn update_tools(
        &self,
        id: &ConversationId,
        enabled_tools: &[String],
    ) -> Result<ConversationDto, AppError> {
        let id = id.to_string();
        self.inner.conversation(&id)?;
        zvs_contract::chat::check_enabled_tools("enabledTools", enabled_tools)?;
        let keys = unique(
            &enabled_tools
                .iter()
                .map(|key| zvs_contract::validate::js_trim(key).to_owned())
                .collect::<Vec<_>>(),
        );
        self.inner.require_tools(&keys)?;
        self.update(
            &id,
            &ConversationPatch {
                enabled_tools: Some(keys),
                ..ConversationPatch::touched(self.inner.now())
            },
        )
    }

    pub fn update_retrieval(
        &self,
        id: &ConversationId,
        retrieval_mode: RetrievalMode,
    ) -> Result<ConversationDto, AppError> {
        let id = id.to_string();
        self.inner.conversation(&id)?;
        self.update(
            &id,
            &ConversationPatch {
                retrieval_mode: Some(retrieval_mode),
                ..ConversationPatch::touched(self.inner.now())
            },
        )
    }

    pub fn update_relations(
        &self,
        id: &ConversationId,
        use_relations: bool,
    ) -> Result<ConversationDto, AppError> {
        let id = id.to_string();
        self.inner.conversation(&id)?;
        self.update(
            &id,
            &ConversationPatch {
                use_relations: Some(use_relations),
                ..ConversationPatch::touched(self.inner.now())
            },
        )
    }

    pub fn truncate(
        &self,
        conversation_id: &ConversationId,
        message_id: &MessageId,
    ) -> Result<ConversationDetailDto, AppError> {
        let conversation = self.get(conversation_id)?;
        let id = conversation_id.to_string();
        self.inner.require_idle(&id)?;
        if !conversation
            .messages
            .iter()
            .any(|message| message.id == *message_id)
        {
            return Err(AppError::new(AppErrorCode::NotFound, MESSAGE_NOT_FOUND));
        }
        let message = message_id.to_string();
        let now = self.inner.now();
        self.inner.database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            repositories.chat.truncate_from(&id, &message)?;
            repositories
                .chat
                .update(&id, &ConversationPatch::touched(now))?;
            Ok(())
        })?;
        self.get(conversation_id)
    }

    pub fn remove(&self, id: &ConversationId) -> Result<(), AppError> {
        let id = id.to_string();
        self.inner.conversation(&id)?;
        self.inner.require_idle(&id)?;
        self.inner
            .read(|repositories| Ok(repositories.chat.remove(&id)?))
    }

    pub fn send(&self, input: &ChatSendInput) -> Result<RunHandleDto, AppError> {
        input.validate()?;
        let conversation_id = input.conversation_id.to_string();
        let conversation = self.get(&input.conversation_id)?;
        self.inner.require_idle(&conversation_id)?;
        let details = &conversation.conversation;
        let provider_id = input.provider_id.unwrap_or(details.provider_id).to_string();
        let model_id = input
            .model_id
            .clone()
            .unwrap_or_else(|| details.model_id.clone());
        let settings = input.settings.unwrap_or(details.settings);
        let model = self
            .inner
            .resolve_model(&provider_id, &model_id, &settings)?;
        let text = input.text().to_owned();
        window_chat_history(
            &details.system_prompt,
            &[],
            &ChatMessage::user(with_selections(&text, &input.selections)),
            model.context_window,
            model.max_output_tokens,
            &[],
            &[],
        )?;
        let user_id = new_id().hyphenated().to_string();
        let assistant_id = new_id().hyphenated().to_string();
        let previous_user = conversation
            .messages
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .map(|message| message.content.as_str());
        let query = retrieval_query(&text, previous_user);
        let searches_up_front: &[VectorStoreId] =
            if details.retrieval_mode == RetrievalMode::Agentic {
                &[]
            } else {
                &details.attached_store_ids
            };
        let retrieval: Vec<Value> = searches_up_front
            .iter()
            .enumerate()
            .map(|(index, store_id)| {
                json!({
                    "id": format!("store{index}"),
                    "type": "vector.search",
                    "input": { "storeId": store_id, "query": query, "k": CHAT_FETCH_K },
                })
            })
            .collect();
        let retrieval_ids: Vec<String> = (0..retrieval.len())
            .map(|index| format!("store{index}"))
            .collect();
        let bindings: serde_json::Map<String, Value> = retrieval_ids
            .iter()
            .map(|id| (id.clone(), json!({ "source": "node", "nodeId": id })))
            .collect();
        let mut nodes = retrieval;
        nodes.extend([
            json!({
                "id": "prepare",
                "type": "chat.prepare",
                "dependencies": retrieval_ids,
                "input": {
                    "conversationId": conversation_id,
                    "userId": user_id,
                    "assistantId": assistant_id,
                    "providerId": provider_id,
                    "modelId": model_id,
                    "query": query,
                    "settings": settings,
                },
                "bindings": bindings,
            }),
            json!({
                "id": "generate",
                "type": "chat.generate",
                "dependencies": ["prepare"],
                "input": {},
                "bindings": { "request": { "source": "node", "nodeId": "prepare" } },
            }),
            json!({
                "id": "persist",
                "type": "chat.persist",
                "dependencies": ["generate"],
                "input": { "assistantId": assistant_id },
                "bindings": { "result": { "source": "node", "nodeId": "generate" } },
            }),
        ]);
        let graph: RunGraph = zvs_contract::parse(json!({ "nodes": nodes }))?;
        let start = StartRunInput {
            subject_id: Some(conversation_id.clone()),
            title: Some(details.title.clone()),
            ..StartRunInput::new(RunKind::Chat, graph)
        };
        let now = self.inner.now();
        let user = MessageRow {
            id: user_id,
            conversation_id: conversation_id.clone(),
            role: MessageRole::User,
            content: text.clone(),
            reasoning: String::new(),
            citations: Vec::<ChatCitationDto>::new(),
            tool_calls: Vec::new(),
            selections: input.selections.clone(),
            tokens_in: tokens(&with_selections(&text, &input.selections)),
            tokens_out: 0,
            usage_estimated: true,
            duration_ms: 0,
            run_id: None,
            partial: false,
            created_at: now,
        };
        let inner = Arc::clone(&self.inner);
        let observer: RunObserver = Arc::new(move |event: &HostEvent| {
            if let Err(error) = inner.record(&assistant_id, event) {
                inner.log(
                    LogLevel::Warn,
                    "Could not record a chat event",
                    &[("error", json!(error.message))],
                );
            }
        });
        let (handle, subscription) = self.runs.start_with(
            start,
            |repositories, run_id| {
                repositories.chat.add_message(&MessageRow {
                    run_id: Some(run_id.to_string()),
                    ..user
                })?;
                repositories.chat.update(
                    &conversation_id,
                    &ConversationPatch {
                        provider_id: Some(provider_id.clone()),
                        model_id: Some(model_id.clone()),
                        settings: Some(settings),
                        ..ConversationPatch::touched(now)
                    },
                )?;
                Ok(())
            },
            Some(observer),
        )?;
        drop(subscription);
        Ok(handle)
    }

    pub fn context(&self, input: &ChatContextInput) -> Result<ChatContextDto, AppError> {
        input.validate()?;
        let conversation = input
            .conversation_id
            .as_ref()
            .map(|id| self.get(id))
            .transpose()?;
        let details = conversation.as_ref().map(|detail| &detail.conversation);
        let settings = input
            .settings
            .or_else(|| details.map(|details| details.settings))
            .unwrap_or_default();
        let model =
            self.inner
                .resolve_model(&input.provider_id.to_string(), &input.model_id, &settings)?;
        let system_prompt = input
            .system_prompt
            .clone()
            .or_else(|| details.map(|details| details.system_prompt.clone()))
            .unwrap_or_default();
        let mut enabled = input
            .enabled_tools
            .clone()
            .or_else(|| details.map(|details| details.enabled_tools.clone()))
            .unwrap_or_default();
        let registered = match &self.inner.tools {
            Some(tools) => tools.list(&ToolListFilter::default())?,
            None => Vec::new(),
        };
        let guidance = self.inner.mcp_guidance(&mut enabled, &registered)?;
        let offered = offered_tools(&enabled, &registered);
        let folder = details.and_then(|details| details.working_folder.as_deref());
        let workspace = if offered.is_empty() && folder.is_none() {
            String::new()
        } else {
            working_folder_section(folder, None)
        };
        let tool_block = [
            guidance,
            workspace,
            file_tools_section(&offered),
            process_tools_section(&offered),
            if offered.is_empty() {
                String::new()
            } else {
                format!("{TOOL_RESULTS_INSTRUCTION}\n{}", tool_specs_json(&offered))
            },
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
        let history: Vec<ChatMessage> = conversation
            .as_ref()
            .map(|detail| detail.messages.iter().map(model_message).collect())
            .unwrap_or_default();
        let usage = measure_context(
            &system_prompt,
            &tool_block,
            &history,
            model.context_window,
            model.max_output_tokens,
        );
        let count = |value: i64| u64::try_from(value).unwrap_or(0);
        Ok(ChatContextDto {
            context_window: count(model.context_window),
            reserved_output: count(model.max_output_tokens),
            system: count(usage.system),
            tools: count(usage.tools),
            tool_items: offered
                .iter()
                .map(|tool| ChatContextToolDto {
                    key: tool.key.clone(),
                    tokens: count(tokens(&tool_specs_json(std::slice::from_ref(tool)))),
                })
                .collect(),
            history: count(usage.history),
            history_messages: usage.history_messages as u64,
            trimmed: usage.trimmed as u64,
            total: count(usage.system + usage.tools + usage.history),
        })
    }

    pub async fn evaluate(&self, input: &ChatEvalInput) -> Result<ChatEvalRunDto, AppError> {
        input.validate()?;
        self.inner.require_stores(&[input.store_id])?;
        let store_id = input.store_id.to_string();
        let cases = self
            .inner
            .read(|repositories| Ok(repositories.vector_eval_cases.list_by_store(&store_id)?))?;
        if cases.is_empty() {
            return Err(AppError::new(AppErrorCode::ValidationFailed, NO_EVAL_CASES));
        }
        let mut modes = Vec::new();
        for mode in input.modes() {
            let mut results = Vec::new();
            for case in &cases {
                results.push(self.evaluate_case(input, mode, case).await);
            }
            let total = results.len().max(1) as f64;
            let hits = results.iter().filter(|case| case.hit).count() as f64;
            let searches: u64 = results.iter().map(|case| case.searches).sum();
            modes.push(ChatEvalModeDto {
                mode,
                hit_rate: hits / total,
                average_searches: searches as f64 / total,
                cases: results,
            });
        }
        let run = ChatEvalRunDto {
            store_id: input.store_id,
            model_id: input.model_id.clone(),
            ran_at: self.inner.now(),
            modes,
        };
        self.inner.log(
            LogLevel::Info,
            "Ran an end-to-end chat evaluation",
            &[
                ("storeId", json!(store_id)),
                ("cases", json!(cases.len())),
                (
                    "modes",
                    json!(
                        run.modes
                            .iter()
                            .map(|mode| json!({ "mode": mode.mode, "hitRate": mode.hit_rate }))
                            .collect::<Vec<_>>()
                    ),
                ),
            ],
        );
        Ok(run)
    }

    async fn evaluate_case(
        &self,
        input: &ChatEvalInput,
        mode: RetrievalMode,
        case: &crate::data::schema::VectorEvalCaseRow,
    ) -> ChatEvalCaseDto {
        let outcome = async {
            let conversation = self.create(&CreateConversationInput {
                title: Some(format!("Проверка · {}", js_slice(&case.question, 60))),
                attached_store_ids: Some(vec![input.store_id]),
                retrieval_mode: Some(mode),
                ..CreateConversationInput::new(input.provider_id, input.model_id.clone())
            })?;
            let turn = async {
                let handle = self.send(&ChatSendInput::new(conversation.id, &case.question))?;
                self.runs.wait(&handle.id).await;
                let status = self.runs.get(&handle.id)?.status;
                let answer = self
                    .get(&conversation.id)?
                    .messages
                    .into_iter()
                    .rev()
                    .find(|message| message.role == MessageRole::Assistant);
                Ok::<_, AppError>((status, answer))
            }
            .await;
            let _ = self.remove(&conversation.id);
            turn
        }
        .await;
        let failed = |error: String| ChatEvalCaseDto {
            case_id: case.id.clone(),
            question: case.question.clone(),
            cited: Vec::new(),
            hit: false,
            searches: 0,
            error: Some(error),
        };
        match outcome {
            Err(error) => failed(error.message),
            Ok((status, None)) => failed(format!("Ответ не получен: {}", status.as_str())),
            Ok((status, Some(answer))) => {
                let cited = unique(
                    &answer
                        .citations
                        .iter()
                        .map(|citation| citation.document_id.clone())
                        .collect::<Vec<_>>(),
                );
                ChatEvalCaseDto {
                    case_id: case.id.clone(),
                    question: case.question.clone(),
                    hit: cited
                        .iter()
                        .any(|document| case.expected_document_ids.contains(document)),
                    searches: answer
                        .tool_calls
                        .iter()
                        .filter(|call| call.key == KNOWLEDGE_SEARCH)
                        .count() as u64,
                    error: (status != RunStatus::Succeeded)
                        .then(|| format!("Ход завершился со статусом {}", status.as_str())),
                    cited,
                }
            }
        }
    }

    pub async fn cancel(&self, id: &RunId) -> Result<(), AppError> {
        if self.runs.get(id)?.kind != RunKind::Chat {
            return Err(AppError::new(
                AppErrorCode::ValidationFailed,
                NOT_A_CHAT_RUN,
            ));
        }
        self.runs.cancel(id).await
    }
}

#[cfg(test)]
mod tests;
