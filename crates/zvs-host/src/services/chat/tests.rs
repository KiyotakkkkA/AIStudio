mod knowledge;
mod tool_loop;
mod workspace;

use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, ChatContextInput, ChatSendInput, ConversationId,
    CreateConversationInput, CreateVectorStoreInput, GraphDirection, GraphLinkDto,
    GraphNeighborDto, GraphNeighborsInput, GraphNodeDto, GraphNodeKind, HostEvent, HostEventDraft,
    ProviderCapability, ProviderId, ProviderKind, RunHandleDto, RunListFilter, RunStatus, ToolTier,
    VectorChunkDto, VectorRerankConfig, VectorSearchHitDto, VectorStoreDto, VectorStoreId,
};

use super::*;
use crate::data::Database;
use crate::data::repositories::{ModelDraft, ProviderDraft, VectorStoreDraft, VectorStorePatch};
use crate::drivers::ai::ports::SharedTextDriver;
use crate::kernel::{
    GraphNeighbors, KernelServices, NodeRegistry, TextDrivers, VectorSearch, register_core_nodes,
};
use crate::platform::events::EventBus;
use crate::services::VectorStoreService;
use crate::services::chat_history::{
    BUDGET_EXCEEDED, SOURCES_HEADING, SOURCES_INSTRUCTION, tokens,
};
use crate::services::rerank::{Reranker, RerankerSource, SharedReranker};
use crate::services::runs::RunServiceOptions;
use crate::services::vector_stores::VectorSearchOptions;
use crate::services::vector_stores::tests::{Setup, fixture, fixture_with};
use crate::testing::{
    FakeDriver, FakeScript, MemoryLogger, MemorySink, TemporaryDatabase, temporary_database,
};

pub(super) struct OneDriver(pub SharedTextDriver);

impl TextDrivers for OneDriver {
    fn text(&self, _provider_id: &str) -> Result<SharedTextDriver, AppError> {
        Ok(Arc::clone(&self.0))
    }
}

type SearchAnswer = Arc<dyn Fn(&str) -> Vec<VectorSearchHitDto> + Send + Sync>;

pub(super) struct FakeSearch {
    calls: Mutex<Vec<(String, String, VectorSearchOptions)>>,
    answer: Mutex<SearchAnswer>,
    gate: watch::Sender<bool>,
    delegate: Option<VectorStoreService>,
    pub listed: Mutex<Vec<VectorStoreDto>>,
    pub documents: Mutex<Vec<zvs_contract::VectorDocumentDto>>,
    pub stored_chunks: Mutex<Vec<VectorChunkDto>>,
}

impl FakeSearch {
    fn new(delegate: Option<VectorStoreService>) -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            answer: Mutex::new(Arc::new(|_| Vec::new())),
            gate: watch::channel(true).0,
            delegate,
            listed: Mutex::new(Vec::new()),
            documents: Mutex::new(Vec::new()),
            stored_chunks: Mutex::new(Vec::new()),
        })
    }

    fn answer(&self, answer: impl Fn(&str) -> Vec<VectorSearchHitDto> + Send + Sync + 'static) {
        *self.answer.lock().unwrap_or_else(PoisonError::into_inner) = Arc::new(answer);
    }

    fn calls(&self) -> Vec<(String, String, VectorSearchOptions)> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn close(&self) {
        self.gate.send_replace(false);
    }

    fn open(&self) {
        self.gate.send_replace(true);
    }
}

#[async_trait]
impl VectorSearch for FakeSearch {
    async fn search(
        &self,
        store_id: &str,
        query: &str,
        options: VectorSearchOptions,
        _cancel: &CancellationToken,
    ) -> Result<Vec<VectorSearchHitDto>, AppError> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((store_id.to_owned(), query.to_owned(), options));
        let mut gate = self.gate.subscribe();
        let _ = gate.wait_for(|open| *open).await;
        let answer = Arc::clone(&self.answer.lock().unwrap_or_else(PoisonError::into_inner));
        Ok(answer(store_id))
    }

    fn has_chunks(&self) -> bool {
        self.delegate.is_some()
    }

    async fn list(&self) -> Result<Vec<VectorStoreDto>, AppError> {
        Ok(self
            .listed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone())
    }

    async fn document(
        &self,
        store_id: &str,
        document_id: &str,
    ) -> Result<Option<zvs_contract::VectorDocumentDto>, AppError> {
        Ok(self
            .documents
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|document| {
                document.store_id.to_string() == store_id && document.id.to_string() == document_id
            })
            .cloned())
    }

    async fn chunks(
        &self,
        store_id: &str,
        document_id: &str,
        indices: &[u64],
    ) -> Result<Vec<VectorChunkDto>, AppError> {
        match &self.delegate {
            Some(delegate) => delegate.chunks(store_id, document_id, indices).await,
            None => Ok(self
                .stored_chunks
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .filter(|chunk| {
                    chunk.document_id == document_id && indices.contains(&chunk.chunk_index)
                })
                .cloned()
                .collect()),
        }
    }

    fn has_related_chunks(&self) -> bool {
        self.delegate.is_some()
    }

    async fn related_chunks(
        &self,
        store_id: &str,
        query: &str,
        document_ids: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<VectorSearchHitDto>, AppError> {
        match &self.delegate {
            Some(delegate) => {
                delegate
                    .related_chunks(store_id, query, document_ids, cancel)
                    .await
            }
            None => Ok(Vec::new()),
        }
    }
}

#[derive(Default)]
struct FakeGraph {
    calls: Mutex<Vec<GraphNeighborsInput>>,
    links: Mutex<Vec<(String, String, String)>>,
}

#[async_trait]
impl GraphNeighbors for FakeGraph {
    async fn neighbors(
        &self,
        input: &GraphNeighborsInput,
    ) -> Result<Vec<GraphNeighborDto>, AppError> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(input.clone());
        let from = input.document_id.to_string();
        Ok(self
            .links
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(source, _, _)| *source == from)
            .map(|(source, target, relation)| GraphNeighborDto {
                node: GraphNodeDto {
                    id: target.clone(),
                    kind: GraphNodeKind::Document,
                },
                edge: GraphLinkDto {
                    from: source.clone(),
                    to: target.clone(),
                    relation: relation.clone(),
                    weight: None,
                    metadata: Value::Null,
                },
                direction: GraphDirection::Outgoing,
            })
            .collect())
    }
}

pub(super) struct Harness {
    pub database: Arc<Database>,
    pub runs: RunService,
    pub chat: ChatService,
    pub driver: Arc<FakeDriver>,
    pub events: Arc<MemorySink>,
    pub logger: Arc<MemoryLogger>,
    pub search: Arc<FakeSearch>,
    pub provider_id: ProviderId,
    pub model_id: String,
    _temporary: Option<TemporaryDatabase>,
}

pub(super) struct HarnessOptions {
    pub database: Option<Arc<Database>>,
    pub vectors: Option<Arc<dyn VectorSearch>>,
    pub search: Option<Arc<FakeSearch>>,
    pub graph: Option<Arc<dyn GraphNeighbors>>,
    pub providers: Option<Arc<dyn TextDrivers>>,
    pub tools: Option<ToolRegistry>,
    pub skills: Option<Arc<dyn SkillCatalogue>>,
    pub registry: Option<Arc<NodeRegistry>>,
    pub capabilities: Vec<ProviderCapability>,
    pub context_window: i64,
    pub max_output: i64,
}

impl Default for HarnessOptions {
    fn default() -> Self {
        Self {
            database: None,
            vectors: None,
            search: None,
            graph: None,
            providers: None,
            tools: None,
            skills: None,
            registry: None,
            capabilities: vec![ProviderCapability::Text, ProviderCapability::Embedding],
            context_window: 1024,
            max_output: 128,
        }
    }
}

pub(super) fn hello_driver() -> Arc<FakeDriver> {
    FakeDriver::shared(FakeScript {
        deltas: Some(vec!["Hello".to_owned(), " world".to_owned()]),
        ..FakeScript::default()
    })
}

impl Harness {
    pub fn new() -> Self {
        Self::with(HarnessOptions::default())
    }

    pub fn with(options: HarnessOptions) -> Self {
        let (temporary, database) = match options.database {
            Some(database) => (None, database),
            None => {
                let temporary = temporary_database();
                let database = temporary.shared();
                (Some(temporary), database)
            }
        };
        let driver = hello_driver();
        let events = MemorySink::shared();
        let bus = EventBus::default();
        bus.attach(events.clone());
        let logger = MemoryLogger::shared();
        let search = options.search.unwrap_or_else(|| FakeSearch::new(None));
        let registry = options.registry.unwrap_or_else(|| {
            let registry = Arc::new(NodeRegistry::new());
            register_core_nodes(&registry).expect("registers the core nodes");
            registry
        });
        let providers = options.providers.unwrap_or_else(|| {
            Arc::new(OneDriver(Arc::clone(&driver) as SharedTextDriver)) as Arc<dyn TextDrivers>
        });
        let runs = RunService::new(RunServiceOptions {
            services: KernelServices {
                providers: Some(providers),
                vector_stores: Some(
                    options
                        .vectors
                        .unwrap_or_else(|| Arc::clone(&search) as Arc<dyn VectorSearch>),
                ),
                graph: options.graph,
                ..KernelServices::default()
            },
            tools: options.tools.clone(),
            ..RunServiceOptions::new(Arc::clone(&database), bus, Arc::clone(&registry))
        })
        .expect("a run service");
        for tool in ["llm.generate", "vector.search"] {
            runs.permissions()
                .grant(tool, "global", ToolTier::Auto, "test")
                .expect("grants");
        }
        let chat = ChatService::new(ChatServiceOptions {
            tools: options.tools,
            skills: options.skills,
            logger: logger.clone(),
            ..ChatServiceOptions::new(Arc::clone(&database), runs.clone(), registry)
        })
        .expect("a chat service");
        let (provider_id, model_id) = database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let provider = repositories.providers.create(&ProviderDraft::new(
                    ProviderKind::Ollama,
                    "Fake",
                    "http://localhost:11434",
                    options.capabilities.clone(),
                    1,
                ))?;
                let models = repositories.models.replace_for_provider(
                    &provider.id,
                    &[ModelDraft {
                        context_window: Some(options.context_window),
                        max_output: Some(options.max_output),
                        ..ModelDraft::new("fake-model", "Fake", 1)
                    }],
                    1,
                )?;
                Ok((provider.id, models[0].id.clone()))
            })
            .expect("seeds a provider");
        Self {
            database,
            runs,
            chat,
            driver,
            events,
            logger,
            search,
            provider_id: ProviderId::parse(&provider_id).expect("a provider id"),
            model_id,
            _temporary: temporary,
        }
    }

    pub fn input(&self) -> CreateConversationInput {
        CreateConversationInput::new(self.provider_id, self.model_id.clone())
    }

    pub fn conversation(&self) -> ConversationId {
        self.create(self.input())
    }

    pub fn create(&self, input: CreateConversationInput) -> ConversationId {
        self.chat.create(&input).expect("creates").id
    }

    pub fn store(&self) -> VectorStoreId {
        let id = new_id().hyphenated().to_string();
        let provider = self.provider_id.to_string();
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .vector_stores
                    .create(&VectorStoreDraft::new(&id, &id, &provider, "embed", 1, 1))?)
            })
            .expect("creates a store");
        VectorStoreId::parse(&id).expect("a store id")
    }

    pub fn send(&self, conversation: ConversationId, text: &str) -> RunHandleDto {
        self.chat
            .send(&ChatSendInput::new(conversation, text))
            .expect("sends")
    }

    pub async fn turn(&self, conversation: ConversationId, text: &str) -> RunHandleDto {
        let handle = self.send(conversation, text);
        self.runs.wait(&handle.id).await;
        handle
    }

    pub fn status(&self, handle: &RunHandleDto) -> RunStatus {
        self.runs.get(&handle.id).expect("a run").status
    }

    pub fn messages(&self, conversation: ConversationId) -> Vec<MessageDto> {
        self.chat
            .get(&conversation)
            .expect("a conversation")
            .messages
    }

    pub fn last(&self, conversation: ConversationId) -> MessageDto {
        self.messages(conversation)
            .pop()
            .expect("at least one message")
    }

    pub fn events(&self) -> Vec<HostEvent> {
        self.events.events()
    }
}

pub(super) async fn eventually(mut check: impl FnMut() -> bool) {
    for _ in 0..500 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the condition never held");
}

pub(super) async fn within<T>(work: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(10), work)
        .await
        .expect("finishes in time")
}

fn hit() -> VectorSearchHitDto {
    serde_json::from_value(json!({
        "id": "chunk",
        "documentId": "document",
        "path": "docs/source.md",
        "chunkIndex": 3,
        "score": 0.9,
        "text": "A retrieved fact.",
        "section": "",
        "page": null,
        "payload": {},
    }))
    .expect("a hit")
}

fn with(hit: &VectorSearchHitDto, patch: Value) -> VectorSearchHitDto {
    let mut value = serde_json::to_value(hit).expect("serialises");
    if let (Value::Object(base), Value::Object(patch)) = (&mut value, patch) {
        base.extend(patch);
    }
    serde_json::from_value(value).expect("a hit")
}

fn token_deltas(events: &[HostEvent], handle: &RunHandleDto) -> Vec<String> {
    events
        .iter()
        .filter(|event| event.stream_id == handle.stream_id)
        .filter_map(|event| match &event.body {
            HostEventDraft::Token { delta, .. } => Some(delta.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_storeless_turn_returns_before_tokens_skips_retrieval_and_persists_usage_and_monotonic_events()
 {
    let harness = Harness::new();
    let created = harness.create(CreateConversationInput {
        system_prompt: Some("Be concise.".to_owned()),
        ..harness.input()
    });
    let handle = harness.send(created, "First question");
    assert!(token_deltas(&harness.events(), &handle).is_empty());
    within(harness.runs.wait(&handle.id)).await;
    let run = harness.runs.get(&handle.id).expect("a run");
    assert_eq!(run.kind, RunKind::Chat);
    assert_eq!(run.subject_id, Some(created.to_string()));
    assert_eq!(run.status, RunStatus::Succeeded);
    assert!(harness.search.calls().is_empty());
    assert_eq!(
        harness
            .runs
            .steps(&handle.id)
            .expect("steps")
            .iter()
            .map(|step| step.step_type.as_str())
            .collect::<Vec<_>>(),
        ["chat.prepare", "chat.generate", "chat.persist"]
    );
    let detail = harness.chat.get(&created).expect("a conversation");
    assert_eq!(detail.conversation.title, "First question");
    assert_eq!(detail.messages.len(), 2);
    let answer = &detail.messages[1];
    assert_eq!(answer.role, MessageRole::Assistant);
    assert_eq!(answer.content, "Hello world");
    assert!(!answer.partial);
    assert_eq!(answer.run_id, Some(handle.id));
    assert!(answer.citations.is_empty());
    assert!(answer.usage_estimated);
    assert!(answer.tokens_in > 0);
    assert!(answer.tokens_out > 0);
    let request = &harness.driver.requests()[0];
    assert_eq!(request.model, "fake-model");
    assert_eq!(request.system.as_deref(), Some("Be concise."));
    assert_eq!(request.messages, [ChatMessage::user("First question")]);
    let sequence: Vec<u64> = harness
        .events()
        .iter()
        .filter(|event| event.stream_id == handle.stream_id)
        .map(|event| event.seq)
        .collect();
    assert_eq!(sequence, (0..sequence.len() as u64).collect::<Vec<_>>());
    assert_eq!(
        token_deltas(&harness.events(), &handle),
        ["Hello", " world"]
    );
    assert_eq!(
        harness
            .runs
            .get(&handle.id)
            .expect("a run")
            .title
            .as_deref(),
        Some("First question")
    );
}

#[tokio::test]
async fn attached_stores_search_in_parallel_and_only_supplied_passages_become_citations() {
    let harness = Harness::new();
    let stores = [harness.store(), harness.store()];
    harness.search.answer(|_| vec![hit()]);
    harness.search.close();
    let created = harness.create(CreateConversationInput {
        attached_store_ids: Some(stores.to_vec()),
        ..harness.input()
    });
    let handle = harness.send(created, "Find a fact");
    eventually(|| harness.search.calls().len() == 2).await;
    assert!(harness.driver.requests().is_empty());
    harness.search.open();
    within(harness.runs.wait(&handle.id)).await;
    assert_eq!(harness.status(&handle), RunStatus::Succeeded);
    let citations = &harness.messages(created)[1].citations;
    assert_eq!(
        citations
            .iter()
            .map(|citation| (
                citation.store_id,
                citation.document_id.as_str(),
                citation.source_path.as_str(),
                citation.chunk_index,
                citation.section.as_str(),
                citation.page,
                citation.related.clone()
            ))
            .collect::<Vec<_>>(),
        stores
            .iter()
            .map(|store| (*store, "document", "docs/source.md", 3, "", None, None))
            .collect::<Vec<_>>()
    );
    assert!(
        citations
            .iter()
            .all(|citation| (citation.score - 0.9).abs() < 1e-9)
    );
    assert!(
        harness.driver.requests()[0]
            .messages
            .iter()
            .any(|message| message.content.contains("A retrieved fact."))
    );
}

#[tokio::test]
async fn cancelling_mid_stream_persists_the_prefix_before_the_end_event_and_releases_the_turn() {
    let harness = Harness::new();
    harness.driver.script(FakeScript {
        manual: Some(true),
        ..FakeScript::default()
    });
    let created = harness.conversation();
    let handle = harness.send(created, "Question");
    within(harness.driver.when_holding()).await;
    harness.driver.release();
    eventually(|| harness.driver.emitted() == 1).await;
    within(harness.driver.when_holding()).await;
    let concurrent = harness
        .chat
        .send(&ChatSendInput::new(created, "Concurrent"))
        .expect_err("busy");
    assert_eq!(concurrent.message, ACTIVE_TURN);
    assert_eq!(
        harness.chat.remove(&created).expect_err("busy").message,
        ACTIVE_TURN
    );
    within(harness.chat.cancel(&handle.id))
        .await
        .expect("cancels");
    let partial = &harness.messages(created)[1];
    assert_eq!(partial.content, "Hello");
    assert!(partial.partial);
    assert_eq!(partial.run_id, Some(handle.id));
    assert_eq!(harness.status(&handle), RunStatus::Cancelled);
    let last = harness.events().pop().expect("an event");
    assert!(matches!(
        last.body,
        HostEventDraft::End { ref outcome } if outcome.status == zvs_contract::RunOutcomeStatus::Cancelled
    ));
    harness.driver.clear_manual();
    let next = within(harness.turn(created, "Continue")).await;
    assert_eq!(harness.status(&next), RunStatus::Succeeded);
}

#[tokio::test]
async fn a_provider_failure_keeps_the_partial_answer_and_fails_the_run() {
    let harness = Harness::new();
    harness.driver.script(FakeScript {
        fail_after: Some(1),
        fail_with: Some(AppError::new(AppErrorCode::Unknown, "connection lost")),
        ..FakeScript::default()
    });
    let created = harness.conversation();
    let handle = within(harness.turn(created, "Question")).await;
    assert_eq!(harness.status(&handle), RunStatus::Failed);
    let answer = &harness.messages(created)[1];
    assert_eq!(answer.content, "Hello");
    assert!(answer.partial);
}

#[tokio::test]
async fn shutdown_keeps_streamed_content_and_recovery_does_not_repeat_the_generation() {
    let harness = Harness::new();
    harness.driver.script(FakeScript {
        manual: Some(true),
        ..FakeScript::default()
    });
    let created = harness.conversation();
    let handle = harness.send(created, "Question");
    within(harness.driver.when_holding()).await;
    harness.driver.release();
    eventually(|| harness.driver.emitted() == 1).await;
    within(harness.runs.dispose()).await;
    let stored = harness.messages(created);
    assert_eq!(stored[1].content, "Hello");
    assert!(stored[1].partial);
    let registry = Arc::new(NodeRegistry::new());
    register_core_nodes(&registry).expect("registers");
    let runs = RunService::new(RunServiceOptions::new(
        Arc::clone(&harness.database),
        EventBus::default(),
        Arc::clone(&registry),
    ))
    .expect("a run service");
    let _chat = ChatService::new(ChatServiceOptions::new(
        Arc::clone(&harness.database),
        runs.clone(),
        registry,
    ))
    .expect("a chat service");
    runs.recover().expect("recovers");
    assert_eq!(
        runs.get(&handle.id).expect("a run").status,
        RunStatus::Interrupted
    );
    assert_eq!(harness.driver.requests().len(), 1);
}

#[tokio::test]
async fn immediate_cancellation_never_calls_the_model_or_leaves_a_phantom_answer() {
    let harness = Harness::new();
    let created = harness.conversation();
    let handle = harness.send(created, "Question");
    within(harness.chat.cancel(&handle.id))
        .await
        .expect("cancels");
    assert_eq!(harness.status(&handle), RunStatus::Cancelled);
    assert!(harness.driver.requests().is_empty());
    assert_eq!(harness.messages(created).len(), 1);
}

#[tokio::test]
async fn windowing_keeps_the_system_prompt_and_current_turn_logs_trimming_and_bounds_history() {
    let harness = Harness::new();
    let created = harness.create(CreateConversationInput {
        system_prompt: Some("Always be helpful.".to_owned()),
        ..harness.input()
    });
    for index in 0..8 {
        within(harness.turn(created, &format!("{index} {}", "past ".repeat(200)))).await;
    }
    within(harness.turn(created, "current turn")).await;
    let request = harness.driver.requests().pop().expect("a request");
    assert_eq!(request.system.as_deref(), Some("Always be helpful."));
    assert_eq!(
        request.messages.last(),
        Some(&ChatMessage::user("current turn"))
    );
    let used = tokens(request.system.as_deref().unwrap_or_default())
        + 8
        + request
            .messages
            .iter()
            .map(|message| tokens(&message.content) + 8)
            .sum::<i64>();
    assert!(used + as_i64(request.max_output_tokens.unwrap_or(0)) <= 1024);
    assert!(request.messages.len() < 17);
    let trimmed = harness.logger.matching(HISTORY_TRIMMED);
    assert!(!trimmed.is_empty());
    assert!(trimmed[0].fields["trimmed"].is_number());
    assert!(harness.events().iter().any(|event| matches!(
        &event.body,
        HostEventDraft::Log { line } if line.get("message") == Some(&json!(HISTORY_TRIMMED))
    )));
}

#[tokio::test]
async fn oversized_mandatory_prompts_fail_before_persisting_or_launching_a_run() {
    let harness = Harness::new();
    let created = harness.create(CreateConversationInput {
        system_prompt: Some("s".repeat(4000)),
        ..harness.input()
    });
    let error = harness
        .chat
        .send(&ChatSendInput::new(created, "current"))
        .expect_err("over budget");
    assert_eq!(error.message, BUDGET_EXCEEDED);
    assert!(harness.messages(created).is_empty());
    assert!(
        harness
            .runs
            .list(&RunListFilter::default())
            .expect("lists")
            .items
            .is_empty()
    );
}

#[tokio::test]
async fn conversations_rename_list_and_cascade_messages_while_keeping_run_history() {
    let harness = Harness::new();
    let created = harness.conversation();
    let handle = within(harness.turn(created, "Question")).await;
    harness
        .chat
        .rename(&created, " Custom title ")
        .expect("renames");
    let listed = harness.chat.list().expect("lists");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].title, "Custom title");
    harness.chat.remove(&created).expect("removes");
    assert!(harness.chat.list().expect("lists").is_empty());
    let left = harness
        .database
        .with_connection(|connection| {
            Ok(Repositories::new(connection)
                .chat
                .messages(&created.to_string())?)
        })
        .expect("reads");
    assert!(left.is_empty());
    assert_eq!(harness.status(&handle), RunStatus::Succeeded);
    assert_eq!(
        harness.chat.get(&created).expect_err("gone").code,
        AppErrorCode::NotFound
    );
}

#[tokio::test]
async fn chat_generation_needs_no_tool_approval_and_completes() {
    let harness = Harness::new();
    harness
        .runs
        .permissions()
        .grant("llm.generate", "global", ToolTier::Off, "test")
        .expect("grants");
    let created = harness.conversation();
    let handle = within(harness.turn(created, "Question")).await;
    assert_eq!(harness.status(&handle), RunStatus::Succeeded);
    assert_eq!(harness.driver.requests().len(), 1);
    assert!(!harness.messages(created)[1].partial);
}

struct TextReranker;

#[async_trait]
impl Reranker for TextReranker {
    async fn rerank(
        &self,
        _query: &str,
        passages: &[String],
        _cancel: &CancellationToken,
    ) -> Result<Vec<f64>, AppError> {
        Ok(passages
            .iter()
            .map(|text| if text == "The answer." { 5.0 } else { -5.0 })
            .collect())
    }
}

fn row(document_id: &str, chunk_index: u32, text: &str) -> crate::drivers::core::VectorRow {
    crate::drivers::core::VectorRow {
        id: format!("{document_id}:{chunk_index}"),
        vector: vec![1.0, 0.0, 0.0],
        payload: json!({}),
        document_id: document_id.to_owned(),
        chunk_index,
        path: format!("docs/{document_id}.md"),
        text: text.to_owned(),
        section: String::new(),
        page: None,
    }
}

#[tokio::test]
async fn a_store_with_reranking_on_hands_chat_reranked_citations() {
    let source: RerankerSource = Arc::new(|_model: &str| Arc::new(TextReranker) as SharedReranker);
    let vectors = fixture_with(Setup {
        rerankers: Some(source),
        ..Setup::default()
    });
    let store: VectorStoreDto = vectors
        .service
        .create(&CreateVectorStoreInput {
            rerank: Some(VectorRerankConfig {
                enabled: true,
                model_ref: "curated:model:bge-reranker-v2-m3-f16".to_owned(),
                candidates: 50,
            }),
            ..vectors.input()
        })
        .await
        .expect("creates");
    let store_id = store.id.to_string();
    vectors
        .upsert(
            &store_id,
            [
                "Noise one.",
                "Noise two.",
                "Noise three.",
                "Noise four.",
                "Noise five.",
                "The answer.",
            ]
            .iter()
            .enumerate()
            .map(|(index, text)| row(&format!("doc{index}"), 0, text))
            .collect(),
        )
        .await;
    let harness = Harness::with(HarnessOptions {
        database: Some(vectors.database.shared()),
        vectors: Some(Arc::new(vectors.service.clone())),
        ..HarnessOptions::default()
    });
    let created = harness.create(CreateConversationInput {
        attached_store_ids: Some(vec![store.id]),
        ..harness.input()
    });
    within(harness.turn(created, "Where is the answer?")).await;
    let citations = &harness.messages(created)[1].citations;
    assert_eq!(citations[0].document_id, "doc5");
    assert_eq!(citations[0].source_path, "docs/doc5.md");
    assert!(citations[0].score > 0.99);
}

#[tokio::test]
async fn chat_brings_neighbours_always_and_related_documents_only_when_asked() {
    let vectors = fixture();
    let store = vectors.store().await;
    let store_id = store.id.to_string();
    let first = vectors.record_document(&store_id, "docs/a.md");
    let second = vectors.record_document(&store_id, "docs/b.md");
    let chunk = |document: &str, index: u32, text: &str| crate::drivers::core::VectorRow {
        path: if document == first {
            "docs/a.md".to_owned()
        } else {
            "docs/b.md".to_owned()
        },
        ..row(document, index, text)
    };
    vectors
        .upsert(
            &store_id,
            vec![
                chunk(&first, 1, "Matched."),
                chunk(&first, 0, "Before."),
                chunk(&first, 2, "After."),
                chunk(&second, 0, "Linked document."),
            ],
        )
        .await;
    let graph = Arc::new(FakeGraph::default());
    graph
        .links
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((first.clone(), second.clone(), "references".to_owned()));
    let search = FakeSearch::new(Some(vectors.service.clone()));
    let matched = serde_json::from_value::<VectorSearchHitDto>(json!({
        "id": format!("{first}:1"),
        "documentId": first,
        "chunkIndex": 1,
        "path": "docs/a.md",
        "text": "Matched.",
        "section": "",
        "page": null,
        "payload": {},
        "score": 0.9,
    }))
    .expect("a hit");
    search.answer(move |_| vec![matched.clone()]);
    let harness = Harness::with(HarnessOptions {
        database: Some(vectors.database.shared()),
        search: Some(Arc::clone(&search)),
        graph: Some(graph.clone()),
        ..HarnessOptions::default()
    });
    let created = harness
        .chat
        .create(&CreateConversationInput {
            attached_store_ids: Some(vec![store.id]),
            ..harness.input()
        })
        .expect("creates");
    assert!(!created.use_relations);
    within(harness.turn(created.id, "What is matched?")).await;
    assert!(
        graph
            .calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    );
    let first_answer = &harness.messages(created.id)[1];
    assert_eq!(
        first_answer
            .citations
            .iter()
            .map(|citation| citation.document_id.as_str())
            .collect::<Vec<_>>(),
        [first.as_str()]
    );
    let requests = harness.driver.requests();
    let block = &requests[0].messages[requests[0].messages.len() - 2].content;
    assert!(block.contains("Before.\nMatched.\nAfter."), "{block}");

    let updated = harness
        .chat
        .update_relations(&created.id, true)
        .expect("updates");
    assert!(updated.use_relations);
    within(harness.turn(created.id, "And what is linked?")).await;
    let calls = graph
        .calls
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].store_id, store.id);
    assert_eq!(calls[0].document_id.to_string(), first);
    assert_eq!(calls[0].direction(), GraphDirection::Both);
    let answer = &harness.messages(created.id)[3];
    assert_eq!(
        answer
            .citations
            .iter()
            .map(|citation| (citation.document_id.as_str(), citation.related.as_deref()))
            .collect::<Vec<_>>(),
        [
            (first.as_str(), None),
            (second.as_str(), Some("references"))
        ]
    );
    let requests = harness.driver.requests();
    let block = &requests[1].messages[requests[1].messages.len() - 2].content;
    assert!(block.contains("Linked document."));
}

#[tokio::test]
async fn an_off_topic_turn_fetches_candidates_but_injects_no_sources_block() {
    let harness = Harness::new();
    let store = harness.store();
    harness.search.answer(|_| {
        let base = hit();
        vec![
            with(&base, json!({ "id": "a", "score": 0.12 })),
            with(
                &base,
                json!({ "id": "b", "documentId": "other", "score": 0.05 }),
            ),
        ]
    });
    let created = harness.create(CreateConversationInput {
        attached_store_ids: Some(vec![store]),
        ..harness.input()
    });
    within(harness.turn(created, "Спасибо!")).await;
    let calls = harness.search.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, store.to_string());
    assert_eq!(calls[0].1, "Спасибо!");
    assert_eq!(calls[0].2.k, Some(20));
    assert_eq!(
        harness.driver.requests()[0].messages,
        [ChatMessage::user("Спасибо!")]
    );
    assert!(harness.messages(created)[1].citations.is_empty());
    let logged = harness.logger.matching(RETRIEVAL_ASSEMBLED);
    assert_eq!(logged.len(), 1);
    let fields = &logged[0].fields;
    assert_eq!(fields["fetched"], json!(2));
    assert_eq!(fields["belowFloor"], json!(2));
    assert_eq!(fields["injected"], json!(0));
    assert_eq!(fields["droppedByBudget"], json!(0));
}

#[tokio::test]
async fn two_stores_merge_by_score_and_the_stronger_second_store_is_cited_first() {
    let harness = Harness::new();
    let weak = harness.store();
    let strong = harness.store();
    let strong_id = strong.to_string();
    harness
        .database
        .with_connection(|connection| {
            Ok(Repositories::new(connection).vector_stores.update(
                &strong_id,
                &VectorStorePatch {
                    chat_min_score: Some(0.5),
                    ..VectorStorePatch::default()
                },
            )?)
        })
        .expect("updates");
    let strong_key = strong_id.clone();
    harness.search.answer(move |store| {
        let base = hit();
        if store == strong_key {
            vec![
                with(
                    &base,
                    json!({ "id": "s1", "documentId": "s", "path": "docs/strong.md", "score": 0.91, "text": "Strong." }),
                ),
                with(
                    &base,
                    json!({ "id": "s2", "documentId": "t", "path": "docs/t.md", "score": 0.45, "text": "Under floor." }),
                ),
            ]
        } else {
            vec![with(
                &base,
                json!({ "id": "w1", "documentId": "w", "path": "docs/weak.md", "score": 0.4, "text": "Weak." }),
            )]
        }
    });
    let created = harness.create(CreateConversationInput {
        attached_store_ids: Some(vec![weak, strong]),
        ..harness.input()
    });
    within(harness.turn(created, "Which store knows it?")).await;
    let citations = &harness.messages(created)[1].citations;
    assert_eq!(
        citations
            .iter()
            .map(|citation| (citation.store_id, citation.source_path.as_str()))
            .collect::<Vec<_>>(),
        [(strong, "docs/strong.md"), (weak, "docs/weak.md")]
    );
    let requests = harness.driver.requests();
    let block = &requests[0].messages[requests[0].messages.len() - 2].content;
    assert!(block.starts_with(&format!("{SOURCES_HEADING}\n{SOURCES_INSTRUCTION}\n\n")));
    assert!(
        block.contains(
            "<source id=\"1\" path=\"docs/strong.md\" score=\"0.91\">\nStrong.\n</source>"
        )
    );
    assert!(
        block.contains("<source id=\"2\" path=\"docs/weak.md\" score=\"0.40\">\nWeak.\n</source>")
    );
    assert!(!block.contains("Under floor."));
}

#[tokio::test]
async fn a_follow_up_turn_searches_with_the_previous_question_prepended() {
    let harness = Harness::new();
    let store = harness.store();
    let created = harness.create(CreateConversationInput {
        attached_store_ids: Some(vec![store]),
        ..harness.input()
    });
    within(harness.turn(created, "Какие условия договора поставки?")).await;
    within(harness.turn(created, "А что во втором пункте?")).await;
    assert_eq!(
        harness
            .search
            .calls()
            .into_iter()
            .map(|(_, query, _)| query)
            .collect::<Vec<_>>(),
        [
            "Какие условия договора поставки?",
            "Какие условия договора поставки?\nА что во втором пункте?"
        ]
    );
    assert_eq!(
        harness.messages(created)[2].content,
        "А что во втором пункте?"
    );
}

#[tokio::test]
async fn a_turn_runs_on_the_provider_selected_for_it_and_the_conversation_remembers_it() {
    let harness = Harness::new();
    let created = harness.conversation();
    let other = harness
        .database
        .with_connection(|connection| {
            let repositories = Repositories::new(connection);
            let provider = repositories.providers.create(&ProviderDraft::new(
                ProviderKind::Ollama,
                "Other",
                "http://localhost:11435",
                vec![ProviderCapability::Text],
                1,
            ))?;
            repositories.models.replace_for_provider(
                &provider.id,
                &[ModelDraft {
                    context_window: Some(1024),
                    max_output: Some(128),
                    ..ModelDraft::new("other-model", "Other", 1)
                }],
                1,
            )?;
            Ok(provider.id)
        })
        .expect("seeds a second provider");
    let other_id = ProviderId::parse(&other).expect("an id");
    let handle = harness
        .chat
        .send(&ChatSendInput {
            provider_id: Some(other_id),
            model_id: Some("other-model".to_owned()),
            settings: Some(ChatSettings {
                temperature: Some(0.2),
                ..ChatSettings::default()
            }),
            ..ChatSendInput::new(created, "Question")
        })
        .expect("sends");
    within(harness.runs.wait(&handle.id)).await;
    assert_eq!(harness.status(&handle), RunStatus::Succeeded);
    let prepared = harness
        .runs
        .steps(&handle.id)
        .expect("steps")
        .into_iter()
        .find(|step| step.step_type == "chat.prepare")
        .and_then(|step| step.output)
        .expect("a prepared request");
    assert_eq!(prepared["providerId"], json!(other));
    assert_eq!(prepared["model"], json!("other-model"));
    let conversation = harness
        .chat
        .get(&created)
        .expect("a conversation")
        .conversation;
    assert_eq!(conversation.provider_id, other_id);
    assert_eq!(conversation.model_id, "other-model");
    assert_eq!(conversation.settings.temperature, Some(0.2));
}

#[tokio::test]
async fn conversations_validate_their_references_and_deduplicate_lists() {
    let harness = Harness::new();
    let store = harness.store();
    let created = harness
        .chat
        .create(&CreateConversationInput {
            attached_store_ids: Some(vec![store, store]),
            ..harness.input()
        })
        .expect("creates");
    assert_eq!(created.attached_store_ids, [store]);
    assert_eq!(created.title, DEFAULT_CONVERSATION_TITLE);
    let missing = VectorStoreId::new(new_id());
    assert_eq!(
        harness
            .chat
            .update_stores(&created.id, &[missing])
            .expect_err("missing")
            .code,
        AppErrorCode::NotFound
    );
    assert!(
        harness
            .chat
            .update_stores(&created.id, &[])
            .expect("updates")
            .attached_store_ids
            .is_empty()
    );
    let unknown_model = harness
        .chat
        .create(&CreateConversationInput::new(harness.provider_id, "nope"))
        .expect_err("unknown model");
    assert_eq!(unknown_model.message, MODEL_UNAVAILABLE);
    let too_much = harness
        .chat
        .create(&CreateConversationInput {
            settings: Some(ChatSettings {
                max_output_tokens: Some(4096),
                ..ChatSettings::default()
            }),
            ..harness.input()
        })
        .expect_err("over the model limits");
    assert_eq!(too_much.message, OUTPUT_BUDGET);
}

#[tokio::test]
async fn truncating_drops_the_message_and_everything_after_it() {
    let harness = Harness::new();
    let created = harness.conversation();
    within(harness.turn(created, "One")).await;
    within(harness.turn(created, "Two")).await;
    let messages = harness.messages(created);
    assert_eq!(messages.len(), 4);
    let detail = harness
        .chat
        .truncate(&created, &messages[2].id)
        .expect("truncates");
    assert_eq!(
        detail
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        ["One", "Hello world"]
    );
    assert_eq!(
        harness
            .chat
            .truncate(&created, &messages[3].id)
            .expect_err("gone")
            .message,
        MESSAGE_NOT_FOUND
    );
}

#[tokio::test]
async fn cancel_refuses_a_run_that_is_not_a_chat_turn() {
    let harness = Harness::new();
    let handle = harness
        .runs
        .start(StartRunInput::new(
            RunKind::Scenario,
            zvs_contract::parse(json!({
                "nodes": [{ "id": "a", "type": "flow.branch", "input": { "condition": true, "then": 1, "else": 2 } }]
            }))
            .expect("a graph"),
        ))
        .expect("starts");
    within(harness.runs.wait(&handle.id)).await;
    assert_eq!(
        harness
            .chat
            .cancel(&handle.id)
            .await
            .expect_err("refused")
            .message,
        NOT_A_CHAT_RUN
    );
}

#[tokio::test]
async fn context_breaks_down_the_prompt_and_history_the_next_turn_will_carry() {
    let harness = Harness::new();
    let draft = harness
        .chat
        .context(&ChatContextInput {
            conversation_id: None,
            provider_id: harness.provider_id,
            model_id: harness.model_id.clone(),
            settings: None,
            system_prompt: Some("Always be helpful.".to_owned()),
            enabled_tools: None,
        })
        .expect("measures a draft");
    assert_eq!(draft.context_window, 1024);
    assert_eq!(draft.reserved_output, 128);
    assert_eq!(draft.system, as_u64(tokens("Always be helpful.") + 8));
    assert_eq!(
        (draft.tools, draft.history, draft.history_messages),
        (0, 0, 0)
    );
    assert_eq!(draft.total, draft.system);

    let created = harness.create(CreateConversationInput {
        system_prompt: Some("Always be helpful.".to_owned()),
        ..harness.input()
    });
    within(harness.turn(created, "first question")).await;
    let measured = harness
        .chat
        .context(&ChatContextInput {
            conversation_id: Some(created),
            provider_id: harness.provider_id,
            model_id: harness.model_id.clone(),
            settings: None,
            system_prompt: None,
            enabled_tools: None,
        })
        .expect("measures a conversation");
    assert_eq!(measured.system, draft.system);
    assert_eq!(measured.history_messages, 2);
    assert_eq!(measured.trimmed, 0);
    assert_eq!(
        measured.history,
        as_u64(tokens("first question") + tokens("Hello world") + 16)
    );
    assert_eq!(measured.total, measured.system + measured.history);
}

fn as_u64(value: i64) -> u64 {
    u64::try_from(value).expect("a non-negative count")
}
