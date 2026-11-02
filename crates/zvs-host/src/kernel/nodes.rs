use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zvs_contract::validate::check;
use zvs_contract::{
    AppError, AppErrorCode, ChatCitationDto, HostEventDraft, Validate, VectorIndexInput,
    VectorIndexReportDto, VectorSearchHitDto, VectorSearchInput, VectorStoreId, VendorTurnOptions,
    WebSourceDto, log_line,
};

use super::registry::NodeRegistry;
use super::types::{NodeDef, PermissionRequirement, StepContext};
use crate::drivers::ai::{ChatMessage, ChatRole, GenerateRequest, TextDelta, ToolCall};
use crate::drivers::sidecar::{JobProgress, JobRunOptions};
use crate::indexing::{IndexContext, IndexProgress};
use crate::services::vector_stores::VectorSearchOptions;

pub const PROVIDERS_UNAVAILABLE: &str = "Provider service unavailable";
pub const VECTORS_UNAVAILABLE: &str = "Сервис векторного поиска недоступен";
pub const JOBS_UNAVAILABLE: &str = "Сервис задач недоступен";
pub const INDEXING_UNAVAILABLE: &str = "Сервис индексации недоступен";
pub const TOOL_CALLS_NOT_ASSISTANT: &str =
    "Вызовы инструментов допустимы только в сообщении ассистента";
pub const TOOL_RESULT_UNPAIRED: &str = "Результат инструмента не связан с предшествующим вызовом";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GenerateRole {
    System,
    User,
    Assistant,
    Tool,
}

impl GenerateRole {
    const fn chat_role(self) -> ChatRole {
        match self {
            Self::System => ChatRole::System,
            Self::User => ChatRole::User,
            Self::Assistant => ChatRole::Assistant,
            Self::Tool => ChatRole::Tool,
        }
    }

    pub const fn from_chat(role: ChatRole) -> Self {
        match role {
            ChatRole::System => Self::System,
            ChatRole::User => Self::User,
            ChatRole::Assistant => Self::Assistant,
            ChatRole::Tool => Self::Tool,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GenerateToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GenerateMessage {
    pub role: GenerateRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<GenerateToolCall>,
}

impl GenerateMessage {
    pub fn new(role: GenerateRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn from_chat(message: &ChatMessage) -> Self {
        Self {
            role: GenerateRole::from_chat(message.role),
            content: message.content.clone(),
            tool_call_id: message.tool_call_id.clone(),
            tool_calls: message
                .tool_calls
                .iter()
                .map(|call| GenerateToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                })
                .collect(),
        }
    }

    fn into_chat(self) -> ChatMessage {
        ChatMessage {
            role: self.role.chat_role(),
            content: self.content,
            tool_call_id: self.tool_call_id,
            tool_calls: self
                .tool_calls
                .into_iter()
                .map(|call| ToolCall::new(call.id, call.name, call.arguments))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GenerateNodeInput {
    pub provider_id: String,
    pub model: String,
    pub messages: Vec<GenerateMessage>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "zvs_contract::wire::present"
    )]
    pub system: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "zvs_contract::wire::present"
    )]
    pub temperature: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "zvs_contract::wire::present"
    )]
    pub top_k: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "zvs_contract::wire::present"
    )]
    pub top_p: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "zvs_contract::wire::present"
    )]
    pub max_output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "VendorTurnOptions::is_default")]
    pub vendor: VendorTurnOptions,
}

impl GenerateNodeInput {
    pub fn request(self) -> GenerateRequest {
        GenerateRequest {
            system: self.system,
            temperature: self.temperature,
            top_k: self
                .top_k
                .map(|value| u32::try_from(value).unwrap_or(u32::MAX)),
            top_p: self.top_p,
            max_output_tokens: self.max_output_tokens,
            vendor: self.vendor,
            ..GenerateRequest::new(
                self.model,
                self.messages
                    .into_iter()
                    .map(GenerateMessage::into_chat)
                    .collect(),
            )
        }
    }
}

fn positive(field: &str, value: u64) -> Result<(), AppError> {
    check::safe_integer(field, value)?;
    check::int_range(field, value, 1, u64::MAX)
}

impl Validate for GenerateNodeInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("providerId", &self.provider_id)?;
        check::non_empty("model", &self.model)?;
        if let Some(temperature) = self.temperature {
            check::number_range("temperature", temperature, 0.0, 2.0)?;
        }
        if let Some(top_k) = self.top_k {
            positive("topK", top_k)?;
        }
        if let Some(top_p) = self.top_p {
            check::number_range("topP", top_p, 0.0, 1.0)?;
        }
        if let Some(tokens) = self.max_output_tokens {
            positive("maxOutputTokens", tokens)?;
        }
        let mut called: HashSet<&str> = HashSet::new();
        for message in &self.messages {
            if !message.tool_calls.is_empty() && message.role != GenerateRole::Assistant {
                return Err(AppError::new(
                    AppErrorCode::ValidationFailed,
                    TOOL_CALLS_NOT_ASSISTANT,
                ));
            }
            for call in &message.tool_calls {
                check::non_empty("toolCalls.id", &call.id)?;
                check::non_empty("toolCalls.name", &call.name)?;
                called.insert(&call.id);
            }
            let answers = message.tool_call_id.as_deref();
            match (message.role, answers) {
                (GenerateRole::Tool, Some(id)) if called.contains(id) => {}
                (GenerateRole::Tool, _) => {
                    return Err(AppError::new(
                        AppErrorCode::ValidationFailed,
                        TOOL_RESULT_UNPAIRED,
                    ));
                }
                (_, Some(_)) => {
                    return Err(AppError::new(
                        AppErrorCode::ValidationFailed,
                        TOOL_RESULT_UNPAIRED,
                    ));
                }
                (_, None) => {}
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum GenerateInput {
    Direct(GenerateNodeInput),
    Prepared { request: GenerateNodeInput },
}

impl GenerateInput {
    fn into_request(self) -> GenerateNodeInput {
        match self {
            Self::Direct(request) | Self::Prepared { request } => request,
        }
    }
}

impl Validate for GenerateInput {
    fn validate(&self) -> Result<(), AppError> {
        match self {
            Self::Direct(request) => request.validate(),
            Self::Prepared { request } => {
                zvs_contract::validate::nested("request", request.validate())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GenerateOutput {
    pub text: String,
    pub reasoning: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub web_sources: Vec<WebSourceDto>,
}

impl Validate for GenerateOutput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

pub struct GenerateNode;

#[async_trait]
impl NodeDef for GenerateNode {
    type Input = GenerateInput;
    type Output = GenerateOutput;

    fn node_type(&self) -> &str {
        "llm.generate"
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
        input: GenerateInput,
    ) -> Result<GenerateOutput, AppError> {
        let input = input.into_request();
        let Some(providers) = &context.services.providers else {
            return Err(AppError::new(AppErrorCode::Conflict, PROVIDERS_UNAVAILABLE));
        };
        let driver = providers.text(&input.provider_id)?;
        let request = input.request();
        let mut deltas = driver.stream(&request, &context.cancel);
        let mut text = String::new();
        let mut reasoning = String::new();
        while let Some(delta) = deltas.next().await {
            let delta = delta?;
            context.check_cancelled()?;
            let TextDelta::Content { text: chunk, kind } = delta else {
                continue;
            };
            match kind {
                zvs_contract::DeltaKind::Reasoning => reasoning.push_str(&chunk),
                zvs_contract::DeltaKind::Text => text.push_str(&chunk),
            }
            context.emit(HostEventDraft::Token {
                delta: chunk,
                kind: Some(kind),
            })?;
        }
        Ok(GenerateOutput {
            text,
            reasoning,
            web_sources: Vec::new(),
        })
    }
}

pub struct VectorSearchNode;

#[async_trait]
impl NodeDef for VectorSearchNode {
    type Input = VectorSearchInput;
    type Output = Vec<VectorSearchHitDto>;

    fn node_type(&self) -> &str {
        "vector.search"
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool("vector.search")
    }

    fn side_effect(&self) -> bool {
        false
    }

    async fn run(
        &self,
        context: &StepContext,
        input: VectorSearchInput,
    ) -> Result<Vec<VectorSearchHitDto>, AppError> {
        context.check_cancelled()?;
        let Some(vectors) = &context.services.vector_stores else {
            return Err(AppError::new(AppErrorCode::Conflict, VECTORS_UNAVAILABLE));
        };
        let options = VectorSearchOptions {
            k: input.k,
            min_score: input.min_score,
            mode: input.mode,
            ..VectorSearchOptions::default()
        };
        let hits = vectors
            .search(
                &input.store_id.to_string(),
                &input.query,
                options,
                &context.cancel,
            )
            .await?;
        context.check_cancelled()?;
        Ok(hits)
    }

    fn tool_meta(&self, input: &Value, output: &Vec<VectorSearchHitDto>) -> Option<Value> {
        let store_id =
            serde_json::from_value::<VectorStoreId>(input.get("storeId")?.clone()).ok()?;
        let citations = output
            .iter()
            .map(|hit| ChatCitationDto::from_hit(store_id, hit, None))
            .collect::<Vec<_>>();
        Some(json!({ "citations": citations }))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct VectorListInput {}

impl<'de> Deserialize<'de> for VectorListInput {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<serde_json::Map<String, Value>>::deserialize(deserializer).map(|_| Self {})
    }
}

impl Validate for VectorListInput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VectorStoreListEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub documents: u64,
    pub status: String,
    pub search_mode: String,
}

impl Validate for VectorStoreListEntry {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("documents", self.documents)
    }
}

pub struct VectorListNode;

#[async_trait]
impl NodeDef for VectorListNode {
    type Input = VectorListInput;
    type Output = Vec<VectorStoreListEntry>;

    fn node_type(&self) -> &str {
        "vector.list"
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool("vector.list")
    }

    fn side_effect(&self) -> bool {
        false
    }

    async fn run(
        &self,
        context: &StepContext,
        _input: VectorListInput,
    ) -> Result<Vec<VectorStoreListEntry>, AppError> {
        context.check_cancelled()?;
        let Some(vectors) = &context.services.vector_stores else {
            return Err(AppError::new(AppErrorCode::Conflict, VECTORS_UNAVAILABLE));
        };
        Ok(vectors
            .list()
            .await?
            .into_iter()
            .map(|store| VectorStoreListEntry {
                id: store.id.to_string(),
                name: store.name,
                description: store.description,
                documents: store.documents,
                status: store.status.as_str().to_owned(),
                search_mode: store.search_mode.as_str().to_owned(),
            })
            .collect())
    }
}

fn step_event(domain: &str, value: &impl Serialize) -> HostEventDraft {
    let mut step = serde_json::Map::new();
    step.insert("domain".to_owned(), json!(domain));
    if let Ok(Value::Object(fields)) = serde_json::to_value(value) {
        step.extend(fields);
    }
    HostEventDraft::Step { step }
}

pub struct VectorIndexNode;

#[async_trait]
impl NodeDef for VectorIndexNode {
    type Input = VectorIndexInput;
    type Output = VectorIndexReportDto;

    fn node_type(&self) -> &str {
        "vector.index"
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool("vector.index")
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(
        &self,
        context: &StepContext,
        input: VectorIndexInput,
    ) -> Result<VectorIndexReportDto, AppError> {
        context.check_cancelled()?;
        let Some(indexing) = &context.services.indexing else {
            return Err(AppError::new(AppErrorCode::Conflict, INDEXING_UNAVAILABLE));
        };
        let store_id = input.store_id.to_string();
        let progress = context.clone();
        let samples = context.clone();
        let documents = context.clone();
        indexing
            .index(
                &input,
                IndexContext {
                    cancel: context.cancel.clone(),
                    on_progress: Some(Arc::new(move |update: IndexProgress| {
                        let _ = progress.emit(HostEventDraft::Progress {
                            done: update.done,
                            total: update.total,
                        });
                        if let Some(message) = update.message {
                            let _ = progress.emit(HostEventDraft::Log {
                                line: log_line(&[
                                    ("storeId", json!(store_id)),
                                    ("message", json!(message)),
                                ]),
                            });
                        }
                    })),
                    on_sample: Some(Arc::new(move |sample| {
                        let _ = samples.emit(step_event("resources", &sample));
                    })),
                    on_document: Some(Arc::new(move |document| {
                        let _ = documents.emit(step_event("document", &document));
                    })),
                },
            )
            .await
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BranchInput {
    pub condition: bool,
    pub then: Value,
    #[serde(rename = "else")]
    pub otherwise: Value,
}

impl Validate for BranchInput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

pub struct BranchNode;

#[async_trait]
impl NodeDef for BranchNode {
    type Input = BranchInput;
    type Output = Value;

    fn node_type(&self) -> &str {
        "flow.branch"
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::None
    }

    fn side_effect(&self) -> bool {
        false
    }

    async fn run(&self, context: &StepContext, input: BranchInput) -> Result<Value, AppError> {
        context.check_cancelled()?;
        Ok(if input.condition {
            input.then
        } else {
            input.otherwise
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MapInput {
    pub items: Vec<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "zvs_contract::wire::present"
    )]
    pub path: Option<Vec<String>>,
}

impl Validate for MapInput {
    fn validate(&self) -> Result<(), AppError> {
        Ok(())
    }
}

fn own_property<'a>(value: &'a Value, key: &str) -> Option<std::borrow::Cow<'a, Value>> {
    match value {
        Value::Object(map) => map.get(key).map(std::borrow::Cow::Borrowed),
        Value::Array(items) if key == "length" => Some(std::borrow::Cow::Owned(json!(items.len()))),
        Value::Array(items) => key
            .parse::<usize>()
            .ok()
            .filter(|index| index.to_string() == key)
            .and_then(|index| items.get(index))
            .map(std::borrow::Cow::Borrowed),
        _ => None,
    }
}

pub struct MapNode;

#[async_trait]
impl NodeDef for MapNode {
    type Input = MapInput;
    type Output = Vec<Value>;

    fn node_type(&self) -> &str {
        "flow.map"
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::None
    }

    fn side_effect(&self) -> bool {
        false
    }

    async fn run(&self, context: &StepContext, input: MapInput) -> Result<Vec<Value>, AppError> {
        let path = input.path.unwrap_or_default();
        input
            .items
            .into_iter()
            .map(|item| {
                context.check_cancelled()?;
                let mut value = item;
                for key in &path {
                    value = own_property(&value, key)
                        .ok_or_else(|| {
                            AppError::new(
                                AppErrorCode::ValidationFailed,
                                format!("Missing map field: {key}"),
                            )
                        })?
                        .into_owned();
                }
                Ok(value)
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobRunInput {
    pub job: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "zvs_contract::wire::present"
    )]
    pub params: Option<Value>,
}

impl Validate for JobRunInput {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("job", &self.job)?;
        check::max_len("job", &self.job, 128)
    }
}

pub struct JobRunNode;

#[async_trait]
impl NodeDef for JobRunNode {
    type Input = JobRunInput;
    type Output = Value;

    fn node_type(&self) -> &str {
        "job.run"
    }

    fn permission(&self) -> PermissionRequirement {
        PermissionRequirement::tool("job.run")
    }

    fn side_effect(&self) -> bool {
        true
    }

    async fn run(&self, context: &StepContext, input: JobRunInput) -> Result<Value, AppError> {
        context.check_cancelled()?;
        let Some(jobs) = &context.services.jobs else {
            return Err(AppError::new(AppErrorCode::Conflict, JOBS_UNAVAILABLE));
        };
        let progress = context.clone();
        let job = input.job.clone();
        jobs.run(
            &input.job,
            input.params.unwrap_or(Value::Null),
            JobRunOptions {
                cancel: Some(context.cancel.clone()),
                on_progress: Some(Arc::new(move |update: JobProgress| {
                    let _ = progress.emit(HostEventDraft::Progress {
                        done: update.done,
                        total: update.total,
                    });
                    if let Some(message) = update.message {
                        let _ = progress.emit(HostEventDraft::Log {
                            line: log_line(&[("job", json!(job)), ("message", json!(message))]),
                        });
                    }
                })),
            },
        )
        .await
    }
}

pub fn register_core_nodes(registry: &NodeRegistry) -> Result<(), AppError> {
    registry.register(GenerateNode)?;
    registry.register(VectorSearchNode)?;
    registry.register(VectorListNode)?;
    registry.register(VectorIndexNode)?;
    super::knowledge_nodes::register_knowledge_nodes(registry)?;
    registry.register(BranchNode)?;
    registry.register(MapNode)
}

pub fn register_job_nodes(registry: &NodeRegistry) -> Result<(), AppError> {
    registry.register(JobRunNode)
}
