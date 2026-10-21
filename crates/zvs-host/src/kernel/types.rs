use std::future::Future;
use std::marker::PhantomData;
use std::sync::Arc;

use async_trait::async_trait;
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, ApprovalRequestDto, GraphNeighborDto, GraphNeighborsInput,
    HostEventDraft, RunId, ToolTier, Validate, VectorChunkDto, VectorDocumentDto, VectorIndexInput,
    VectorIndexReportDto, VectorSearchHitDto, VectorStoreDto,
};

pub use crate::data::schema::ApprovalDecision;
use crate::downloads::SharedDownloadExecutor;
use crate::drivers::ai::ProviderRegistry;
use crate::drivers::ai::ports::SharedTextDriver;
use crate::drivers::sidecar::SharedSidecarJobs;
use crate::indexing::{IndexContext, IndexingService};
use crate::services::graph::GraphService;
use crate::services::vector_stores::{VectorSearchOptions, VectorStoreService};

pub const CANCELLED_MESSAGE: &str = "Выполнение отменено";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionRequirement {
    None,
    Tool {
        tool: String,
        tier: Option<ToolTier>,
    },
}

impl PermissionRequirement {
    pub fn tool(tool: impl Into<String>) -> Self {
        Self::Tool {
            tool: tool.into(),
            tier: None,
        }
    }

    pub fn tool_at(tool: impl Into<String>, tier: ToolTier) -> Self {
        Self::Tool {
            tool: tool.into(),
            tier: Some(tier),
        }
    }

    pub fn tool_key(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Tool { tool, .. } => Some(tool),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AdmissionOrigin {
    #[default]
    Graph,
    Model,
}

pub trait TextDrivers: Send + Sync {
    fn text(&self, provider_id: &str) -> Result<SharedTextDriver, AppError>;
}

impl TextDrivers for ProviderRegistry {
    fn text(&self, provider_id: &str) -> Result<SharedTextDriver, AppError> {
        Self::text(self, provider_id)
    }
}

#[async_trait]
pub trait VectorSearch: Send + Sync {
    async fn search(
        &self,
        store_id: &str,
        query: &str,
        options: VectorSearchOptions,
        cancel: &CancellationToken,
    ) -> Result<Vec<VectorSearchHitDto>, AppError>;

    async fn list(&self) -> Result<Vec<VectorStoreDto>, AppError> {
        Err(unavailable())
    }

    fn has_chunks(&self) -> bool {
        false
    }

    async fn chunks(
        &self,
        _store_id: &str,
        _document_id: &str,
        _indices: &[u64],
    ) -> Result<Vec<VectorChunkDto>, AppError> {
        Err(unavailable())
    }

    async fn document(
        &self,
        _store_id: &str,
        _document_id: &str,
    ) -> Result<Option<VectorDocumentDto>, AppError> {
        Err(unavailable())
    }

    fn has_related_chunks(&self) -> bool {
        false
    }

    async fn related_chunks(
        &self,
        _store_id: &str,
        _query: &str,
        _document_ids: &[String],
        _cancel: &CancellationToken,
    ) -> Result<Vec<VectorSearchHitDto>, AppError> {
        Err(unavailable())
    }
}

fn unavailable() -> AppError {
    AppError::new(
        AppErrorCode::Conflict,
        "Сервис векторного поиска недоступен",
    )
}

#[async_trait]
impl VectorSearch for VectorStoreService {
    async fn search(
        &self,
        store_id: &str,
        query: &str,
        options: VectorSearchOptions,
        cancel: &CancellationToken,
    ) -> Result<Vec<VectorSearchHitDto>, AppError> {
        Self::search(self, store_id, query, options, cancel).await
    }

    async fn list(&self) -> Result<Vec<VectorStoreDto>, AppError> {
        Self::list(self).await
    }

    fn has_chunks(&self) -> bool {
        true
    }

    async fn chunks(
        &self,
        store_id: &str,
        document_id: &str,
        indices: &[u64],
    ) -> Result<Vec<VectorChunkDto>, AppError> {
        Self::chunks(self, store_id, document_id, indices).await
    }

    async fn document(
        &self,
        store_id: &str,
        document_id: &str,
    ) -> Result<Option<VectorDocumentDto>, AppError> {
        Self::document(self, store_id, document_id)
    }

    fn has_related_chunks(&self) -> bool {
        true
    }

    async fn related_chunks(
        &self,
        store_id: &str,
        query: &str,
        document_ids: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<VectorSearchHitDto>, AppError> {
        Self::related_chunks(self, store_id, query, document_ids, cancel).await
    }
}

#[async_trait]
pub trait GraphNeighbors: Send + Sync {
    async fn neighbors(
        &self,
        input: &GraphNeighborsInput,
    ) -> Result<Vec<GraphNeighborDto>, AppError>;
}

#[async_trait]
impl GraphNeighbors for GraphService {
    async fn neighbors(
        &self,
        input: &GraphNeighborsInput,
    ) -> Result<Vec<GraphNeighborDto>, AppError> {
        Self::neighbors(self, input).await
    }
}

#[async_trait]
pub trait VectorIndexer: Send + Sync {
    async fn index(
        &self,
        input: &VectorIndexInput,
        context: IndexContext,
    ) -> Result<VectorIndexReportDto, AppError>;
}

#[async_trait]
impl VectorIndexer for IndexingService {
    async fn index(
        &self,
        input: &VectorIndexInput,
        context: IndexContext,
    ) -> Result<VectorIndexReportDto, AppError> {
        Self::index(self, input, context).await
    }
}

#[derive(Clone, Default)]
pub struct KernelServices {
    pub providers: Option<Arc<dyn TextDrivers>>,
    pub vector_stores: Option<Arc<dyn VectorSearch>>,
    pub jobs: Option<SharedSidecarJobs>,
    pub downloads: Option<SharedDownloadExecutor>,
    pub indexing: Option<Arc<dyn VectorIndexer>>,
    pub graph: Option<Arc<dyn GraphNeighbors>>,
}

pub type ApprovalObserver =
    Arc<dyn Fn(&ApprovalRequestDto, Option<ApprovalDecision>) + Send + Sync>;

#[derive(Clone)]
pub struct ToolStepRequest {
    pub key: String,
    pub input: Value,
    pub label: String,
    pub minimum_tier: Option<ToolTier>,
    pub on_approval: Option<ApprovalObserver>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolStepResult {
    pub step_id: String,
    pub output: Value,
    pub meta: Option<Value>,
}

pub type ToolStepFuture = BoxFuture<'static, Result<ToolStepResult, AppError>>;

pub trait StepLink: Send + Sync {
    fn emit(&self, event: HostEventDraft) -> Result<(), AppError>;

    fn request_approval(
        &self,
        request: ApprovalRequestDto,
    ) -> BoxFuture<'static, Result<ApprovalDecision, AppError>>;

    fn invoke_tool(&self, _request: ToolStepRequest) -> Option<ToolStepFuture> {
        None
    }
}

#[derive(Clone)]
pub struct StepContext {
    pub run_id: RunId,
    pub cancel: CancellationToken,
    pub services: KernelServices,
    link: Arc<dyn StepLink>,
}

impl StepContext {
    pub fn new(
        run_id: RunId,
        cancel: CancellationToken,
        services: KernelServices,
        link: Arc<dyn StepLink>,
    ) -> Self {
        Self {
            run_id,
            cancel,
            services,
            link,
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub fn check_cancelled(&self) -> Result<(), AppError> {
        if self.cancel.is_cancelled() {
            Err(cancelled())
        } else {
            Ok(())
        }
    }

    pub fn emit(&self, event: HostEventDraft) -> Result<(), AppError> {
        self.link.emit(event)
    }

    pub async fn request_approval(
        &self,
        request: ApprovalRequestDto,
    ) -> Result<ApprovalDecision, AppError> {
        self.link.request_approval(request).await
    }

    pub fn invoke_tool(&self, request: ToolStepRequest) -> Option<ToolStepFuture> {
        self.link.invoke_tool(request)
    }

    pub fn link(&self) -> Arc<dyn StepLink> {
        Arc::clone(&self.link)
    }
}

pub fn cancelled() -> AppError {
    AppError::new(AppErrorCode::RunCancelled, CANCELLED_MESSAGE)
}

#[async_trait]
pub trait NodeDef: Send + Sync + 'static {
    type Input: DeserializeOwned + JsonSchema + Validate + Send + 'static;
    type Output: Serialize + JsonSchema + Validate + Send + 'static;

    fn node_type(&self) -> &str;

    fn permission(&self) -> PermissionRequirement;

    fn side_effect(&self) -> bool;

    async fn run(
        &self,
        context: &StepContext,
        input: Self::Input,
    ) -> Result<Self::Output, AppError>;

    fn tool_meta(&self, _input: &Value, _output: &Self::Output) -> Option<Value> {
        None
    }
}

pub struct FnNode<I, O, F> {
    node_type: String,
    permission: PermissionRequirement,
    side_effect: bool,
    run: F,
    marker: PhantomData<fn(I) -> O>,
}

impl<I, O, F> FnNode<I, O, F> {
    pub fn new(
        node_type: impl Into<String>,
        permission: PermissionRequirement,
        side_effect: bool,
        run: F,
    ) -> Self {
        Self {
            node_type: node_type.into(),
            permission,
            side_effect,
            run,
            marker: PhantomData,
        }
    }
}

#[async_trait]
impl<I, O, F, Fut> NodeDef for FnNode<I, O, F>
where
    I: DeserializeOwned + JsonSchema + Validate + Send + 'static,
    O: Serialize + JsonSchema + Validate + Send + 'static,
    F: Fn(StepContext, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O, AppError>> + Send + 'static,
{
    type Input = I;
    type Output = O;

    fn node_type(&self) -> &str {
        &self.node_type
    }

    fn permission(&self) -> PermissionRequirement {
        self.permission.clone()
    }

    fn side_effect(&self) -> bool {
        self.side_effect
    }

    async fn run(&self, context: &StepContext, input: I) -> Result<O, AppError> {
        (self.run)(context.clone(), input).await
    }
}
