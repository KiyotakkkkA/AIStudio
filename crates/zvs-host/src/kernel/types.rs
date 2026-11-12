use std::future::Future;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use futures::future::BoxFuture;
use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, ApprovalRequestDto, GraphNeighborDto, GraphNeighborsInput,
    HostEventDraft, QuestionAnswerDto, QuestionRequestDto, RunId, ToolAuthorisationDto, ToolTier,
    Validate, VectorChunkDto, VectorDocumentDto, VectorIndexInput, VectorIndexReportDto,
    VectorSearchHitDto, VectorStoreDto, log_line,
};

use crate::access::{AccessRequest, Admitted, Clearance, SharedGate};
pub use crate::data::schema::ApprovalDecision;
use crate::downloads::SharedDownloadExecutor;
use crate::drivers::ai::ProviderRegistry;
use crate::drivers::ai::ports::SharedTextDriver;
use crate::drivers::sidecar::SharedSidecarJobs;
use crate::indexing::{IndexContext, IndexingService};
use crate::policy::ReviewVerdict;
use crate::services::graph::GraphService;
use crate::services::vector_stores::{VectorSearchOptions, VectorStoreService};

pub const CANCELLED_MESSAGE: &str = "Выполнение отменено";
pub const NO_QUESTION_SERVICE: &str = "Вопросы пользователю недоступны в этом запуске";
pub const TOOL_AUTHORISED: &str = "Tool call authorised";
pub const NO_PERMISSION_SERVICE: &str = "Сервис разрешений недоступен";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionOutcome {
    Answered(Vec<QuestionAnswerDto>),
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionRequirement {
    None,
    Kernel,
    Tool {
        tool: String,
        tier: Option<ToolTier>,
    },
    AppFolder {
        tool: String,
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

    pub fn app_folder(tool: impl Into<String>) -> Self {
        Self::AppFolder { tool: tool.into() }
    }

    pub fn tool_key(&self) -> Option<&str> {
        match self {
            Self::None | Self::Kernel => None,
            Self::Tool { tool, .. } | Self::AppFolder { tool } => Some(tool),
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
    pub policy: Option<Arc<dyn ExecReview>>,
}

#[async_trait]
pub trait ExecReview: Send + Sync {
    async fn review(
        &self,
        context: &StepContext,
        effect: &crate::access::Effect,
    ) -> Result<ReviewVerdict, AppError>;

    async fn refused(
        &self,
        context: &StepContext,
        effect: &crate::access::Effect,
    ) -> Result<Option<String>, AppError>;
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
    pub authorisation: Option<ToolAuthorisationDto>,
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

    fn ask_user(
        &self,
        _request: QuestionRequestDto,
    ) -> BoxFuture<'static, Result<QuestionOutcome, AppError>> {
        Box::pin(async { Err(AppError::new(AppErrorCode::Conflict, NO_QUESTION_SERVICE)) })
    }
}

#[derive(Clone)]
pub struct StepContext {
    pub run_id: RunId,
    pub cancel: CancellationToken,
    pub services: KernelServices,
    link: Arc<dyn StepLink>,
    gate: Option<SharedGate>,
    authorisation: Option<Arc<Mutex<Option<ToolAuthorisationDto>>>>,
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
            gate: None,
            authorisation: None,
        }
    }

    pub fn with_link(&self, link: Arc<dyn StepLink>) -> Self {
        Self {
            link,
            ..self.clone()
        }
    }

    #[must_use]
    pub fn with_gate(self, gate: SharedGate) -> Self {
        Self {
            gate: Some(gate),
            ..self
        }
    }

    pub fn gate(&self) -> Option<SharedGate> {
        self.gate.clone()
    }

    pub async fn screen(&self, request: &AccessRequest) -> Result<(), AppError> {
        match &self.gate {
            Some(gate) => gate.screen(request, self).await,
            None if Clearance::open(request).is_some() => Ok(()),
            None => Err(no_gate()),
        }
    }

    pub async fn admit(&self, request: AccessRequest) -> Result<Clearance, AppError> {
        match &self.gate {
            Some(gate) => gate.admit(request, self).await,
            None => Clearance::open(&request).ok_or_else(no_gate),
        }
    }

    pub fn recording(self) -> Self {
        Self {
            authorisation: Some(Arc::new(Mutex::new(None))),
            ..self
        }
    }

    pub fn authorise(&self, authorisation: ToolAuthorisationDto) -> Result<(), AppError> {
        let Some(slot) = &self.authorisation else {
            return Ok(());
        };
        let mut fields = vec![
            ("message", json!(TOOL_AUTHORISED)),
            ("authorisation", json!(authorisation.label())),
        ];
        if let Some(reason) = &authorisation.reason {
            fields.push(("reason", json!(reason)));
        }
        *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(authorisation);
        self.link.emit(HostEventDraft::Log {
            line: log_line(&fields),
        })
    }

    pub fn authorisation(&self) -> Option<ToolAuthorisationDto> {
        self.authorisation
            .as_ref()
            .and_then(|slot| slot.lock().unwrap_or_else(PoisonError::into_inner).clone())
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

    pub async fn ask_user(&self, request: QuestionRequestDto) -> Result<QuestionOutcome, AppError> {
        self.link.ask_user(request).await
    }

    pub fn link(&self) -> Arc<dyn StepLink> {
        Arc::clone(&self.link)
    }
}

fn no_gate() -> AppError {
    AppError::new(AppErrorCode::PermissionDenied, NO_PERMISSION_SERVICE)
}

pub fn cancelled() -> AppError {
    AppError::new(AppErrorCode::RunCancelled, CANCELLED_MESSAGE)
}

#[async_trait]
pub trait NodeDef: Send + Sync + 'static {
    type Input: DeserializeOwned + JsonSchema + Validate + Send + 'static;
    type Output: Serialize + JsonSchema + Validate + Send + 'static;
    type Prepared: Send + Sync + 'static;

    fn node_type(&self) -> &str;

    fn permission(&self) -> PermissionRequirement;

    fn permission_for(&self, _input: &Value) -> PermissionRequirement {
        self.permission()
    }

    fn side_effect(&self) -> bool;

    async fn prepare(
        &self,
        context: &StepContext,
        input: Self::Input,
    ) -> Result<Self::Prepared, AppError>;

    fn access(&self, prepared: &Self::Prepared, request: AccessRequest) -> AccessRequest;

    async fn execute(
        &self,
        context: &StepContext,
        admitted: Admitted<Self::Prepared>,
    ) -> Result<Self::Output, AppError>;

    async fn refused(&self, _prepared: Self::Prepared, _error: &AppError) {}

    fn tool_meta(&self, _input: &Value, _output: &Self::Output) -> Option<Value> {
        None
    }
}

#[async_trait]
pub trait PlainNode: Send + Sync + 'static {
    type Input: DeserializeOwned + JsonSchema + Validate + Send + Sync + 'static;
    type Output: Serialize + JsonSchema + Validate + Send + 'static;

    fn node_type(&self) -> &str;

    fn permission(&self) -> PermissionRequirement;

    fn permission_for(&self, _input: &Value) -> PermissionRequirement {
        self.permission()
    }

    fn side_effect(&self) -> bool;

    fn access(&self, _input: &Self::Input, request: AccessRequest) -> AccessRequest {
        request
    }

    async fn execute(
        &self,
        context: &StepContext,
        admitted: Admitted<Self::Input>,
    ) -> Result<Self::Output, AppError>;

    fn tool_meta(&self, _input: &Value, _output: &Self::Output) -> Option<Value> {
        None
    }
}

#[async_trait]
impl<T: PlainNode> NodeDef for T {
    type Input = T::Input;
    type Output = T::Output;
    type Prepared = T::Input;

    fn node_type(&self) -> &str {
        PlainNode::node_type(self)
    }

    fn permission(&self) -> PermissionRequirement {
        PlainNode::permission(self)
    }

    fn permission_for(&self, input: &Value) -> PermissionRequirement {
        PlainNode::permission_for(self, input)
    }

    fn side_effect(&self) -> bool {
        PlainNode::side_effect(self)
    }

    async fn prepare(&self, _context: &StepContext, input: T::Input) -> Result<T::Input, AppError> {
        Ok(input)
    }

    fn access(&self, prepared: &T::Input, request: AccessRequest) -> AccessRequest {
        PlainNode::access(self, prepared, request)
    }

    async fn execute(
        &self,
        context: &StepContext,
        admitted: Admitted<T::Input>,
    ) -> Result<T::Output, AppError> {
        PlainNode::execute(self, context, admitted).await
    }

    fn tool_meta(&self, input: &Value, output: &T::Output) -> Option<Value> {
        PlainNode::tool_meta(self, input, output)
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
impl<I, O, F, Fut> PlainNode for FnNode<I, O, F>
where
    I: DeserializeOwned + JsonSchema + Validate + Send + Sync + 'static,
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

    async fn execute(&self, context: &StepContext, admitted: Admitted<I>) -> Result<O, AppError> {
        (self.run)(context.clone(), admitted.into_inner()).await
    }
}
