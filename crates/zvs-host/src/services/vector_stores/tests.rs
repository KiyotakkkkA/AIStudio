use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::oneshot;
use zvs_contract::{UpdateVectorStoreInput, VectorOcrLanguage};

use super::*;
use crate::data::repositories::VectorDocumentDraft;
use crate::drivers::core::{InProcessCore, VectorCore, VectorRow};
use crate::services::rerank::{Reranker, SharedReranker, normalise_rerank_score};
use crate::testing::{
    FakeClock, FakeDriver, FakeDriverSource, FakeScript, MemoryLogger, TemporaryDatabase,
    seed_embedding_provider, temporary_database,
};

pub(crate) type Hook = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
pub(crate) struct ProbeVectorCore {
    pub inner: InProcessCore,
    pub fail_create: AtomicBool,
    pub fail_remove: AtomicUsize,
    pub remove_calls: AtomicUsize,
    pub hold_remove: Mutex<Option<oneshot::Receiver<()>>>,
    pub on_create: Mutex<Option<Hook>>,
    pub on_remove: Mutex<Option<Hook>>,
    pub schema_version: Mutex<Option<u32>>,
    pub fail_stats: Mutex<Option<AppError>>,
    pub hits: Mutex<Option<Vec<VectorHit>>>,
    pub searches: Mutex<Vec<VectorSearchRequest>>,
}

impl ProbeVectorCore {
    pub(crate) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        mutex.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn versioned(&self, mut stats: VectorStats) -> VectorStats {
        if let Some(version) = *Self::lock(&self.schema_version) {
            stats.schema_version = version;
        }
        stats
    }

    pub fn searches(&self) -> Vec<VectorSearchRequest> {
        Self::lock(&self.searches).clone()
    }

    pub fn last_search(&self) -> VectorSearchRequest {
        self.searches().pop().expect("a search")
    }

    fn run(hook: &Mutex<Option<Hook>>) {
        let hook = Self::lock(hook).clone();
        if let Some(hook) = hook {
            hook();
        }
    }
}

fn native(message: &str) -> AppError {
    AppError::new(AppErrorCode::NativeError, message)
}

#[async_trait]
impl VectorCore for ProbeVectorCore {
    async fn create_index(
        &self,
        path: &Path,
        dimension: u32,
        metric: crate::drivers::core::VectorMetric,
    ) -> Result<VectorStats, AppError> {
        Self::run(&self.on_create);
        if self.fail_create.load(Ordering::SeqCst) {
            return Err(native("failed"));
        }
        Ok(self.versioned(self.inner.create_index(path, dimension, metric).await?))
    }

    async fn open_index(&self, path: &Path) -> Result<VectorStats, AppError> {
        Ok(self.versioned(self.inner.open_index(path).await?))
    }

    async fn upsert(
        &self,
        path: &Path,
        rows: Vec<VectorRow>,
        token: &CancellationToken,
    ) -> Result<usize, AppError> {
        self.inner.upsert(path, rows, token).await
    }

    async fn search(
        &self,
        path: &Path,
        request: VectorSearchRequest,
    ) -> Result<Vec<VectorHit>, AppError> {
        Self::lock(&self.searches).push(request.clone());
        let scripted = Self::lock(&self.hits).clone();
        match scripted {
            Some(hits) => Ok(hits),
            None => self.inner.search(path, request).await,
        }
    }

    async fn chunks(
        &self,
        path: &Path,
        document_id: &str,
        indices: &[u32],
    ) -> Result<Vec<VectorChunk>, AppError> {
        self.inner.chunks(path, document_id, indices).await
    }

    async fn maintain(&self, path: &Path, ann_threshold: usize) -> Result<VectorStats, AppError> {
        Ok(self.versioned(self.inner.maintain(path, ann_threshold).await?))
    }

    async fn delete_by_ids(&self, path: &Path, ids: &[String]) -> Result<(), AppError> {
        self.inner.delete_by_ids(path, ids).await
    }

    async fn delete_by_source(&self, path: &Path, document_id: &str) -> Result<(), AppError> {
        self.inner.delete_by_source(path, document_id).await
    }

    async fn stats(&self, path: &Path) -> Result<VectorStats, AppError> {
        if let Some(error) = Self::lock(&self.fail_stats).clone() {
            return Err(error);
        }
        Ok(self.versioned(self.inner.stats(path).await?))
    }

    async fn remove_index(&self, path: &Path) -> Result<(), AppError> {
        self.remove_calls.fetch_add(1, Ordering::SeqCst);
        Self::run(&self.on_remove);
        let held = Self::lock(&self.hold_remove).take();
        if let Some(gate) = held {
            let _ = gate.await;
        }
        let failing = self.fail_remove.load(Ordering::SeqCst);
        if failing > 0 {
            self.fail_remove.store(failing - 1, Ordering::SeqCst);
            return Err(native("locked"));
        }
        self.inner.remove_index(path).await
    }
}

pub(crate) enum Answer {
    Scores(Vec<f64>),
    Fail(&'static str),
    Hang,
    CancelThenFail(CancellationToken),
}

#[derive(Default)]
pub(crate) struct ScriptedReranker {
    pub answers: Mutex<VecDeque<Answer>>,
    pub calls: Mutex<Vec<(String, Vec<String>)>>,
    pub models: Mutex<Vec<String>>,
}

impl ScriptedReranker {
    pub fn answer(&self, answer: Answer) {
        ProbeVectorCore::lock(&self.answers).push_back(answer);
    }

    pub fn source(self: &Arc<Self>) -> RerankerSource {
        let reranker = Arc::clone(self);
        Arc::new(move |model_ref: &str| {
            ProbeVectorCore::lock(&reranker.models).push(model_ref.to_owned());
            Arc::clone(&reranker) as SharedReranker
        })
    }

    pub fn calls(&self) -> Vec<(String, Vec<String>)> {
        ProbeVectorCore::lock(&self.calls).clone()
    }

    pub fn models(&self) -> Vec<String> {
        ProbeVectorCore::lock(&self.models).clone()
    }
}

#[async_trait]
impl Reranker for ScriptedReranker {
    async fn rerank(
        &self,
        query: &str,
        passages: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<f64>, AppError> {
        ProbeVectorCore::lock(&self.calls).push((query.to_owned(), passages.to_vec()));
        let answer = ProbeVectorCore::lock(&self.answers)
            .pop_front()
            .unwrap_or(Answer::Scores(vec![0.0; passages.len()]));
        match answer {
            Answer::Scores(scores) => Ok(scores),
            Answer::Fail(message) => Err(AppError::new(AppErrorCode::Unknown, message)),
            Answer::Hang => {
                cancel.cancelled().await;
                Err(cancelled_error())
            }
            Answer::CancelThenFail(outer) => {
                outer.cancel();
                Err(AppError::new(AppErrorCode::Unknown, "aborted"))
            }
        }
    }
}

pub(crate) struct Fixture {
    pub service: VectorStoreService,
    pub core: Arc<ProbeVectorCore>,
    pub drivers: Arc<FakeDriverSource>,
    pub logger: Arc<MemoryLogger>,
    pub provider: ProviderId,
    pub database: TemporaryDatabase,
}

pub(crate) struct Setup {
    pub stopwatch: Option<Stopwatch>,
    pub rerankers: Option<RerankerSource>,
    pub rerank_timeout: Duration,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            stopwatch: None,
            rerankers: None,
            rerank_timeout: DEFAULT_RERANK_TIMEOUT,
        }
    }
}

pub(crate) fn unit_vectors() -> Arc<FakeDriver> {
    FakeDriver::with_vectors(vec![vec![1.0, 0.0, 0.0]])
}

pub(crate) fn fixture() -> Fixture {
    fixture_with(Setup::default())
}

pub(crate) fn fixture_with(setup: Setup) -> Fixture {
    let database = temporary_database();
    let provider = database
        .with_connection(|connection| Ok(seed_embedding_provider(connection)))
        .expect("seeds a provider");
    let core = Arc::new(ProbeVectorCore::default());
    let drivers = FakeDriverSource::shared(unit_vectors());
    let logger = MemoryLogger::shared();
    let mut options = VectorStoreServiceOptions::new(
        database.shared(),
        core.clone(),
        drivers.clone(),
        database.directory().join("stores"),
        logger.clone(),
        Arc::new(FakeClock::new(1000)),
    );
    if let Some(stopwatch) = setup.stopwatch {
        options.stopwatch = stopwatch;
    }
    options.rerankers = setup.rerankers;
    options.rerank_timeout = setup.rerank_timeout;
    Fixture {
        service: VectorStoreService::new(options),
        core,
        drivers,
        logger,
        provider: ProviderId::parse(&provider).expect("an id"),
        database,
    }
}

impl Fixture {
    pub fn input(&self) -> CreateVectorStoreInput {
        CreateVectorStoreInput {
            dimension: Some(3),
            ..CreateVectorStoreInput::new("Knowledge", self.provider, "embed-model")
        }
    }

    pub async fn store(&self) -> VectorStoreDto {
        self.service.create(&self.input()).await.expect("creates")
    }

    pub fn row(&self, id: &str) -> Option<VectorStoreRow> {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).vector_stores.find_by_id(id)?)
            })
            .expect("reads")
    }

    pub fn rows(&self) -> Vec<VectorStoreRow> {
        self.database
            .with_connection(|connection| Ok(Repositories::new(connection).vector_stores.list()?))
            .expect("reads")
    }

    pub fn patch(&self, id: &str, patch: &VectorStorePatch) {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .vector_stores
                    .update(id, patch)?)
            })
            .expect("patches");
    }

    pub fn path(&self, id: &str) -> PathBuf {
        self.service.store_path(id).expect("a path")
    }

    pub fn record_document(&self, store_id: &str, path: &str) -> String {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .vector_documents
                    .record_indexed(&VectorDocumentDraft {
                        id: new_id().hyphenated().to_string(),
                        store_id: store_id.to_owned(),
                        source_path: path.to_owned(),
                        content_hash: path.to_owned(),
                        chunk_count: 1,
                        bytes: 10,
                        indexed_at: 1,
                    })?
                    .id)
            })
            .expect("records")
    }

    pub async fn upsert(&self, store_id: &str, rows: Vec<VectorRow>) {
        self.core
            .inner
            .upsert(&self.path(store_id), rows, &CancellationToken::new())
            .await
            .expect("upserts");
    }

    pub async fn search(
        &self,
        store_id: &str,
        query: &str,
        options: VectorSearchOptions,
    ) -> Result<VectorSearchResultDto, AppError> {
        self.service
            .search_timed(store_id, query, options, &CancellationToken::new())
            .await
    }
}

pub(crate) fn vector_row(document_id: &str, chunk_index: u32, text: &str) -> VectorRow {
    VectorRow {
        id: format!("{document_id}:{chunk_index}"),
        vector: vec![1.0, 0.0, 0.0],
        payload: json!({ "chunkCount": 3 }),
        document_id: document_id.to_owned(),
        chunk_index,
        path: format!("{document_id}.md"),
        text: text.to_owned(),
        section: "Раздел".to_owned(),
        page: Some(chunk_index + 1),
    }
}

fn options(k: u64) -> VectorSearchOptions {
    VectorSearchOptions {
        k: Some(k),
        ..VectorSearchOptions::default()
    }
}

#[tokio::test]
async fn cancelling_retrieval_aborts_embedding_and_releases_the_store() {
    let fixture = fixture();
    let store = fixture.store().await;
    let manual = FakeDriver::shared(FakeScript {
        manual: Some(true),
        vectors: Some(vec![vec![1.0, 0.0, 0.0]]),
        ..FakeScript::default()
    });
    fixture.drivers.set(manual.clone());
    let cancel = CancellationToken::new();
    let searching = {
        let service = fixture.service.clone();
        let id = store.id.to_string();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            service
                .search(&id, "query", VectorSearchOptions::default(), &cancel)
                .await
        })
    };
    manual.when_holding().await;
    cancel.cancel();
    assert!(searching.await.expect("joins").is_err());
    fixture.drivers.set(unit_vectors());
    assert!(
        fixture
            .service
            .search(
                &store.id.to_string(),
                "query",
                VectorSearchOptions::default(),
                &CancellationToken::new()
            )
            .await
            .expect("searches")
            .is_empty()
    );
}

#[tokio::test]
async fn timed_search_measures_embedding_and_native_search_separately() {
    let readings = Arc::new(Mutex::new(VecDeque::from([100.0, 118.0, 120.0, 165.0])));
    let fixture = fixture_with(Setup {
        stopwatch: Some(Arc::new(move || {
            ProbeVectorCore::lock(&readings).pop_front().unwrap_or(0.0)
        })),
        ..Setup::default()
    });
    let store = fixture.store().await;
    let result = fixture
        .search(
            &store.id.to_string(),
            "hello",
            VectorSearchOptions::default(),
        )
        .await
        .expect("searches");
    assert_eq!(
        result,
        VectorSearchResultDto {
            hits: Vec::new(),
            embedding_ms: 18.0,
            search_ms: 45.0,
            rerank_ms: None,
            rerank: None,
            mode: Some(VectorSearchMode::Hybrid),
        }
    );
}

#[tokio::test]
async fn creation_persists_the_pending_row_before_native_work_and_rolls_back_partial_creation() {
    let fixture = fixture();
    let seen = Arc::new(Mutex::new(Vec::new()));
    {
        let seen = Arc::clone(&seen);
        let database = fixture.database.shared();
        *ProbeVectorCore::lock(&fixture.core.on_create) = Some(Arc::new(move || {
            let statuses = database
                .with_connection(|connection| {
                    Ok(Repositories::new(connection).vector_stores.list()?)
                })
                .expect("reads")
                .into_iter()
                .map(|row| row.status);
            ProbeVectorCore::lock(&seen).extend(statuses);
        }));
    }
    fixture.core.fail_create.store(true, Ordering::SeqCst);
    let error = fixture
        .service
        .create(&fixture.input())
        .await
        .expect_err("fails");
    assert_eq!(error.message, "failed");
    assert_eq!(*ProbeVectorCore::lock(&seen), [VectorHealth::Pending]);
    assert_eq!(fixture.core.remove_calls.load(Ordering::SeqCst), 1);
    assert!(fixture.rows().is_empty());
}

#[tokio::test]
async fn failed_creation_retains_recoverable_metadata_if_native_cleanup_also_fails() {
    let fixture = fixture();
    fixture.core.fail_create.store(true, Ordering::SeqCst);
    fixture.core.fail_remove.store(1, Ordering::SeqCst);
    let error = fixture
        .service
        .create(&fixture.input())
        .await
        .expect_err("fails");
    assert_eq!(error.code, AppErrorCode::NativeError);
    assert_eq!(error.message, CREATION_CLEANUP_FAILED);
    assert_eq!(fixture.rows()[0].status, VectorHealth::Broken);
    assert_eq!(
        fixture
            .logger
            .matching("Could not clean up failed creation")
            .len(),
        1
    );
}

#[tokio::test]
async fn delete_removes_native_storage_first_cascades_documents_and_restricts_provider_deletion() {
    let fixture = fixture();
    let store = fixture.store().await;
    let id = store.id.to_string();
    fixture.record_document(&id, "a.md");
    let provider = fixture.provider.to_string();
    assert!(
        fixture
            .database
            .with_connection(|connection| Ok(Repositories::new(connection)
                .providers
                .remove(&provider)?))
            .is_err()
    );
    let present = Arc::new(Mutex::new(Vec::new()));
    {
        let present = Arc::clone(&present);
        let database = fixture.database.shared();
        let id = id.clone();
        *ProbeVectorCore::lock(&fixture.core.on_remove) = Some(Arc::new(move || {
            let found = database
                .with_connection(|connection| {
                    Ok(Repositories::new(connection)
                        .vector_stores
                        .find_by_id(&id)?)
                })
                .expect("reads")
                .is_some();
            ProbeVectorCore::lock(&present).push(found);
        }));
    }
    fixture.service.remove(&id).await.expect("removes");
    assert_eq!(*ProbeVectorCore::lock(&present), [true]);
    assert!(!fixture.path(&id).exists());
    assert!(
        fixture
            .database
            .with_connection(|connection| Ok(Repositories::new(connection)
                .vector_documents
                .list_by_store(&id)?))
            .expect("reads")
            .is_empty()
    );
    assert!(fixture.service.list().await.expect("lists").is_empty());
    assert!(
        fixture
            .database
            .with_connection(|connection| Ok(Repositories::new(connection)
                .providers
                .remove(&provider)?))
            .is_ok()
    );
}

#[tokio::test]
async fn failed_deletion_logs_and_preserves_metadata_and_a_later_removal_succeeds() {
    let fixture = fixture();
    let store = fixture.store().await;
    let id = store.id.to_string();
    fixture.core.fail_remove.store(1, Ordering::SeqCst);
    let error = fixture.service.remove(&id).await.expect_err("fails");
    assert_eq!(error.code, AppErrorCode::NativeError);
    assert!(fixture.row(&id).is_some());
    assert!(fixture.path(&id).exists());
    assert_eq!(
        fixture
            .logger
            .matching("Could not remove vector store")
            .len(),
        1
    );
    fixture.service.remove(&id).await.expect("removes");
}

#[tokio::test]
async fn list_reports_drift_and_opening_and_explicit_reconciliation_repair_all_native_counts() {
    let fixture = fixture();
    let store = fixture.store().await;
    let id = store.id.to_string();
    fixture
        .upsert(
            &id,
            vec![
                vector_row("a", 0, "one"),
                vector_row("a", 1, "two"),
                vector_row("a", 2, "three"),
                vector_row("b", 0, "four"),
                vector_row("b", 1, "five"),
            ],
        )
        .await;
    let listed = fixture.service.list().await.expect("lists");
    assert_eq!(listed[0].status, VectorHealth::Stale);
    let stats = fixture
        .core
        .inner
        .stats(&fixture.path(&id))
        .await
        .expect("stats");
    let opened = fixture.service.get(&id).await.expect("opens");
    assert_eq!(opened.status, VectorHealth::Healthy);
    assert_eq!((opened.documents, opened.vectors), (2, 5));
    assert_eq!(opened.bytes, stats.on_disk_bytes);
    assert_eq!(opened.last_indexed_at, None);

    fixture
        .core
        .inner
        .delete_by_source(&fixture.path(&id), "b")
        .await
        .expect("deletes");
    fixture
        .core
        .inner
        .delete_by_ids(&fixture.path(&id), &["a:1".to_owned(), "a:2".to_owned()])
        .await
        .expect("deletes");
    let stats = fixture
        .core
        .inner
        .stats(&fixture.path(&id))
        .await
        .expect("stats");
    let reconciled = fixture.service.reconcile(&id).await.expect("reconciles");
    assert_eq!((reconciled.documents, reconciled.vectors), (1, 1));
    assert_eq!(reconciled.bytes, stats.on_disk_bytes);

    fixture
        .core
        .inner
        .remove_index(&fixture.path(&id))
        .await
        .expect("drops the table");
    assert_eq!(
        fixture.service.get(&id).await.expect("opens").status,
        VectorHealth::Broken
    );
    fixture.service.remove(&id).await.expect("removes");
}

#[tokio::test]
async fn a_store_without_a_table_is_pending_until_the_table_fails_to_open() {
    let fixture = fixture();
    let id = new_id().hyphenated().to_string();
    let provider = fixture.provider.to_string();
    fixture
        .database
        .with_connection(|connection| {
            Ok(Repositories::new(connection)
                .vector_stores
                .create(&VectorStoreDraft::new(&id, "Pending", &provider, "m", 3, 1))?)
        })
        .expect("creates");
    assert_eq!(
        fixture.service.get(&id).await.expect("opens").status,
        VectorHealth::Pending
    );
    *ProbeVectorCore::lock(&fixture.core.fail_stats) = Some(native("corrupt"));
    assert_eq!(
        fixture.service.get(&id).await.expect("opens").status,
        VectorHealth::Broken
    );
    assert_eq!(
        fixture
            .logger
            .matching("Could not inspect vector store")
            .len(),
        1
    );
}

#[tokio::test]
async fn search_embeds_with_the_saved_provider_and_external_model_then_passes_the_vector_to_rust() {
    let fixture = fixture();
    let store = fixture.store().await;
    let id = store.id.to_string();
    fixture
        .search(
            &id,
            "question",
            VectorSearchOptions {
                k: Some(5),
                min_score: Some(0.3),
                ..VectorSearchOptions::default()
            },
        )
        .await
        .expect("searches");
    assert_eq!(
        fixture.drivers.driver().expect("a driver").embeddings(),
        [(vec!["question".to_owned()], "embed-model".to_owned())]
    );
    assert_eq!(
        fixture.core.last_search(),
        VectorSearchRequest {
            vector: vec![1.0, 0.0, 0.0],
            k: 5,
            min_score: 0.3,
            mode: Some(CoreMode::Hybrid),
            query: Some("question".to_owned()),
            ..VectorSearchRequest::default()
        }
    );
    fixture
        .drivers
        .set(FakeDriver::with_vectors(vec![vec![1.0]]));
    assert_eq!(
        fixture
            .search(&id, "question", VectorSearchOptions::default())
            .await
            .expect_err("refused")
            .code,
        AppErrorCode::ValidationFailed
    );
    fixture
        .database
        .with_connection(|connection| {
            connection.execute(
                "UPDATE provider SET enabled = 0 WHERE id = ?1",
                [fixture.provider.to_string()],
            )?;
            Ok(())
        })
        .expect("disables");
    assert_eq!(
        fixture
            .search(&id, "question", VectorSearchOptions::default())
            .await
            .expect_err("refused")
            .code,
        AppErrorCode::Conflict
    );
}

#[tokio::test]
async fn the_query_prefix_reaches_the_embedder_and_the_mode_follows_the_store_unless_overridden() {
    let fixture = fixture();
    let store = fixture
        .service
        .create(&CreateVectorStoreInput {
            query_prefix: Some("query: ".to_owned()),
            search_mode: Some(VectorSearchMode::Vector),
            ..fixture.input()
        })
        .await
        .expect("creates");
    assert_eq!(store.query_prefix, "query: ");
    assert_eq!(store.search_mode, VectorSearchMode::Vector);
    assert_eq!(store.schema_version, 2);
    let id = store.id.to_string();

    let plain = fixture
        .search(&id, "отпуск", VectorSearchOptions::default())
        .await
        .expect("searches");
    assert_eq!(
        fixture
            .drivers
            .driver()
            .expect("a driver")
            .embeddings()
            .last()
            .cloned(),
        Some((vec!["query: отпуск".to_owned()], "embed-model".to_owned()))
    );
    assert_eq!(plain.mode, Some(VectorSearchMode::Vector));
    assert_eq!(fixture.core.last_search().mode, Some(CoreMode::Vector));

    let hybrid = fixture
        .search(
            &id,
            "отпуск",
            VectorSearchOptions {
                mode: Some(VectorSearchMode::Hybrid),
                ..VectorSearchOptions::default()
            },
        )
        .await
        .expect("searches");
    assert_eq!(hybrid.mode, Some(VectorSearchMode::Hybrid));
    let request = fixture.core.last_search();
    assert_eq!(request.mode, Some(CoreMode::Hybrid));
    assert_eq!(request.query.as_deref(), Some("отпуск"));
}

#[tokio::test]
async fn a_v1_store_searches_by_vectors_only_whatever_its_mode_says() {
    let fixture = fixture();
    let store = fixture.store().await;
    let id = store.id.to_string();
    *ProbeVectorCore::lock(&fixture.core.schema_version) = Some(1);
    assert_eq!(
        fixture
            .service
            .reconcile(&id)
            .await
            .expect("reconciles")
            .schema_version,
        1
    );
    let result = fixture
        .search(
            &id,
            "q",
            VectorSearchOptions {
                mode: Some(VectorSearchMode::Hybrid),
                ..VectorSearchOptions::default()
            },
        )
        .await
        .expect("searches");
    assert_eq!(result.mode, Some(VectorSearchMode::Vector));
    assert_eq!(
        effective_mode(VectorSearchMode::Hybrid, 2, None),
        VectorSearchMode::Hybrid
    );
    assert_eq!(
        effective_mode(VectorSearchMode::Hybrid, 2, Some(VectorSearchMode::Vector)),
        VectorSearchMode::Vector
    );
    assert_eq!(
        effective_mode(VectorSearchMode::Hybrid, 1, None),
        VectorSearchMode::Vector
    );
}

#[tokio::test]
async fn prefixes_are_frozen_once_the_store_holds_vectors_but_the_search_mode_is_not() {
    let fixture = fixture();
    let store = fixture.store().await;
    let id = store.id;
    let updated = fixture
        .service
        .update(&UpdateVectorStoreInput {
            query_prefix: Some("q: ".to_owned()),
            document_prefix: Some("d: ".to_owned()),
            ..UpdateVectorStoreInput::new(id)
        })
        .await
        .expect("updates");
    assert_eq!(
        (
            updated.query_prefix.as_str(),
            updated.document_prefix.as_str()
        ),
        ("q: ", "d: ")
    );
    fixture.patch(
        &id.to_string(),
        &VectorStorePatch {
            last_indexed_at: Some(Some(5)),
            ..VectorStorePatch::default()
        },
    );
    let frozen = fixture
        .service
        .update(&UpdateVectorStoreInput {
            query_prefix: Some("other: ".to_owned()),
            ..UpdateVectorStoreInput::new(id)
        })
        .await
        .expect_err("refused");
    assert_eq!(frozen.code, AppErrorCode::Conflict);
    assert_eq!(frozen.message, SETTINGS_FROZEN);
    assert_eq!(
        fixture
            .service
            .update(&UpdateVectorStoreInput {
                search_mode: Some(VectorSearchMode::Vector),
                ..UpdateVectorStoreInput::new(id)
            })
            .await
            .expect("updates")
            .search_mode,
        VectorSearchMode::Vector
    );
}

#[tokio::test]
async fn neighbour_chunks_and_the_best_chunk_of_a_related_document_are_read_from_the_index() {
    let fixture = fixture();
    let store = fixture.store().await;
    let id = store.id.to_string();
    fixture
        .upsert(
            &id,
            vec![
                vector_row("a", 0, "first"),
                vector_row("a", 1, "second"),
                vector_row("a", 2, "third"),
                vector_row("b", 0, "other"),
            ],
        )
        .await;
    let chunks = fixture
        .service
        .chunks(&id, "a", &[2, 0, 0])
        .await
        .expect("reads");
    assert_eq!(
        chunks
            .iter()
            .map(|chunk| (chunk.chunk_index, chunk.text.as_str(), chunk.page))
            .collect::<Vec<_>>(),
        [(0, "first", Some(1)), (2, "third", Some(3))]
    );
    assert!(
        fixture
            .service
            .chunks(&id, "a", &[])
            .await
            .expect("reads")
            .is_empty()
    );

    let related = fixture
        .service
        .related_chunks(
            &id,
            "q",
            &["b".to_owned(), "b".to_owned(), "missing".to_owned()],
            &CancellationToken::new(),
        )
        .await
        .expect("reads");
    assert_eq!(
        related
            .iter()
            .map(|hit| hit.id.as_str())
            .collect::<Vec<_>>(),
        ["b:0"]
    );
    let last = fixture.core.last_search();
    assert_eq!(last.k, 1);
    assert_eq!(last.mode, Some(CoreMode::Vector));
    assert_eq!(last.document_ids, Some(vec!["missing".to_owned()]));
}

#[tokio::test]
async fn names_backends_and_overlaps_are_checked_on_create_and_update() {
    let fixture = fixture();
    let store = fixture.store().await;
    let renamed = fixture
        .service
        .update(&UpdateVectorStoreInput {
            name: Some("  New ".to_owned()),
            ..UpdateVectorStoreInput::new(store.id)
        })
        .await
        .expect("updates");
    assert_eq!(renamed.name, "New");
    assert_eq!(
        fixture
            .service
            .create(&CreateVectorStoreInput {
                name: "New".to_owned(),
                ..fixture.input()
            })
            .await
            .expect_err("refused")
            .code,
        AppErrorCode::Conflict
    );
    assert_eq!(
        fixture
            .service
            .create(&CreateVectorStoreInput {
                name: "Other".to_owned(),
                backend: Some(VectorBackend::Qdrant),
                ..fixture.input()
            })
            .await
            .expect_err("refused")
            .code,
        AppErrorCode::UnsupportedFormat
    );
    assert_eq!(
        fixture
            .service
            .update(&UpdateVectorStoreInput {
                chunk_overlap: Some(999),
                ..UpdateVectorStoreInput::new(store.id)
            })
            .await
            .expect_err("refused")
            .message,
        OVERLAP_TOO_LARGE
    );
    let id = store.id.to_string();
    fixture.service.remove(&id).await.expect("removes");
    assert_eq!(
        fixture.service.get(&id).await.expect_err("gone").code,
        AppErrorCode::NotFound
    );
}

#[tokio::test]
async fn mutations_cannot_race_active_native_operations() {
    let fixture = fixture();
    let store = fixture.store().await;
    let id = store.id.to_string();
    let (release, gate) = oneshot::channel();
    *ProbeVectorCore::lock(&fixture.core.hold_remove) = Some(gate);
    let removing = {
        let service = fixture.service.clone();
        let id = id.clone();
        tokio::spawn(async move { service.remove(&id).await })
    };
    while fixture.core.remove_calls.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        fixture
            .service
            .update(&UpdateVectorStoreInput {
                name: Some("New".to_owned()),
                ..UpdateVectorStoreInput::new(store.id)
            })
            .await
            .expect_err("busy")
            .message,
        STORE_BUSY
    );
    assert_eq!(
        fixture.service.reconcile(&id).await.expect_err("busy").code,
        AppErrorCode::Conflict
    );
    assert_eq!(
        fixture.service.list().await.expect("lists")[0].status,
        VectorHealth::Healthy
    );
    release.send(()).expect("releases");
    removing.await.expect("joins").expect("removes");
}

#[tokio::test]
async fn advanced_stages_round_trip_and_an_enabled_stage_without_a_model_is_refused() {
    let fixture = fixture();
    let store = fixture
        .service
        .create(&CreateVectorStoreInput {
            rerank: Some(VectorRerankConfig {
                enabled: true,
                model_ref: "curated:model:bge-reranker-v2-m3-f16".to_owned(),
                candidates: 80,
            }),
            ocr: Some(VectorOcrConfig {
                enabled: true,
                model_ref: "curated:model:qwen2.5-vl-7b-instruct-q4_k_m".to_owned(),
                language: VectorOcrLanguage::RusEng,
                min_chars_per_page: 120,
            }),
            ..fixture.input()
        })
        .await
        .expect("creates");
    assert_eq!(
        store.rerank,
        VectorRerankConfig {
            enabled: true,
            model_ref: "curated:model:bge-reranker-v2-m3-f16".to_owned(),
            candidates: 80,
        }
    );
    assert_eq!(store.ocr.language, VectorOcrLanguage::RusEng);
    assert_eq!(
        fixture
            .service
            .get(&store.id.to_string())
            .await
            .expect("opens")
            .ocr
            .min_chars_per_page,
        120
    );
    let updated = fixture
        .service
        .update(&UpdateVectorStoreInput {
            ocr: Some(VectorOcrConfig::default()),
            ..UpdateVectorStoreInput::new(store.id)
        })
        .await
        .expect("updates");
    assert!(!updated.ocr.enabled);
    assert_eq!(updated.rerank.candidates, 80);
    let blank = fixture
        .service
        .update(&UpdateVectorStoreInput {
            rerank: Some(VectorRerankConfig {
                enabled: true,
                model_ref: "  ".to_owned(),
                candidates: 10,
            }),
            ..UpdateVectorStoreInput::new(store.id)
        })
        .await
        .expect_err("refused");
    assert_eq!(blank.code, AppErrorCode::ValidationFailed);
    assert_eq!(blank.message, RERANK_MODEL_REQUIRED);
    let ocr = fixture
        .service
        .create(&CreateVectorStoreInput {
            name: "Other".to_owned(),
            ocr: Some(VectorOcrConfig {
                enabled: true,
                ..VectorOcrConfig::default()
            }),
            ..fixture.input()
        })
        .await
        .expect_err("refused");
    assert_eq!(ocr.message, OCR_MODEL_REQUIRED);
}

#[tokio::test]
async fn a_slow_query_embedding_does_not_hold_the_store_lock() {
    let fixture = fixture();
    let store = fixture.store().await;
    let manual = FakeDriver::shared(FakeScript {
        manual: Some(true),
        vectors: Some(vec![vec![1.0, 0.0, 0.0]]),
        ..FakeScript::default()
    });
    fixture.drivers.set(manual.clone());
    let searching = {
        let service = fixture.service.clone();
        let id = store.id.to_string();
        tokio::spawn(async move {
            service
                .search_timed(
                    &id,
                    "query",
                    VectorSearchOptions::default(),
                    &CancellationToken::new(),
                )
                .await
        })
    };
    manual.when_holding().await;
    let (row, lease) = fixture.service.lease(&store.id.to_string()).expect("free");
    assert_eq!(row.id, store.id.to_string());
    drop(lease);
    assert!(
        (fixture
            .service
            .update(&UpdateVectorStoreInput {
                chat_min_score: Some(0.5),
                ..UpdateVectorStoreInput::new(store.id)
            })
            .await
            .expect("updates")
            .chat_min_score
            - 0.5)
            .abs()
            < f64::EPSILON
    );
    manual.release();
    assert!(
        searching
            .await
            .expect("joins")
            .expect("searches")
            .hits
            .is_empty()
    );
}

#[tokio::test]
async fn a_search_whose_store_changed_while_embedding_is_refused() {
    let fixture = fixture();
    let store = fixture.store().await;
    let manual = FakeDriver::shared(FakeScript {
        manual: Some(true),
        vectors: Some(vec![vec![1.0, 0.0, 0.0]]),
        ..FakeScript::default()
    });
    fixture.drivers.set(manual.clone());
    let searching = {
        let service = fixture.service.clone();
        let id = store.id.to_string();
        tokio::spawn(async move {
            service
                .search_timed(
                    &id,
                    "query",
                    VectorSearchOptions::default(),
                    &CancellationToken::new(),
                )
                .await
        })
    };
    manual.when_holding().await;
    fixture.patch(
        &store.id.to_string(),
        &VectorStorePatch {
            query_prefix: Some("query: ".to_owned()),
            ..VectorStorePatch::default()
        },
    );
    manual.release();
    let error = searching.await.expect("joins").expect_err("refused");
    assert_eq!(error.code, AppErrorCode::Conflict);
    assert_eq!(error.message, STORE_CHANGED);
}

#[tokio::test]
async fn clearing_recreates_the_table_and_drops_the_documents() {
    let fixture = fixture();
    let store = fixture.store().await;
    let id = store.id.to_string();
    fixture.record_document(&id, "a.md");
    fixture.upsert(&id, vec![vector_row("a", 0, "first")]).await;
    fixture.service.reconcile(&id).await.expect("reconciles");
    let cleared = fixture.service.clear(&id).await.expect("clears");
    assert_eq!(cleared.status, VectorHealth::Pending);
    assert_eq!(
        (cleared.documents, cleared.vectors, cleared.bytes),
        (0, 0, 0)
    );
    assert_eq!(
        fixture
            .core
            .inner
            .stats(&fixture.path(&id))
            .await
            .expect("stats")
            .row_count,
        0
    );
    assert_eq!(fixture.logger.matching("Reset a vector store").len(), 1);
    fixture.core.fail_remove.store(1, Ordering::SeqCst);
    let error = fixture.service.clear(&id).await.expect_err("fails");
    assert_eq!(error.message, CLEAR_FAILED);
    assert_eq!(fixture.row(&id).expect("kept").status, VectorHealth::Broken);
    let recreated = fixture
        .service
        .recreate_table(&id)
        .await
        .expect("recreates");
    assert_eq!(recreated.status, VectorHealth::Healthy);
}

const RERANK_MODEL: &str = "curated:model:bge-reranker-v2-m3-f16";

fn candidate(id: &str, score: f32, text: &str) -> VectorHit {
    VectorHit {
        id: id.to_owned(),
        score,
        payload: json!({ "chunkCount": 1 }),
        document_id: id.to_owned(),
        chunk_index: 0,
        path: format!("{id}.md"),
        text: text.to_owned(),
        section: String::new(),
        page: None,
        keyword_score: None,
        fused_score: None,
        provenance: None,
    }
}

fn candidates() -> Vec<VectorHit> {
    vec![
        candidate("a", 0.9, "about cats"),
        candidate("b", 0.8, "about dogs"),
        candidate("c", 0.7, "the answer"),
        candidate("d", 0.2, "noise"),
    ]
}

struct Reranking {
    fixture: Fixture,
    reranker: Arc<ScriptedReranker>,
}

fn reranking_with(timeout: Duration, attached: bool) -> Reranking {
    let reranker = Arc::new(ScriptedReranker::default());
    let fixture = fixture_with(Setup {
        rerankers: attached.then(|| reranker.source()),
        rerank_timeout: timeout,
        ..Setup::default()
    });
    *ProbeVectorCore::lock(&fixture.core.hits) = Some(candidates());
    Reranking { fixture, reranker }
}

fn reranking() -> Reranking {
    reranking_with(DEFAULT_RERANK_TIMEOUT, true)
}

impl Reranking {
    async fn store(&self, candidates: u64) -> String {
        self.fixture
            .service
            .create(&CreateVectorStoreInput {
                rerank: Some(VectorRerankConfig {
                    enabled: true,
                    model_ref: RERANK_MODEL.to_owned(),
                    candidates,
                }),
                ..self.fixture.input()
            })
            .await
            .expect("creates")
            .id
            .to_string()
    }
}

fn ids(hits: &[VectorSearchHitDto]) -> Vec<&str> {
    hits.iter().map(|hit| hit.id.as_str()).collect()
}

#[tokio::test]
async fn a_store_with_reranking_on_fetches_the_candidates_reranks_them_and_returns_k() {
    let setup = reranking();
    let id = setup.store(30).await;
    setup
        .reranker
        .answer(Answer::Scores(vec![-2.0, -1.0, 4.0, 0.0]));
    let result = setup
        .fixture
        .search(&id, "which one is the answer?", options(2))
        .await
        .expect("searches");
    let request = setup.fixture.core.last_search();
    assert_eq!(request.vector, [1.0, 0.0, 0.0]);
    assert_eq!(request.k, 30);
    assert!(request.min_score.abs() < f32::EPSILON);
    assert_eq!(request.mode, Some(CoreMode::Hybrid));
    assert_eq!(setup.reranker.models(), [RERANK_MODEL]);
    assert_eq!(
        setup.reranker.calls(),
        [(
            "which one is the answer?".to_owned(),
            vec![
                "about cats".to_owned(),
                "about dogs".to_owned(),
                "the answer".to_owned(),
                "noise".to_owned()
            ]
        )]
    );
    assert_eq!(result.rerank, Some(VectorRerankOutcome::Applied));
    assert!(result.rerank_ms.expect("timed") >= 0.0);
    assert_eq!(ids(&result.hits), ["c", "d"]);
    assert_eq!(result.hits[0].vector_score, Some(f64::from(0.7_f32)));
    assert_eq!(
        result.hits[0].rerank_score,
        Some(normalise_rerank_score(4.0))
    );
}

#[tokio::test]
async fn the_floor_applies_to_the_rerank_score_once_reranking_ran() {
    let setup = reranking();
    let id = setup.store(50).await;
    setup
        .reranker
        .answer(Answer::Scores(vec![-5.0, -5.0, 5.0, -5.0]));
    let result = setup
        .fixture
        .search(
            &id,
            "q",
            VectorSearchOptions {
                min_score: Some(0.5),
                ..options(10)
            },
        )
        .await
        .expect("searches");
    assert_eq!(ids(&result.hits), ["c"]);
}

#[tokio::test]
async fn k_larger_than_the_candidate_count_still_fetches_k() {
    let setup = reranking();
    let id = setup.store(2).await;
    setup
        .fixture
        .search(&id, "q", options(4))
        .await
        .expect("searches");
    let request = setup.fixture.core.last_search();
    assert_eq!(request.k, 4);
    assert!(request.min_score.abs() < f32::EPSILON);
}

#[tokio::test]
async fn a_store_with_reranking_off_never_touches_the_reranker() {
    let setup = reranking();
    let store = setup.fixture.store().await;
    let result = setup
        .fixture
        .search(
            &store.id.to_string(),
            "q",
            VectorSearchOptions {
                min_score: Some(0.1),
                ..options(3)
            },
        )
        .await
        .expect("searches");
    let request = setup.fixture.core.last_search();
    assert_eq!(request.k, 3);
    assert!((request.min_score - 0.1).abs() < f32::EPSILON);
    assert!(setup.reranker.models().is_empty());
    assert_eq!(result.rerank, None);
    assert_eq!(result.rerank_ms, None);
    assert_eq!(result.hits[0].rerank_score, None);
}

#[tokio::test]
async fn the_caller_can_ask_for_vector_order_on_a_reranking_store() {
    let setup = reranking();
    let id = setup.store(50).await;
    let result = setup
        .fixture
        .search(
            &id,
            "q",
            VectorSearchOptions {
                rerank: false,
                ..options(2)
            },
        )
        .await
        .expect("searches");
    assert!(setup.reranker.calls().is_empty());
    assert_eq!(result.rerank, None);
    assert_eq!(ids(&result.hits), ["a", "b", "c", "d"]);
}

#[tokio::test]
async fn a_crashing_reranker_degrades_to_vector_order_with_a_warning() {
    let setup = reranking();
    let id = setup.store(50).await;
    setup.reranker.answer(Answer::Fail("connection reset"));
    let result = setup
        .fixture
        .search(
            &id,
            "q",
            VectorSearchOptions {
                min_score: Some(0.75),
                ..options(2)
            },
        )
        .await
        .expect("searches");
    assert_eq!(result.rerank, Some(VectorRerankOutcome::Fallback));
    assert_eq!(ids(&result.hits), ["a", "b"]);
    assert_eq!(result.hits[0].rerank_score, None);
    let warnings = setup.fixture.logger.matching(RERANK_FAILED);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].fields["storeId"], json!(id));
    assert_eq!(warnings[0].fields["timedOut"], json!(false));
}

#[tokio::test]
async fn a_reranker_that_never_answers_times_out_into_vector_order() {
    let setup = reranking_with(Duration::from_millis(20), true);
    let id = setup.store(50).await;
    setup.reranker.answer(Answer::Hang);
    let result = setup
        .fixture
        .search(&id, "q", options(3))
        .await
        .expect("searches");
    assert_eq!(result.rerank, Some(VectorRerankOutcome::Fallback));
    assert_eq!(ids(&result.hits), ["a", "b", "c"]);
    assert_eq!(
        setup.fixture.logger.matching(RERANK_FAILED)[0].fields["timedOut"],
        json!(true)
    );
}

#[tokio::test]
async fn a_malformed_answer_is_treated_as_a_failure_not_applied() {
    let setup = reranking();
    let id = setup.store(50).await;
    setup.reranker.answer(Answer::Scores(vec![1.0, 2.0]));
    let result = setup
        .fixture
        .search(&id, "q", options(4))
        .await
        .expect("searches");
    assert_eq!(result.rerank, Some(VectorRerankOutcome::Fallback));
    assert_eq!(ids(&result.hits), ["a", "b", "c", "d"]);
    setup
        .reranker
        .answer(Answer::Scores(vec![1.0, f64::NAN, 0.0, 0.0]));
    assert_eq!(
        setup
            .fixture
            .search(&id, "q", options(4))
            .await
            .expect("searches")
            .rerank,
        Some(VectorRerankOutcome::Fallback)
    );
}

#[tokio::test]
async fn without_a_reranker_attached_the_search_still_answers() {
    let setup = reranking_with(DEFAULT_RERANK_TIMEOUT, false);
    let id = setup.store(50).await;
    let result = setup
        .fixture
        .search(&id, "q", options(1))
        .await
        .expect("searches");
    assert_eq!(result.rerank, Some(VectorRerankOutcome::Fallback));
    assert_eq!(ids(&result.hits), ["a"]);
    assert_eq!(setup.fixture.logger.matching(RERANK_UNAVAILABLE).len(), 1);
    setup
        .fixture
        .service
        .attach_rerankers(setup.reranker.source());
    setup
        .reranker
        .answer(Answer::Scores(vec![0.0, 0.0, 0.0, 9.0]));
    assert_eq!(
        setup
            .fixture
            .search(&id, "q", options(1))
            .await
            .expect("searches")
            .hits[0]
            .id,
        "d"
    );
}

#[tokio::test]
async fn cancelling_the_search_is_a_cancellation_not_a_fallback() {
    let setup = reranking();
    let id = setup.store(50).await;
    let cancel = CancellationToken::new();
    setup
        .reranker
        .answer(Answer::CancelThenFail(cancel.clone()));
    assert!(
        setup
            .fixture
            .service
            .search_timed(&id, "q", options(1), &cancel)
            .await
            .is_err()
    );
    assert!(setup.fixture.logger.matching(RERANK_FAILED).is_empty());
}

#[tokio::test]
async fn an_empty_candidate_list_is_applied_without_calling_the_reranker() {
    let setup = reranking();
    *ProbeVectorCore::lock(&setup.fixture.core.hits) = Some(Vec::new());
    let id = setup.store(50).await;
    let result = setup
        .fixture
        .search(&id, "q", options(3))
        .await
        .expect("searches");
    assert_eq!(result.rerank, Some(VectorRerankOutcome::Applied));
    assert!(setup.reranker.calls().is_empty());
}

#[tokio::test]
async fn a_long_query_is_capped_before_it_reaches_the_reranker() {
    let setup = reranking();
    let id = setup.store(50).await;
    setup
        .fixture
        .search(&id, &"ж".repeat(1500), options(1))
        .await
        .expect("searches");
    assert_eq!(
        setup.reranker.calls()[0].0.chars().count(),
        MAX_RERANK_QUERY_CHARS
    );
}

#[test]
fn hits_from_the_core_carry_their_scores_and_provenance() {
    let hit = hit_dto(VectorHit {
        keyword_score: Some(3.5),
        fused_score: Some(0.5),
        provenance: Some(VectorProvenance::Keyword),
        page: Some(2),
        ..candidate("a", 0.5, "x")
    });
    assert_eq!(hit.keyword_score, Some(3.5));
    assert_eq!(hit.fused_score, Some(0.5));
    assert_eq!(hit.provenance, Some(VectorHitProvenance::Keyword));
    assert_eq!(hit.page, Some(2));
    assert_eq!(hit.payload, json!({ "chunkCount": 1 }));
    let wire: Value = serde_json::to_value(&hit).expect("serialises");
    assert_eq!(wire["provenance"], json!("keyword"));
}
