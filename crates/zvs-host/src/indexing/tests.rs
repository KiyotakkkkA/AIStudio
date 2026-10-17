use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use futures::FutureExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::{Notify, oneshot};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AddVectorSourceInput, AppError, AppErrorCode, CreateVectorStoreInput, HostEvent,
    HostEventDraft, ProviderId, ResourceSampleDto, RunId, RunKind, RunStatus, VectorDocumentDto,
    VectorIndexInput, VectorIndexReportDto, VectorSourceKind,
};

use super::*;
use crate::data::Repositories;
use crate::data::repositories::VectorStorePatch;
use crate::data::schema::{ProviderRow, VectorDocumentRow};
use crate::drivers::ai::capabilities::AdapterCapabilities;
use crate::drivers::ai::ports::{AiDriver, DiscoveredModel, EmbeddingDriver};
use crate::drivers::core::{
    ChunkConfig, SectionChunk, TextChunk, TextCore, TextSegment, VectorChunk, VectorCore,
    VectorHit, VectorMetric, VectorRow, VectorSearchRequest, VectorStats,
};
use crate::ipc::vector_stores::start_index_run;
use crate::kernel::nodes::VectorIndexNode;
use crate::kernel::{KernelServices, NodeRegistry};
use crate::platform::events::EventBus;
use crate::platform::logger::silent_logger;
use crate::services::providers::DriverSource;
use crate::services::vector_sources::DEFAULT_ANN_THRESHOLD;
use crate::services::{
    RunService, RunServiceOptions, VectorSourceService, VectorSourceServiceOptions,
    VectorStoreService, VectorStoreServiceOptions,
};
use crate::testing::{
    FAKE_CAPABILITIES, FakeClock, TemporaryDatabase, TemporaryDirectory, seed_embedding_provider,
    temporary_database, temporary_directory,
};

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

struct WordCore;

fn word_chunks(text: &str, config: ChunkConfig) -> Vec<TextChunk> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let stride = config.size.saturating_sub(config.overlap).max(1);
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < words.len() {
        let slice = &words[start..words.len().min(start + config.size)];
        chunks.push(TextChunk {
            text: slice.join(" "),
            byte_start: u32::try_from(start).expect("small"),
            byte_end: u32::try_from(start + slice.len()).expect("small"),
            token_count: u32::try_from(slice.len()).expect("small"),
        });
        if start + config.size >= words.len() {
            break;
        }
        start += stride;
    }
    chunks
}

#[async_trait]
impl TextCore for WordCore {
    async fn chunk(&self, text: String, config: ChunkConfig) -> Result<Vec<TextChunk>, AppError> {
        Ok(word_chunks(&text, config))
    }

    async fn chunk_segments(
        &self,
        segments: Vec<TextSegment>,
        config: ChunkConfig,
    ) -> Result<Vec<SectionChunk>, AppError> {
        Ok(segments
            .into_iter()
            .flat_map(|segment| {
                word_chunks(&segment.text, config)
                    .into_iter()
                    .map(move |chunk| SectionChunk {
                        text: chunk.text,
                        byte_start: chunk.byte_start,
                        byte_end: chunk.byte_end,
                        token_count: chunk.token_count,
                        section: segment.section.clone(),
                        page: segment.page,
                    })
            })
            .collect())
    }

    async fn hash(&self, bytes: Vec<u8>) -> Result<String, AppError> {
        Ok(Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }
}

struct Table {
    dimension: u32,
    metric: VectorMetric,
    schema_version: u32,
    rows: Vec<VectorRow>,
}

#[derive(Default)]
struct MemoryCore {
    tables: Mutex<HashMap<PathBuf, Table>>,
    maintained: Mutex<Vec<(PathBuf, usize)>>,
}

impl MemoryCore {
    fn stats_of(table: &Table) -> VectorStats {
        let mut documents: Vec<&str> = table
            .rows
            .iter()
            .map(|row| row.document_id.as_str())
            .collect();
        documents.sort_unstable();
        documents.dedup();
        VectorStats {
            row_count: table.rows.len(),
            document_count: documents.len(),
            dimension: table.dimension,
            metric: table.metric,
            on_disk_bytes: 100,
            index_type: "FLAT".to_owned(),
            schema_version: table.schema_version,
            keyword_index: table.schema_version >= 2,
        }
    }

    fn missing() -> AppError {
        AppError::new(AppErrorCode::NotFound, "Missing table")
    }

    fn rows(&self, path: &Path) -> Vec<VectorRow> {
        lock(&self.tables)
            .get(path)
            .map(|table| table.rows.clone())
            .unwrap_or_default()
    }

    fn rows_for(&self, path: &Path, document_id: &str) -> Vec<VectorRow> {
        self.rows(path)
            .into_iter()
            .filter(|row| row.document_id == document_id)
            .collect()
    }

    fn set_schema_version(&self, path: &Path, version: u32) {
        if let Some(table) = lock(&self.tables).get_mut(path) {
            table.schema_version = version;
        }
    }

    fn maintained(&self) -> Vec<(PathBuf, usize)> {
        lock(&self.maintained).clone()
    }

    fn with_table<T>(
        &self,
        path: &Path,
        work: impl FnOnce(&mut Table) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut tables = lock(&self.tables);
        let table = tables.get_mut(path).ok_or_else(Self::missing)?;
        work(table)
    }
}

#[async_trait]
impl VectorCore for MemoryCore {
    async fn create_index(
        &self,
        path: &Path,
        dimension: u32,
        metric: VectorMetric,
    ) -> Result<VectorStats, AppError> {
        let table = Table {
            dimension,
            metric,
            schema_version: 2,
            rows: Vec::new(),
        };
        let stats = Self::stats_of(&table);
        lock(&self.tables).insert(path.to_path_buf(), table);
        Ok(stats)
    }

    async fn open_index(&self, path: &Path) -> Result<VectorStats, AppError> {
        self.stats(path).await
    }

    async fn upsert(
        &self,
        path: &Path,
        rows: Vec<VectorRow>,
        token: &CancellationToken,
    ) -> Result<usize, AppError> {
        if token.is_cancelled() {
            return Err(AppError::new(
                AppErrorCode::RunCancelled,
                "Операция отменена",
            ));
        }
        self.with_table(path, |table| {
            if table.schema_version < 2 {
                return Err(AppError::new(
                    AppErrorCode::ValidationFailed,
                    "Vector index must be fully re-indexed",
                ));
            }
            let count = rows.len();
            table
                .rows
                .retain(|row| !rows.iter().any(|incoming| incoming.id == row.id));
            table.rows.extend(rows);
            Ok(count)
        })
    }

    async fn search(
        &self,
        path: &Path,
        _request: VectorSearchRequest,
    ) -> Result<Vec<VectorHit>, AppError> {
        self.with_table(path, |_| Ok(Vec::new()))
    }

    async fn chunks(
        &self,
        path: &Path,
        document_id: &str,
        indices: &[u32],
    ) -> Result<Vec<VectorChunk>, AppError> {
        self.with_table(path, |table| {
            Ok(table
                .rows
                .iter()
                .filter(|row| row.document_id == document_id && indices.contains(&row.chunk_index))
                .map(|row| VectorChunk {
                    id: row.id.clone(),
                    payload: row.payload.clone(),
                    document_id: row.document_id.clone(),
                    chunk_index: row.chunk_index,
                    path: row.path.clone(),
                    text: row.text.clone(),
                    section: row.section.clone(),
                    page: row.page,
                })
                .collect())
        })
    }

    async fn maintain(&self, path: &Path, ann_threshold: usize) -> Result<VectorStats, AppError> {
        let stats = self.stats(path).await?;
        lock(&self.maintained).push((path.to_path_buf(), ann_threshold));
        Ok(stats)
    }

    async fn delete_by_ids(&self, path: &Path, ids: &[String]) -> Result<(), AppError> {
        self.with_table(path, |table| {
            table.rows.retain(|row| !ids.contains(&row.id));
            Ok(())
        })
    }

    async fn delete_by_source(&self, path: &Path, document_id: &str) -> Result<(), AppError> {
        self.with_table(path, |table| {
            table.rows.retain(|row| row.document_id != document_id);
            Ok(())
        })
    }

    async fn stats(&self, path: &Path) -> Result<VectorStats, AppError> {
        self.with_table(path, |table| Ok(Self::stats_of(table)))
    }

    async fn remove_index(&self, path: &Path) -> Result<(), AppError> {
        lock(&self.tables).remove(path);
        Ok(())
    }
}

enum Once {
    RateLimited,
    Hold(oneshot::Receiver<()>),
}

type CallHook = Arc<dyn Fn(usize) + Send + Sync>;

#[derive(Default)]
struct EmbedState {
    calls: Vec<Vec<String>>,
    rate_limits: usize,
    once: VecDeque<Once>,
    hook: Option<CallHook>,
    holding: bool,
}

#[derive(Default)]
struct Embedder {
    state: Mutex<EmbedState>,
    held: Notify,
}

impl Embedder {
    fn calls(&self) -> Vec<Vec<String>> {
        lock(&self.state).calls.clone()
    }

    fn clear(&self) {
        lock(&self.state).calls.clear();
    }

    fn inputs(&self) -> Vec<String> {
        self.calls().into_iter().flatten().collect()
    }

    fn once(&self, behaviour: Once) {
        lock(&self.state).once.push_back(behaviour);
    }

    fn hold(&self) -> oneshot::Sender<()> {
        let (release, gate) = oneshot::channel();
        self.once(Once::Hold(gate));
        release
    }

    fn rate_limit(&self, times: usize) {
        lock(&self.state).rate_limits = times;
    }

    fn on_call(&self, hook: impl Fn(usize) + Send + Sync + 'static) {
        lock(&self.state).hook = Some(Arc::new(hook));
    }

    async fn when_holding(&self) {
        loop {
            let notified = self.held.notified();
            if lock(&self.state).holding {
                return;
            }
            notified.await;
        }
    }
}

fn limited() -> AppError {
    AppError::new(AppErrorCode::RateLimited, "slow down")
}

#[async_trait]
impl EmbeddingDriver for Embedder {
    fn capabilities(&self) -> AdapterCapabilities {
        FAKE_CAPABILITIES
    }

    async fn embed(
        &self,
        texts: &[String],
        _model: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        let (behaviour, hook, count) = {
            let mut state = lock(&self.state);
            state.calls.push(texts.to_vec());
            let behaviour = state.once.pop_front();
            if behaviour.is_none() && state.rate_limits > 0 {
                state.rate_limits -= 1;
                return Err(limited());
            }
            (behaviour, state.hook.clone(), state.calls.len())
        };
        if let Some(hook) = hook {
            hook(count);
        }
        match behaviour {
            Some(Once::RateLimited) => return Err(limited()),
            Some(Once::Hold(gate)) => {
                lock(&self.state).holding = true;
                self.held.notify_waiters();
                let outcome = tokio::select! {
                    _ = gate => Ok(()),
                    () = cancel.cancelled() => Err(AppError::new(AppErrorCode::RunCancelled, "Операция отменена")),
                };
                lock(&self.state).holding = false;
                outcome?;
            }
            None => {}
        }
        Ok(texts
            .iter()
            .map(|text| {
                #[allow(clippy::cast_precision_loss)]
                let length = text.encode_utf16().count() as f32;
                vec![length, 1.0, 0.0]
            })
            .collect())
    }

    fn dimensions(&self) -> Option<usize> {
        Some(3)
    }

    async fn list_models(
        &self,
        _cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        Ok(Vec::new())
    }
}

struct EmbeddingSource(Arc<Embedder>);

#[async_trait]
impl DriverSource for EmbeddingSource {
    async fn ephemeral_driver(&self, _row: &ProviderRow) -> Result<AiDriver, AppError> {
        Ok(AiDriver {
            text: None,
            embedding: Some(self.0.clone()),
            image: None,
        })
    }

    fn invalidate(&self, _provider_id: &str) {}
}

#[derive(Default)]
struct FakeSampler {
    watching: AtomicUsize,
    started: AtomicUsize,
    sample: Option<ResourceSampleDto>,
}

impl ResourceSampler for Arc<FakeSampler> {
    fn watch(&self, _interval: Duration, on_sample: service::SampleSink) -> SamplingGuard {
        self.watching.fetch_add(1, Ordering::SeqCst);
        self.started.fetch_add(1, Ordering::SeqCst);
        if let Some(sample) = &self.sample {
            on_sample(sample.clone());
        }
        let sampler = Arc::clone(self);
        SamplingGuard::new(move || {
            sampler.watching.fetch_sub(1, Ordering::SeqCst);
        })
    }
}

struct Harness {
    database: TemporaryDatabase,
    workspace: TemporaryDirectory,
    corpus: PathBuf,
    core: Arc<MemoryCore>,
    embedder: Arc<Embedder>,
    clock: Arc<FakeClock>,
    stores: VectorStoreService,
    sources: VectorSourceService,
    indexing: IndexingService,
    sleeps: Arc<Mutex<Vec<u64>>>,
    store_id: String,
}

impl Harness {
    async fn new() -> Self {
        Self::with(|_| {}).await
    }

    async fn with(tune: impl FnOnce(&mut IndexingServiceOptions)) -> Self {
        let database = temporary_database();
        let workspace = temporary_directory("studio-index-");
        let corpus = workspace.join("corpus");
        std::fs::create_dir_all(&corpus).expect("creates");
        let core = Arc::new(MemoryCore::default());
        let embedder = Arc::new(Embedder::default());
        let drivers: Arc<dyn DriverSource> = Arc::new(EmbeddingSource(embedder.clone()));
        let clock = Arc::new(FakeClock::new(1000));
        let provider = database
            .with_connection(|connection| Ok(seed_embedding_provider(connection)))
            .expect("seeds");
        let stores = VectorStoreService::new(VectorStoreServiceOptions::new(
            database.shared(),
            core.clone(),
            drivers.clone(),
            workspace.path().to_path_buf(),
            silent_logger(),
            clock.clone(),
        ));
        let sources = VectorSourceService::new(VectorSourceServiceOptions::new(
            database.shared(),
            stores.clone(),
            silent_logger(),
            clock.clone(),
        ));
        let sleeps = Arc::new(Mutex::new(Vec::new()));
        let mut options = IndexingServiceOptions::new(
            database.shared(),
            Arc::new(WordCore),
            drivers,
            stores.clone(),
            silent_logger(),
            clock.clone(),
        );
        options.batch_size = 2;
        options.concurrency = 1;
        options.backoff_ms = 10;
        options.sleep = {
            let sleeps = Arc::clone(&sleeps);
            Arc::new(move |delay: Duration, _cancel| {
                lock(&sleeps).push(u64::try_from(delay.as_millis()).expect("small"));
                async { Ok(()) }.boxed()
            })
        };
        tune(&mut options);
        let indexing = IndexingService::new(options);
        let store = stores
            .create(&CreateVectorStoreInput {
                dimension: Some(3),
                chunk_size: Some(4),
                chunk_overlap: Some(1),
                ..CreateVectorStoreInput::new(
                    "Knowledge",
                    ProviderId::parse(&provider).expect("an id"),
                    "embed-model",
                )
            })
            .await
            .expect("creates a store");
        Self {
            database,
            workspace,
            corpus,
            core,
            embedder,
            clock,
            stores,
            sources,
            indexing,
            sleeps,
            store_id: store.id.to_string(),
        }
    }

    fn write(&self, name: &str, text: &str) -> String {
        let path = self.corpus.join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("creates");
        std::fs::write(&path, text).expect("writes");
        path.to_string_lossy().into_owned()
    }

    fn path(&self, name: &str) -> String {
        self.corpus.join(name).to_string_lossy().into_owned()
    }

    fn add_folder(&self, include: &[&str], exclude: &[&str]) -> String {
        self.add_source(&self.corpus.clone(), include, exclude)
    }

    fn add_source(&self, path: &Path, include: &[&str], exclude: &[&str]) -> String {
        let owned = |values: &[&str]| values.iter().map(|value| (*value).to_owned()).collect();
        self.sources
            .add_source(&AddVectorSourceInput {
                kind: Some(VectorSourceKind::Folder),
                include: Some(owned(include)),
                exclude: Some(owned(exclude)),
                recursive: Some(true),
                ..AddVectorSourceInput::new(
                    zvs_contract::VectorStoreId::parse(&self.store_id).expect("an id"),
                    path.to_string_lossy(),
                )
            })
            .expect("adds a source")
            .id
    }

    fn input(&self, full: bool) -> VectorIndexInput {
        VectorIndexInput::new(
            zvs_contract::VectorStoreId::parse(&self.store_id).expect("an id"),
            full,
        )
    }

    async fn index(&self, full: bool) -> VectorIndexReportDto {
        self.index_with(full, IndexContext::default()).await
    }

    async fn index_with(&self, full: bool, context: IndexContext) -> VectorIndexReportDto {
        self.indexing
            .index(&self.input(full), context)
            .await
            .expect("indexes")
    }

    async fn index_error(&self, full: bool) -> AppError {
        self.indexing
            .index(&self.input(full), IndexContext::default())
            .await
            .expect_err("refuses")
    }

    fn documents(&self) -> Vec<VectorDocumentRow> {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .vector_documents
                    .list_by_store(&self.store_id)?)
            })
            .expect("lists")
    }

    fn document(&self, path: &str) -> VectorDocumentRow {
        self.documents()
            .into_iter()
            .find(|row| row.source_path == path)
            .expect("a document")
    }

    fn sorted_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .documents()
            .into_iter()
            .map(|row| row.source_path)
            .collect();
        paths.sort();
        paths
    }

    fn store_path(&self) -> PathBuf {
        self.stores.store_path(&self.store_id).expect("a path")
    }

    fn rows(&self) -> Vec<VectorRow> {
        self.core.rows(&self.store_path())
    }

    fn rows_for(&self, document_id: &str) -> Vec<VectorRow> {
        self.core.rows_for(&self.store_path(), document_id)
    }

    fn patch_store(&self, patch: &VectorStorePatch) {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .vector_stores
                    .update(&self.store_id, patch)?)
            })
            .expect("patches");
    }
}

fn sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

#[tokio::test]
async fn a_folder_is_walked_extracted_embedded_and_recorded_end_to_end() {
    let host = Harness::new().await;
    host.write("guide.md", "alpha beta gamma delta epsilon zeta eta theta");
    host.write("nested/notes.txt", "one two three four five");
    host.write("table.csv", "name,role\nana,support\nbo,design");
    host.add_folder(&[], &[]);

    let report = host.index(false).await;

    assert_eq!(report.indexed, 3);
    assert_eq!(report.unchanged, 0);
    assert!(report.chunks > 0);
    assert_eq!(
        host.sorted_paths(),
        sorted(vec![
            host.path("guide.md"),
            host.corpus
                .join("nested")
                .join("notes.txt")
                .to_string_lossy()
                .into_owned(),
            host.path("table.csv"),
        ])
    );
    assert_eq!(host.rows().len() as u64, report.chunks);
    let store = host.stores.require_store(&host.store_id).expect("a store");
    assert_eq!(store.last_indexed_at, Some(1000));
    assert!(!report.cancelled);
}

#[tokio::test]
async fn unchanged_files_are_skipped_on_a_re_index_and_re_embedded_when_forced() {
    let host = Harness::new().await;
    host.write("guide.md", "alpha beta gamma delta epsilon");
    host.add_folder(&[], &[]);
    host.index(false).await;
    assert!(!host.embedder.calls().is_empty());
    host.embedder.clear();

    let again = host.index(false).await;
    assert_eq!(again.unchanged, 1);
    assert_eq!(again.indexed, 0);
    assert!(host.embedder.calls().is_empty());

    let forced = host.index(true).await;
    assert_eq!(forced.indexed, 1);
    assert!(!host.embedder.calls().is_empty());
}

#[tokio::test]
async fn a_changed_file_replaces_exactly_its_own_vectors() {
    let host = Harness::new().await;
    host.write(
        "a.md",
        "alpha beta gamma delta epsilon zeta eta theta iota kappa",
    );
    host.write("b.md", "stable text that never changes here");
    host.add_folder(&[], &[]);
    host.index(false).await;
    let before = host.document(&host.path("a.md"));
    let untouched = host.document(&host.path("b.md"));
    let untouched_rows = json!(host.rows_for(&untouched.id));

    host.write("a.md", "short now");
    let report = host.index(false).await;
    assert_eq!(report.indexed, 1);
    assert_eq!(report.unchanged, 1);

    let changed = host.document(&host.path("a.md"));
    assert_eq!(changed.id, before.id);
    assert_eq!(changed.chunk_count, 1);
    assert_eq!(host.rows_for(&changed.id).len(), 1);
    assert_eq!(json!(host.rows_for(&untouched.id)), untouched_rows);
}

#[tokio::test]
async fn a_removed_file_loses_its_vectors_and_its_metadata() {
    let host = Harness::new().await;
    host.write("keep.md", "one two three four");
    let doomed = host.write("gone.md", "five six seven eight");
    host.add_folder(&[], &[]);
    host.index(false).await;
    let removed = host.document(&doomed);
    assert!(!host.rows_for(&removed.id).is_empty());

    std::fs::remove_file(&doomed).expect("removes");
    let report = host.index(false).await;

    assert_eq!(report.removed, 1);
    assert_eq!(host.sorted_paths(), [host.path("keep.md")]);
    assert!(host.rows_for(&removed.id).is_empty());
}

#[tokio::test]
async fn a_source_that_cannot_be_reached_keeps_its_documents_instead_of_losing_them() {
    let host = Harness::new().await;
    host.write("keep.md", "one two three four");
    let drive = host.workspace.join("drive");
    std::fs::create_dir_all(&drive).expect("creates");
    std::fs::write(drive.join("remote.md"), "five six seven eight").expect("writes");
    host.add_folder(&[], &[]);
    host.add_source(&drive, &[], &[]);
    host.index(false).await;
    let remote = host.document(&drive.join("remote.md").to_string_lossy());
    let remote_rows = json!(host.rows_for(&remote.id));
    assert!(!host.rows_for(&remote.id).is_empty());

    std::fs::remove_dir_all(&drive).expect("unplugs");
    let report = host.index(false).await;

    assert_eq!(report.removed, 0);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("Источник недоступен"))
    );
    assert!(host.sorted_paths().contains(&remote.source_path));
    assert_eq!(json!(host.rows_for(&remote.id)), remote_rows);
}

#[tokio::test]
async fn a_file_beyond_a_truncated_walk_is_not_forgotten() {
    let host = Harness::with(|options| {
        options.limits = WalkLimits {
            max_files: 1,
            ..WalkLimits::default()
        };
    })
    .await;
    host.write("a.md", "one two three");
    host.add_folder(&[], &[]);
    host.index(false).await;
    host.write("b.md", "four five six");

    let report = host.index(false).await;

    assert!(
        report
            .notes
            .contains(&"Достигнут предел в 1 файлов".to_owned())
    );
    assert_eq!(report.removed, 0);
    assert!(host.sorted_paths().contains(&host.path("a.md")));
}

fn assert_consistent(host: &Harness) {
    let documents = host.documents();
    for document in &documents {
        assert_eq!(
            host.rows_for(&document.id).len() as i64,
            document.chunk_count
        );
    }
    let orphans: Vec<VectorRow> = host
        .rows()
        .into_iter()
        .filter(|row| {
            !documents
                .iter()
                .any(|document| document.id == row.document_id)
        })
        .collect();
    assert!(orphans.is_empty());
}

#[tokio::test]
async fn cancellation_stops_at_a_batch_boundary_and_leaves_the_store_consistent() {
    let host = Harness::new().await;
    for index in 0..6 {
        host.write(
            &format!("file-{index}.md"),
            "alpha beta gamma delta epsilon",
        );
    }
    host.add_folder(&[], &[]);
    let cancel = CancellationToken::new();
    {
        let cancel = cancel.clone();
        host.embedder.on_call(move |seen| {
            if seen >= 2 {
                cancel.cancel();
            }
        });
    }

    let report = host
        .index_with(
            false,
            IndexContext {
                cancel,
                ..IndexContext::default()
            },
        )
        .await;

    assert!(report.cancelled);
    assert!(report.indexed < 6);
    assert_consistent(&host);
}

#[tokio::test]
async fn cancellation_mid_embed_records_nothing_for_the_file_in_flight() {
    let host = Harness::new().await;
    host.write("a.md", "alpha beta gamma delta epsilon zeta eta theta");
    host.add_folder(&[], &[]);
    let release = host.embedder.hold();
    let cancel = CancellationToken::new();
    let running = {
        let indexing = host.indexing.clone();
        let input = host.input(false);
        let context = IndexContext {
            cancel: cancel.clone(),
            ..IndexContext::default()
        };
        tokio::spawn(async move { indexing.index(&input, context).await })
    };
    host.embedder.when_holding().await;
    cancel.cancel();
    let report = running.await.expect("joins").expect("reports");
    drop(release);

    assert!(report.cancelled);
    assert_eq!(report.indexed, 0);
    assert_eq!(report.failed, 0);
    assert!(host.documents().is_empty());
    assert!(host.rows().is_empty());
    assert_eq!(
        report.notes.len(),
        0,
        "a cancelled file is neither failed nor noted"
    );
}

#[tokio::test]
async fn a_cancelled_run_resumes_where_it_stopped_on_the_next_run() {
    let host = Harness::new().await;
    for index in 0..6 {
        host.write(
            &format!("file-{index}.md"),
            "alpha beta gamma delta epsilon",
        );
    }
    host.add_folder(&[], &[]);
    let cancel = CancellationToken::new();
    {
        let cancel = cancel.clone();
        host.embedder.on_call(move |seen| {
            if seen >= 5 {
                cancel.cancel();
            }
        });
    }
    let first = host
        .index_with(
            false,
            IndexContext {
                cancel,
                ..IndexContext::default()
            },
        )
        .await;
    assert!(first.cancelled);
    assert!(first.indexed > 0 && first.indexed < 6);
    host.embedder.on_call(|_| {});

    let second = host.index(false).await;

    assert!(!second.cancelled);
    assert_eq!(second.unchanged, first.indexed);
    assert_eq!(second.indexed, 6 - first.indexed);
    assert_eq!(host.documents().len(), 6);
    assert_consistent(&host);
}

#[tokio::test]
async fn a_rate_limit_is_met_with_backoff_rather_than_failure() {
    let host = Harness::new().await;
    host.write("guide.md", "alpha beta gamma delta epsilon zeta");
    host.add_folder(&[], &[]);
    host.embedder.rate_limit(2);

    let report = host.index(false).await;

    assert_eq!(report.failed, 0);
    assert_eq!(report.indexed, 1);
    assert_eq!(*lock(&host.sleeps), [10, 20]);
}

#[tokio::test]
async fn a_rate_limit_that_outlives_the_attempt_budget_fails_only_that_file() {
    let host = Harness::with(|options| {
        options.max_attempts = 2;
        options.backoff_ms = 1;
        options.batch_size = 32;
    })
    .await;
    host.write("guide.md", "alpha beta gamma");
    host.write("other.md", "delta epsilon zeta");
    host.add_folder(&[], &[]);
    host.embedder.once(Once::RateLimited);
    host.embedder.once(Once::RateLimited);

    let report = host.index(false).await;

    assert_eq!(report.failed, 1);
    assert_eq!(report.indexed, 1);
    assert!(report.notes.iter().any(|note| note.contains("Ошибка")));
}

#[tokio::test]
async fn unsupported_formats_are_skipped_with_a_reason_and_never_fail_the_run() {
    let host = Harness::new().await;
    host.write("manual.pdf", "%PDF-1.7");
    host.write("readme.md", "alpha beta gamma");
    host.add_folder(&[], &[]);

    let report = host.index(false).await;

    assert_eq!(report.indexed, 1);
    assert_eq!(report.skipped, 1);
    assert_eq!(report.failed, 0);
    assert!(report.notes.join(" ").contains(".pdf"));
}

#[tokio::test]
async fn a_document_that_reads_as_nothing_is_skipped_rather_than_recorded_empty() {
    let host = Harness::new().await;
    host.write("blank.md", "  \n\n ");
    host.add_folder(&[], &[]);

    let report = host.index(false).await;

    assert_eq!(report.skipped, 1);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.ends_with("не удалось извлечь текст"))
    );
    assert!(host.documents().is_empty());
}

#[tokio::test]
async fn include_and_exclude_patterns_choose_what_is_walked() {
    let host = Harness::new().await;
    host.write("docs/guide.md", "alpha beta gamma");
    host.write("docs/skip.txt", "delta epsilon");
    host.write("node_modules/dep/index.md", "should not appear");
    host.add_folder(&["**/*.md"], &["node_modules"]);

    let report = host.index(false).await;

    assert_eq!(report.indexed, 1);
    assert_eq!(
        host.sorted_paths(),
        [host
            .corpus
            .join("docs")
            .join("guide.md")
            .to_string_lossy()
            .into_owned()]
    );
}

#[tokio::test]
async fn progress_counts_embedded_chunks_against_an_estimate_and_ends_on_the_real_total() {
    let host = Harness::new().await;
    host.write("guide.md", "alpha beta gamma delta epsilon zeta eta theta");
    host.add_folder(&[], &[]);
    let progress = Arc::new(Mutex::new(Vec::<IndexProgress>::new()));
    let sink = Arc::clone(&progress);

    let report = host
        .index_with(
            false,
            IndexContext {
                on_progress: Some(Arc::new(move |update| lock(&sink).push(update))),
                ..IndexContext::default()
            },
        )
        .await;

    let progress = lock(&progress).clone();
    assert!(progress.len() > 1);
    let last = progress.last().expect("a last update");
    assert_eq!((last.done, last.total), (report.chunks, report.chunks));
    assert!(progress.iter().all(|update| update.done <= update.total));
    assert_eq!(progress[0].message.as_deref(), Some("Найдено файлов: 1"));
    assert_eq!(
        last.message.as_deref(),
        Some("Готово: 1 проиндексировано, 0 без изменений")
    );
}

#[tokio::test]
async fn a_document_can_be_removed_by_hand_and_takes_its_vectors_with_it() {
    let host = Harness::new().await;
    host.write("guide.md", "alpha beta gamma delta");
    host.add_folder(&[], &[]);
    host.index(false).await;
    let document = host.documents().remove(0);

    host.sources
        .remove_document(&host.store_id, &document.id)
        .await
        .expect("removes");

    assert!(host.documents().is_empty());
    assert!(host.rows().is_empty());
    let error = host
        .sources
        .remove_document(&host.store_id, &document.id)
        .await
        .expect_err("gone");
    assert_eq!(error.code, AppErrorCode::NotFound);
}

#[tokio::test]
async fn sources_are_persisted_deduplicated_by_path_and_removable() {
    let host = Harness::new().await;
    let first = host.add_folder(&["**/*.md"], &[]);
    let again = host.add_folder(&["**/*.txt"], &[]);
    assert_eq!(again, first);
    let listed = host.sources.list_sources(&host.store_id).expect("lists");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].include, ["**/*.txt"]);

    host.sources.remove_source(&first).expect("removes");

    assert!(
        host.sources
            .list_sources(&host.store_id)
            .expect("lists")
            .is_empty()
    );
    let error = host.index_error(false).await;
    assert_eq!(error.code, AppErrorCode::Conflict);
    assert_eq!(error.message, service::NO_SOURCES);
}

#[tokio::test]
async fn two_indexing_runs_of_one_store_do_not_overlap() {
    let host = Harness::new().await;
    host.write("guide.md", "alpha beta gamma delta");
    host.add_folder(&[], &[]);
    let release = host.embedder.hold();
    let running = {
        let indexing = host.indexing.clone();
        let input = host.input(false);
        tokio::spawn(async move { indexing.index(&input, IndexContext::default()).await })
    };
    host.embedder.when_holding().await;

    let error = host.index_error(false).await;
    assert_eq!(error.code, AppErrorCode::Conflict);
    assert_eq!(error.message, service::ALREADY_RUNNING);

    release.send(()).expect("releases");
    let report = running.await.expect("joins").expect("reports");
    assert_eq!(report.indexed, 1);
    host.index(false).await;
}

#[tokio::test]
async fn rows_carry_text_section_and_page_as_columns_and_the_document_prefix_reaches_the_embedder()
{
    let host = Harness::new().await;
    host.patch_store(&VectorStorePatch {
        document_prefix: Some("passage: ".to_owned()),
        ..VectorStorePatch::default()
    });
    host.write(
        "handbook.md",
        "# Отпуск\nalpha beta gamma\n## Порядок\ndelta epsilon zeta",
    );
    host.add_folder(&[], &[]);

    host.index(false).await;

    let mut rows = host.rows();
    rows.sort_by_key(|row| row.chunk_index);
    assert_eq!(
        rows.iter()
            .map(|row| row.section.as_str())
            .collect::<Vec<_>>(),
        ["Отпуск", "Отпуск › Порядок"]
    );
    assert_eq!(rows[0].text, "Отпуск alpha beta gamma");
    assert_eq!(rows[0].page, None);
    assert_eq!(
        rows[0].payload,
        json!({ "chunkCount": 2, "byteStart": 0, "byteEnd": 4, "tokenCount": 4 })
    );
    assert_eq!(rows[1].id, format!("{}:1", rows[1].document_id));
    let inputs = host.embedder.inputs();
    assert!(inputs.contains(&"passage: handbook › Отпуск\nОтпуск alpha beta gamma".to_owned()));
    assert!(
        inputs.contains(
            &"passage: handbook › Отпуск › Порядок\nПорядок delta epsilon zeta".to_owned()
        )
    );
}

#[tokio::test]
async fn pages_from_an_extractor_land_in_the_page_column() {
    let host = Harness::new().await;
    host.indexing.registry().register(
        &[".deck"],
        Arc::new(FnExtractor(|_: &ExtractionInput| {
            Ok(vec![
                Segment::new("first slide words", &["Intro"], Some(1)),
                Segment::new("second slide words", &[], Some(2)),
            ])
        })),
    );
    host.write("talk.deck", "binary");
    host.add_folder(&[], &[]);

    host.index(false).await;

    let mut rows = host.rows();
    rows.sort_by_key(|row| row.chunk_index);
    assert_eq!(
        rows.iter()
            .map(|row| (row.section.clone(), row.page))
            .collect::<Vec<_>>(),
        [("Intro".to_owned(), Some(1)), (String::new(), Some(2))]
    );
}

#[tokio::test]
async fn the_search_indexes_are_refreshed_once_a_run_changed_something() {
    let host = Harness::new().await;
    host.write("a.md", "one two three");
    host.add_folder(&[], &[]);
    host.index(false).await;
    assert_eq!(
        host.core.maintained(),
        [(host.store_path(), DEFAULT_ANN_THRESHOLD)]
    );

    host.index(false).await;

    assert_eq!(host.core.maintained().len(), 1);
}

#[tokio::test]
async fn a_v1_store_refuses_an_incremental_run_and_is_rebuilt_as_v2_by_a_full_one() {
    let host = Harness::new().await;
    host.write("a.md", "one two three");
    host.write("b.md", "four five six");
    host.add_folder(&[], &[]);
    host.index(false).await;
    let before = sorted(host.documents().into_iter().map(|row| row.id).collect());
    host.core.set_schema_version(&host.store_path(), 1);
    host.stores
        .reconcile(&host.store_id)
        .await
        .expect("reconciles");
    assert_eq!(
        host.stores
            .get(&host.store_id)
            .await
            .expect("a store")
            .schema_version,
        1
    );

    let error = host.index_error(false).await;
    assert_eq!(error.code, AppErrorCode::Conflict);
    assert_eq!(error.message, service::OLD_SCHEMA);

    let report = host.index(true).await;
    assert_eq!(report.indexed, 2);
    assert_eq!(
        host.stores
            .get(&host.store_id)
            .await
            .expect("a store")
            .schema_version,
        2
    );
    assert_eq!(
        sorted(host.documents().into_iter().map(|row| row.id).collect()),
        before
    );
    assert_eq!(host.rows().len() as u64, report.chunks);
    assert!(host.rows().iter().all(|row| !row.text.is_empty()));
}

#[tokio::test]
async fn an_embedding_of_the_wrong_size_fails_the_file_and_not_the_run() {
    let host = Harness::with(|options| options.batch_size = 32).await;
    host.database
        .with_connection(|connection| {
            connection.execute(
                "UPDATE vector_store SET dimension = 4 WHERE id = ?1",
                [&host.store_id],
            )?;
            Ok(())
        })
        .expect("patches");
    host.write("a.md", "one two three");
    host.add_folder(&[], &[]);

    let report = host.index(false).await;

    assert_eq!(report.failed, 1);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains(service::DIMENSION_MISMATCH))
    );
    assert!(host.documents().is_empty());
}

#[tokio::test]
async fn the_vector_index_channel_runs_the_job_through_the_kernel_and_reports_progress() {
    let host = Harness::new().await;
    host.write("guide.md", "alpha beta gamma delta epsilon zeta eta theta");
    host.add_folder(&[], &[]);
    let nodes = Arc::new(NodeRegistry::new());
    nodes.register(VectorIndexNode).expect("registers");
    let runs = RunService::new(RunServiceOptions {
        services: KernelServices {
            indexing: Some(Arc::new(host.indexing.clone())),
            ..KernelServices::default()
        },
        clock: host.clock.clone(),
        ..RunServiceOptions::new(host.database.shared(), EventBus::default(), nodes)
    })
    .expect("a run service");

    let handle = start_index_run(&host.stores, &runs, &host.input(false)).expect("starts");
    let seen = Arc::new(Mutex::new(Vec::<HostEventDraft>::new()));
    let subscription = {
        let seen = Arc::clone(&seen);
        runs.subscribe(
            &handle.id.to_string(),
            Arc::new(move |event: &HostEvent| lock(&seen).push(event.body.clone())),
        )
        .expect("subscribes")
    };
    runs.wait(&handle.id).await;
    subscription.unsubscribe();

    let run = runs.get(&handle.id).expect("a run");
    assert_eq!(run.status, RunStatus::Succeeded);
    assert_eq!(run.kind, RunKind::Indexing);
    let seen = lock(&seen).clone();
    assert!(
        seen.iter()
            .any(|event| matches!(event, HostEventDraft::Progress { done, .. } if *done > 0))
    );
    assert!(seen.iter().any(|event| matches!(
        event,
        HostEventDraft::Step { step } if step.get("domain") == Some(&json!("document"))
            && step.get("sourcePath") == Some(&json!(host.path("guide.md")))
    )));
    assert!(seen.iter().any(|event| matches!(
        event,
        HostEventDraft::Log { line } if line.get("storeId") == Some(&json!(host.store_id))
    )));
    assert_eq!(host.documents().len(), 1);
    let steps = runs.steps(&handle.id).expect("steps");
    let output = steps[0].output.clone().expect("an output");
    assert_eq!(output["indexed"], json!(1));
    assert_eq!(output["cancelled"], json!(false));
    runs.dispose().await;
}

#[tokio::test]
async fn cancelling_the_run_cancels_the_index_and_reports_it_as_cancelled() {
    let host = Harness::new().await;
    host.write("guide.md", "alpha beta gamma delta epsilon zeta eta theta");
    host.add_folder(&[], &[]);
    let release = host.embedder.hold();
    let nodes = Arc::new(NodeRegistry::new());
    nodes.register(VectorIndexNode).expect("registers");
    let runs = RunService::new(RunServiceOptions {
        services: KernelServices {
            indexing: Some(Arc::new(host.indexing.clone())),
            ..KernelServices::default()
        },
        clock: host.clock.clone(),
        ..RunServiceOptions::new(host.database.shared(), EventBus::default(), nodes)
    })
    .expect("a run service");

    let handle = start_index_run(&host.stores, &runs, &host.input(false)).expect("starts");
    host.embedder.when_holding().await;
    runs.cancel(&handle.id).await.expect("cancels");
    runs.wait(&handle.id).await;
    drop(release);

    assert!(host.documents().is_empty());
    assert!(host.rows().is_empty());
    let id: &RunId = &handle.id;
    assert_eq!(runs.get(id).expect("a run").status, RunStatus::Cancelled);
    runs.dispose().await;
}

#[tokio::test]
async fn machine_load_is_sampled_while_the_pipeline_runs_and_stops_with_it() {
    let sampler = Arc::new(FakeSampler {
        sample: Some(ResourceSampleDto {
            at: 1000,
            cpu_percent: 41.0,
            memory_used_bytes: 8,
            memory_total_bytes: 16,
            process_memory_bytes: 4,
            gpu_name: Some("NVIDIA".to_owned()),
            gpu_percent: Some(77.0),
            vram_used_bytes: Some(2),
            vram_total_bytes: Some(8),
        }),
        ..FakeSampler::default()
    });
    let host = {
        let sampler = sampler.clone();
        Harness::with(move |options| options.resources = Some(Arc::new(sampler))).await
    };
    host.write("guide.md", "alpha beta gamma delta");
    host.add_folder(&[], &[]);
    let samples = Arc::new(Mutex::new(Vec::<f64>::new()));
    let sink = Arc::clone(&samples);

    host.index_with(
        false,
        IndexContext {
            on_sample: Some(Arc::new(move |sample: ResourceSampleDto| {
                lock(&sink).push(sample.gpu_percent.unwrap_or(-1.0));
            })),
            ..IndexContext::default()
        },
    )
    .await;

    assert_eq!(*lock(&samples), [77.0]);
    assert_eq!(sampler.watching.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn nothing_is_sampled_when_the_caller_does_not_ask_for_it() {
    let sampler = Arc::new(FakeSampler::default());
    let host = {
        let sampler = sampler.clone();
        Harness::with(move |options| options.resources = Some(Arc::new(sampler))).await
    };
    host.write("guide.md", "alpha beta");
    host.add_folder(&[], &[]);

    host.index(false).await;

    assert_eq!(sampler.started.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn each_document_is_reported_as_its_vectors_land_not_only_when_the_run_ends() {
    let host = Harness::new().await;
    host.write("first.md", "alpha beta gamma");
    host.write("second.md", "delta epsilon zeta");
    host.add_folder(&[], &[]);
    let seen = Arc::new(Mutex::new(Vec::<VectorDocumentDto>::new()));
    let sink = Arc::clone(&seen);

    let report = host
        .index_with(
            false,
            IndexContext {
                on_document: Some(Arc::new(move |document| lock(&sink).push(document))),
                ..IndexContext::default()
            },
        )
        .await;

    assert_eq!(report.indexed, 2);
    let seen = lock(&seen).clone();
    assert_eq!(seen.len(), 2);
    assert_eq!(
        sorted(
            seen.iter()
                .map(|document| document.source_path.clone())
                .collect()
        ),
        sorted(vec![host.path("first.md"), host.path("second.md")])
    );
    assert!(seen.iter().all(|document| document.chunk_count > 0));
}

#[tokio::test]
async fn a_file_that_is_skipped_or_unchanged_is_never_reported_as_a_document() {
    let host = Harness::new().await;
    host.write("guide.md", "alpha beta gamma");
    host.write("photo.png", "not text");
    host.add_folder(&[], &[]);
    host.index(false).await;
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&seen);

    let report = host
        .index_with(
            false,
            IndexContext {
                on_document: Some(Arc::new(move |document: VectorDocumentDto| {
                    lock(&sink).push(document.source_path);
                })),
                ..IndexContext::default()
            },
        )
        .await;

    assert_eq!(report.unchanged, 1);
    assert_eq!(report.skipped, 1);
    assert!(lock(&seen).is_empty());
}

#[tokio::test]
async fn a_reset_empties_the_store_but_keeps_it_its_sources_and_its_settings() {
    let host = Harness::new().await;
    host.write("guide.md", "alpha beta gamma");
    host.write("other.md", "delta epsilon zeta");
    let source = host.add_folder(&[], &[]);
    let indexed = host.index(false).await;
    assert_eq!(indexed.indexed, 2);
    assert_eq!(
        host.sources
            .list_documents(&host.store_id)
            .expect("lists")
            .len(),
        2
    );

    let cleared = host.stores.clear(&host.store_id).await.expect("clears");

    assert_eq!(cleared.id.to_string(), host.store_id);
    assert_eq!(cleared.documents, 0);
    assert_eq!(cleared.vectors, 0);
    assert_eq!(cleared.last_indexed_at, None);
    assert!(
        host.sources
            .list_documents(&host.store_id)
            .expect("lists")
            .is_empty()
    );
    assert_eq!(
        host.sources
            .list_sources(&host.store_id)
            .expect("lists")
            .into_iter()
            .map(|row| row.id)
            .collect::<Vec<_>>(),
        [source]
    );
    assert_eq!(cleared.chunk_size, 4);
    assert_eq!(cleared.dimension, 3);
    assert_eq!(
        host.core
            .stats(&host.store_path())
            .await
            .expect("stats")
            .row_count,
        0
    );
}

#[tokio::test]
async fn a_store_can_be_indexed_again_after_a_reset() {
    let host = Harness::new().await;
    host.write("guide.md", "alpha beta gamma");
    host.add_folder(&[], &[]);
    host.index(false).await;
    host.stores.clear(&host.store_id).await.expect("clears");

    let again = host.index(false).await;

    assert_eq!(again.indexed, 1);
    assert_eq!(again.unchanged, 0);
    assert_eq!(
        host.sources
            .list_documents(&host.store_id)
            .expect("lists")
            .len(),
        1
    );
}

#[tokio::test]
async fn a_missing_store_is_refused() {
    let host = Harness::new().await;
    let missing = host
        .indexing
        .index(
            &VectorIndexInput::new(
                zvs_contract::VectorStoreId::parse("0199aa11-1111-7111-8111-111111111111")
                    .expect("an id"),
                false,
            ),
            IndexContext::default(),
        )
        .await
        .expect_err("refuses");
    assert_eq!(missing.code, AppErrorCode::NotFound);
}

#[tokio::test]
async fn a_disabled_embedding_provider_is_refused_before_anything_is_walked() {
    let host = Harness::new().await;
    host.write("guide.md", "alpha beta gamma");
    host.add_folder(&[], &[]);
    host.database
        .with_connection(|connection| {
            connection.execute("UPDATE provider SET enabled = 0", [])?;
            Ok(())
        })
        .expect("disables");

    let error = host.index_error(false).await;

    assert_eq!(error.code, AppErrorCode::Conflict);
    assert_eq!(error.message, service::PROVIDER_UNUSABLE);
    assert!(host.embedder.calls().is_empty());
}

#[tokio::test]
async fn embedding_batches_spread_over_workers_and_land_in_chunk_order() {
    let host = Harness::with(|options| {
        options.batch_size = 1;
        options.concurrency = 3;
    })
    .await;
    host.write(
        "long.md",
        "one two three four five six seven eight nine ten eleven twelve",
    );
    host.add_folder(&[], &[]);

    let report = host.index(false).await;

    let rows = host.rows();
    assert_eq!(rows.len() as u64, report.chunks);
    assert_eq!(host.embedder.calls().len(), rows.len());
    let indices: Vec<u32> = rows.iter().map(|row| row.chunk_index).collect();
    assert_eq!(
        indices,
        (0..u32::try_from(rows.len()).expect("small")).collect::<Vec<_>>()
    );
    let document: Value = json!(host.documents()[0].chunk_count);
    assert_eq!(document, json!(rows.len()));
}
