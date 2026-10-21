use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zvs_contract::validate::check;
use zvs_contract::{
    AppError, AppErrorCode, ChatCitationDto, HostEventDraft, Validate, VectorIndexInput,
    VectorIndexReportDto, VectorSearchHitDto, VectorSearchInput, VectorStoreId, log_line,
};

use super::registry::NodeRegistry;
use super::types::{NodeDef, PermissionRequirement, StepContext};
use crate::drivers::ai::{ChatMessage, ChatRole, GenerateRequest, TextDelta};
use crate::drivers::sidecar::{JobProgress, JobRunOptions};
use crate::indexing::{IndexContext, IndexProgress};
use crate::services::vector_stores::VectorSearchOptions;

pub const PROVIDERS_UNAVAILABLE: &str = "Provider service unavailable";
pub const VECTORS_UNAVAILABLE: &str = "Vector service unavailable";
pub const JOBS_UNAVAILABLE: &str = "Сервис задач недоступен";
pub const INDEXING_UNAVAILABLE: &str = "Сервис индексации недоступен";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GenerateRole {
    System,
    User,
    Assistant,
}

impl GenerateRole {
    const fn chat_role(self) -> ChatRole {
        match self {
            Self::System => ChatRole::System,
            Self::User => ChatRole::User,
            Self::Assistant => ChatRole::Assistant,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GenerateMessage {
    pub role: GenerateRole,
    pub content: String,
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
            ..GenerateRequest::new(
                self.model,
                self.messages
                    .into_iter()
                    .map(|message| ChatMessage::new(message.role.chat_role(), message.content))
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
        Ok(GenerateOutput { text, reasoning })
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
