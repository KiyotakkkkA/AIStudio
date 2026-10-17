use std::collections::{HashMap, HashSet};
use std::path::{MAIN_SEPARATOR, PathBuf};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures::FutureExt;
use futures::future::{BoxFuture, try_join_all};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, ResourceSampleDto, VECTOR_SCHEMA_VERSION, Validate, VectorBackend,
    VectorDocumentDto, VectorIndexInput, VectorIndexReportDto, VectorStoreId,
};

use super::extraction::{
    ExtractionContext, ExtractionInput, ExtractorRegistry, NoteSink, create_extractor_registry,
    is_blank, js_basename, js_extname,
};
use super::walk::{DiscoveredFile, WalkLimits, walk_sources};
use super::{NOTES_MAX, cancelled};
use crate::data::repositories::{VectorDocumentDraft, VectorStorePatch};
use crate::data::schema::{VectorDocumentRow, VectorStoreRow};
use crate::data::{Database, Repositories};
use crate::drivers::ai::ports::SharedEmbeddingDriver;
use crate::drivers::core::{ChunkConfig, SectionChunk, SharedTextCore, TextSegment, VectorRow};
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger};
use crate::platform::long_path::extended;
use crate::platform::metrics::ResourceMonitor;
use crate::services::providers::DriverSource;
use crate::services::stored::stored_id;
use crate::services::vector_sources::{DEFAULT_ANN_THRESHOLD, describe, document_dto};
use crate::services::vector_stores::VectorStoreService;

pub const SECTION_SEPARATOR: &str = " › ";
pub const BYTES_PER_TOKEN: u64 = 6;
pub const ONLY_LANCEDB: &str = "Поддерживается только LanceDB";
pub const ALREADY_RUNNING: &str = "Индексация этого хранилища уже выполняется";
pub const NO_SOURCES: &str = "У хранилища нет источников для индексации";
pub const OLD_SCHEMA: &str =
    "Хранилище создано в старом формате индекса — запустите полную переиндексацию";
pub const PROVIDER_NOT_FOUND: &str = "Провайдер эмбеддингов не найден";
pub const PROVIDER_UNUSABLE: &str =
    "Провайдер эмбеддингов должен быть включён и поддерживать эмбеддинги";
pub const NO_EMBEDDINGS: &str = "Провайдер не поддерживает эмбеддинги";
pub const UNEXPECTED_VECTORS: &str = "Провайдер вернул неожиданное число векторов";
pub const DIMENSION_MISMATCH: &str = "Размерность эмбеддинга не совпадает с хранилищем";
pub const NOTHING_EXTRACTED: &str = "не удалось извлечь текст";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexProgress {
    pub done: u64,
    pub total: u64,
    pub message: Option<String>,
}

pub type ProgressSink = Arc<dyn Fn(IndexProgress) + Send + Sync>;
pub type SampleSink = Arc<dyn Fn(ResourceSampleDto) + Send + Sync>;
pub type DocumentSink = Arc<dyn Fn(VectorDocumentDto) + Send + Sync>;

#[derive(Clone, Default)]
pub struct IndexContext {
    pub cancel: CancellationToken,
    pub on_progress: Option<ProgressSink>,
    pub on_sample: Option<SampleSink>,
    pub on_document: Option<DocumentSink>,
}

pub struct SamplingGuard(Option<Box<dyn FnOnce() + Send>>);

impl SamplingGuard {
    pub fn new(stop: impl FnOnce() + Send + 'static) -> Self {
        Self(Some(Box::new(stop)))
    }
}

impl Drop for SamplingGuard {
    fn drop(&mut self) {
        if let Some(stop) = self.0.take() {
            stop();
        }
    }
}

pub trait ResourceSampler: Send + Sync {
    fn watch(&self, interval: Duration, on_sample: SampleSink) -> SamplingGuard;
}

pub type SharedResourceSampler = Arc<dyn ResourceSampler>;

impl ResourceSampler for Arc<ResourceMonitor> {
    fn watch(&self, interval: Duration, on_sample: SampleSink) -> SamplingGuard {
        let watch = ResourceMonitor::watch(self, interval, move |sample| on_sample(sample));
        SamplingGuard::new(move || watch.stop())
    }
}

pub type Sleeper = Arc<
    dyn Fn(Duration, CancellationToken) -> BoxFuture<'static, Result<(), AppError>> + Send + Sync,
>;
pub type FileReader =
    Arc<dyn Fn(String) -> BoxFuture<'static, Result<Vec<u8>, AppError>> + Send + Sync>;

pub fn default_sleep() -> Sleeper {
    Arc::new(|delay, cancel| {
        async move {
            if cancel.is_cancelled() {
                return Err(cancelled());
            }
            tokio::select! {
                () = cancel.cancelled() => Err(cancelled()),
                () = tokio::time::sleep(delay) => Ok(()),
            }
        }
        .boxed()
    })
}

pub fn default_reader() -> FileReader {
    Arc::new(|path| {
        async move {
            let target = extended(&PathBuf::from(path));
            tokio::task::spawn_blocking(move || std::fs::read(target))
                .await
                .map_err(|_| super::worker_failed())?
                .map_err(|error| AppError::new(AppErrorCode::NativeError, error.to_string()))
        }
        .boxed()
    })
}

pub struct IndexingServiceOptions {
    pub database: Arc<Database>,
    pub text: SharedTextCore,
    pub drivers: Arc<dyn DriverSource>,
    pub stores: VectorStoreService,
    pub extractors: ExtractorRegistry,
    pub limits: WalkLimits,
    pub logger: SharedLogger,
    pub clock: SharedClock,
    pub sleep: Sleeper,
    pub batch_size: usize,
    pub concurrency: usize,
    pub max_attempts: u32,
    pub backoff_ms: u64,
    pub progress_interval_ms: i64,
    pub resources: Option<SharedResourceSampler>,
    pub sample_interval: Duration,
    pub read_file: FileReader,
    pub ann_threshold: usize,
}

impl IndexingServiceOptions {
    pub fn new(
        database: Arc<Database>,
        text: SharedTextCore,
        drivers: Arc<dyn DriverSource>,
        stores: VectorStoreService,
        logger: SharedLogger,
        clock: SharedClock,
    ) -> Self {
        Self {
            database,
            text,
            drivers,
            stores,
            extractors: create_extractor_registry(),
            limits: WalkLimits::default(),
            logger,
            clock,
            sleep: default_sleep(),
            batch_size: 32,
            concurrency: 1,
            max_attempts: 5,
            backoff_ms: 500,
            progress_interval_ms: 250,
            resources: None,
            sample_interval: Duration::from_secs(1),
            read_file: default_reader(),
            ann_threshold: DEFAULT_ANN_THRESHOLD,
        }
    }
}

pub fn embedding_input(
    path: &str,
    section: &[String],
    text: &str,
    document_prefix: &str,
) -> String {
    let base = js_basename(path);
    let extension = js_extname(path);
    let title = match base.strip_suffix(extension.as_str()) {
        Some(stem) if !stem.is_empty() && !extension.is_empty() => stem.to_owned(),
        _ => base,
    };
    let header = std::iter::once(title.as_str())
        .chain(section.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(SECTION_SEPARATOR);
    format!("{document_prefix}{header}\n{text}")
}

pub fn estimate_chunks(bytes: u64, chunk_size: u64, chunk_overlap: u64) -> u64 {
    let tokens = bytes.div_ceil(BYTES_PER_TOKEN).max(1);
    let stride = chunk_size.saturating_sub(chunk_overlap).max(1);
    tokens.div_ceil(stride).max(1)
}

fn within(path: &str, root: &str) -> bool {
    let fold = |value: &str| {
        if cfg!(windows) {
            value.to_lowercase()
        } else {
            value.to_owned()
        }
    };
    let target = fold(path);
    let base = fold(root);
    if target == base {
        return true;
    }
    let prefix = if base.ends_with(MAIN_SEPARATOR) {
        base
    } else {
        format!("{base}{MAIN_SEPARATOR}")
    };
    target.starts_with(&prefix)
}

fn count(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

#[derive(Default)]
struct Tally {
    files: u64,
    indexed: u64,
    unchanged: u64,
    removed: u64,
    skipped: u64,
    failed: u64,
    chunks: u64,
    notes: Vec<String>,
}

impl Tally {
    fn note(&mut self, message: String) {
        if self.notes.len() < NOTES_MAX {
            self.notes.push(message);
        }
    }
}

#[derive(Default)]
struct RunState {
    tally: Tally,
    pending: i64,
    pending_for_file: i64,
}

type SharedState = Arc<Mutex<RunState>>;

fn lock(state: &SharedState) -> MutexGuard<'_, RunState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

enum Outcome {
    Indexed,
    Unchanged,
    Skipped(String),
}

struct Inner {
    options: IndexingServiceOptions,
    running: Mutex<HashSet<String>>,
    last_report_at: AtomicI64,
}

struct Running {
    inner: Arc<Inner>,
    id: String,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.inner
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
    }
}

#[derive(Clone)]
pub struct IndexingService {
    inner: Arc<Inner>,
}

impl IndexingService {
    pub fn new(options: IndexingServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                running: Mutex::new(HashSet::new()),
                last_report_at: AtomicI64::new(0),
            }),
        }
    }

    pub fn registry(&self) -> &ExtractorRegistry {
        &self.inner.options.extractors
    }

    fn options(&self) -> &IndexingServiceOptions {
        &self.inner.options
    }

    fn stores(&self) -> &VectorStoreService {
        &self.inner.options.stores
    }

    fn now(&self) -> i64 {
        self.inner.options.clock.now()
    }

    fn database<T>(
        &self,
        work: impl FnOnce(&Repositories<'_>) -> rusqlite::Result<T>,
    ) -> Result<T, AppError> {
        self.inner
            .options
            .database
            .with_connection(|connection| Ok(work(&Repositories::new(connection))?))
    }

    pub async fn index(
        &self,
        input: &VectorIndexInput,
        context: IndexContext,
    ) -> Result<VectorIndexReportDto, AppError> {
        input.validate()?;
        let store = self.stores().require_store(&input.store_id.to_string())?;
        if store.backend != VectorBackend::Lancedb {
            return Err(AppError::new(AppErrorCode::UnsupportedFormat, ONLY_LANCEDB));
        }
        let _running = {
            let mut running = self
                .inner
                .running
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if !running.insert(store.id.clone()) {
                return Err(AppError::new(AppErrorCode::Conflict, ALREADY_RUNNING));
            }
            Running {
                inner: Arc::clone(&self.inner),
                id: store.id.clone(),
            }
        };
        let _sampling = match (&context.on_sample, &self.options().resources) {
            (Some(on_sample), Some(resources)) => {
                Some(resources.watch(self.options().sample_interval, Arc::clone(on_sample)))
            }
            _ => None,
        };
        self.pipeline(store, input.full(), &context).await
    }

    fn report(
        &self,
        context: &IndexContext,
        state: &SharedState,
        message: Option<String>,
        force: bool,
    ) {
        let Some(on_progress) = &context.on_progress else {
            return;
        };
        let at = self.now();
        let last = self.inner.last_report_at.load(Ordering::SeqCst);
        if !force && at - last < self.options().progress_interval_ms {
            return;
        }
        self.inner.last_report_at.store(at, Ordering::SeqCst);
        let progress = {
            let state = lock(state);
            let done = state.tally.chunks;
            let projected = i64::try_from(done).unwrap_or(i64::MAX) + state.pending;
            IndexProgress {
                done,
                total: done.max(u64::try_from(projected).unwrap_or(0)),
                message,
            }
        };
        on_progress(progress);
    }

    fn note(state: &SharedState, message: String) {
        lock(state).tally.note(message);
    }

    async fn pipeline(
        &self,
        current: VectorStoreRow,
        full: bool,
        context: &IndexContext,
    ) -> Result<VectorIndexReportDto, AppError> {
        let cancel = &context.cancel;
        let sources =
            self.database(|repositories| repositories.vector_sources.list_by_store(&current.id))?;
        if sources.is_empty() {
            return Err(AppError::new(AppErrorCode::Conflict, NO_SOURCES));
        }
        let store = self.upgrade(current, full).await?;
        let embedding = self.embedding(&store, cancel).await?;
        let walked = walk_sources(sources, self.options().limits, cancel.clone()).await?;
        let chunk_size = count(store.chunk_size);
        let chunk_overlap = count(store.chunk_overlap);
        let pending: u64 = walked
            .files
            .iter()
            .map(|file| estimate_chunks(file.bytes, chunk_size, chunk_overlap))
            .sum();
        let state: SharedState = Arc::new(Mutex::new(RunState {
            tally: Tally {
                files: walked.files.len() as u64,
                notes: walked.notes.clone(),
                ..Tally::default()
            },
            pending: i64::try_from(pending).unwrap_or(i64::MAX),
            pending_for_file: 0,
        }));
        let known_rows =
            self.database(|repositories| repositories.vector_documents.list_by_store(&store.id))?;
        let known: HashMap<String, VectorDocumentRow> = known_rows
            .iter()
            .map(|row| (row.source_path.clone(), row.clone()))
            .collect();
        let discovered: HashSet<&str> =
            walked.files.iter().map(|file| file.path.as_str()).collect();
        self.report(
            context,
            &state,
            Some(format!("Найдено файлов: {}", walked.files.len())),
            true,
        );

        let mut was_cancelled = cancel.is_cancelled();
        let unseen = |path: &str| {
            walked.truncated || walked.unreachable.iter().any(|root| within(path, root))
        };
        for document in &known_rows {
            if discovered.contains(document.source_path.as_str()) || unseen(&document.source_path) {
                continue;
            }
            if was_cancelled {
                break;
            }
            match self.forget(&store, document).await {
                Ok(()) => lock(&state).tally.removed += 1,
                Err(error) => {
                    let mut state = lock(&state);
                    state.tally.failed += 1;
                    state.tally.note(format!(
                        "Не удалось удалить {}: {}",
                        document.source_path,
                        describe(&error)
                    ));
                }
            }
        }

        for file in &walked.files {
            if cancel.is_cancelled() {
                was_cancelled = true;
                break;
            }
            lock(&state).pending_for_file =
                i64::try_from(estimate_chunks(file.bytes, chunk_size, chunk_overlap)).unwrap_or(0);
            let outcome = self
                .ingest(
                    &store,
                    file,
                    known.get(&file.path),
                    full,
                    &embedding,
                    context,
                    &state,
                )
                .await;
            match outcome {
                Ok(outcome) => {
                    {
                        let mut state = lock(&state);
                        match outcome {
                            Outcome::Unchanged => state.tally.unchanged += 1,
                            Outcome::Skipped(reason) => {
                                state.tally.skipped += 1;
                                state
                                    .tally
                                    .note(format!("Пропущен {}: {reason}", file.path));
                            }
                            Outcome::Indexed => state.tally.indexed += 1,
                        }
                        state.pending -= state.pending_for_file;
                    }
                    self.report(context, &state, Some(file.path.clone()), true);
                }
                Err(error) => {
                    {
                        let mut state = lock(&state);
                        state.pending -= state.pending_for_file;
                    }
                    if error.code == AppErrorCode::RunCancelled || cancel.is_cancelled() {
                        was_cancelled = true;
                        break;
                    }
                    {
                        let mut state = lock(&state);
                        state.tally.failed += 1;
                        state
                            .tally
                            .note(format!("Ошибка {}: {}", file.path, describe(&error)));
                    }
                    self.options().logger.log(
                        LogLevel::Warn,
                        "indexing",
                        "Could not index a file",
                        &[
                            ("storeId", json!(store.id)),
                            ("path", json!(file.path)),
                            ("error", json!(describe(&error))),
                        ],
                    );
                    self.report(context, &state, Some(file.path.clone()), true);
                }
            }
        }

        let summary = {
            let mut state = lock(&state);
            state.pending = 0;
            if was_cancelled {
                format!(
                    "Отменено: обработано {} из {} файлов",
                    state.tally.indexed, state.tally.files
                )
            } else {
                format!(
                    "Готово: {} проиндексировано, {} без изменений",
                    state.tally.indexed, state.tally.unchanged
                )
            }
        };
        self.report(context, &state, Some(summary), true);

        let changed = {
            let state = lock(&state);
            state.tally.indexed > 0 || state.tally.removed > 0
        };
        if changed {
            let now = self.now();
            self.database(|repositories| {
                repositories.vector_stores.update(
                    &store.id,
                    &VectorStorePatch {
                        last_indexed_at: Some(Some(now)),
                        updated_at: Some(now),
                        ..VectorStorePatch::default()
                    },
                )
            })?;
            self.maintain(&store.id).await;
        }
        if let Err(error) = self.stores().reconcile(&store.id).await {
            self.options().logger.log(
                LogLevel::Warn,
                "indexing",
                "Could not reconcile after indexing",
                &[
                    ("storeId", json!(store.id)),
                    ("error", json!(error.to_string())),
                ],
            );
        }

        let tally = std::mem::take(&mut lock(&state).tally);
        Ok(VectorIndexReportDto {
            store_id: stored_id(VectorStoreId::parse, &store.id)?,
            files: tally.files,
            indexed: tally.indexed,
            unchanged: tally.unchanged,
            removed: tally.removed,
            skipped: tally.skipped,
            failed: tally.failed,
            chunks: tally.chunks,
            cancelled: was_cancelled,
            notes: tally.notes,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn ingest(
        &self,
        store: &VectorStoreRow,
        file: &DiscoveredFile,
        known: Option<&VectorDocumentRow>,
        full: bool,
        embedding: &SharedEmbeddingDriver,
        context: &IndexContext,
        state: &SharedState,
    ) -> Result<Outcome, AppError> {
        let extractors = self.registry();
        if !extractors.supports(&file.path) {
            return Ok(Outcome::Skipped(extractors.reason(&file.path)));
        }
        let bytes = (self.options().read_file)(file.path.clone()).await?;
        let size = bytes.len();
        let content_hash = self.options().text.hash(bytes.clone()).await?;
        if !full && known.is_some_and(|known| known.content_hash == content_hash) {
            return Ok(Outcome::Unchanged);
        }
        let note: NoteSink = {
            let state = Arc::clone(state);
            Arc::new(move |message| Self::note(&state, message))
        };
        let input = ExtractionInput {
            path: file.path.clone(),
            bytes,
            context: ExtractionContext {
                ocr: Some(store.ocr.clone()),
                cancel: Some(context.cancel.clone()),
                note: Some(note),
            },
        };
        let segments = match extractors.extract(&input).await {
            Ok(segments) => segments,
            Err(error) if error.code == AppErrorCode::UnsupportedFormat => {
                return Ok(Outcome::Skipped(error.message));
            }
            Err(error) => return Err(error),
        };
        drop(input);
        if segments.iter().all(|segment| is_blank(&segment.text)) {
            return Ok(Outcome::Skipped(NOTHING_EXTRACTED.to_owned()));
        }
        let chunks = self
            .options()
            .text
            .chunk_segments(
                segments
                    .into_iter()
                    .map(|segment| TextSegment {
                        text: segment.text,
                        section: segment.section,
                        page: segment.page,
                    })
                    .collect(),
                ChunkConfig {
                    size: usize::try_from(store.chunk_size).unwrap_or(0),
                    overlap: usize::try_from(store.chunk_overlap).unwrap_or(0),
                },
            )
            .await?;
        {
            let mut state = lock(state);
            let real = i64::try_from(chunks.len()).unwrap_or(i64::MAX);
            state.pending += real - state.pending_for_file;
            state.pending_for_file = real;
        }
        self.report(context, state, Some(file.path.clone()), true);
        let document_id = known.map_or_else(
            || new_id().hyphenated().to_string(),
            |known| known.id.clone(),
        );
        let on_batch = |embedded: usize| {
            lock(state).tally.chunks += embedded as u64;
            self.report(context, state, None, false);
        };
        let vectors = self
            .embed(
                store,
                embedding,
                &chunks,
                file,
                &document_id,
                &context.cancel,
                &on_batch,
            )
            .await?;
        let (row, _lease) = self.stores().lease(&store.id)?;
        let path = self.stores().store_path(&row.id)?;
        let core = self.stores().core();
        if known.is_some() {
            core.delete_by_source(&path, &document_id).await?;
        }
        let chunk_count = vectors.len();
        if !vectors.is_empty() {
            core.upsert(&path, vectors, &context.cancel).await?;
        }
        let now = self.now();
        let recorded = self.database(|repositories| {
            repositories
                .vector_documents
                .record_indexed(&VectorDocumentDraft {
                    id: document_id.clone(),
                    store_id: row.id.clone(),
                    source_path: file.path.clone(),
                    content_hash: content_hash.clone(),
                    chunk_count: i64::try_from(chunk_count).unwrap_or(i64::MAX),
                    bytes: i64::try_from(size).unwrap_or(i64::MAX),
                    indexed_at: now,
                })
        })?;
        if let Some(on_document) = &context.on_document {
            on_document(document_dto(&recorded)?);
        }
        Ok(Outcome::Indexed)
    }

    #[allow(clippy::too_many_arguments)]
    async fn embed(
        &self,
        store: &VectorStoreRow,
        embedding: &SharedEmbeddingDriver,
        chunks: &[SectionChunk],
        file: &DiscoveredFile,
        document_id: &str,
        cancel: &CancellationToken,
        on_batch: &(dyn Fn(usize) + Send + Sync),
    ) -> Result<Vec<VectorRow>, AppError> {
        let size = self.options().batch_size.max(1);
        let batches: Vec<(usize, &[SectionChunk])> = chunks
            .chunks(size)
            .enumerate()
            .map(|(index, batch)| (index * size, batch))
            .collect();
        let rows: Mutex<Vec<VectorRow>> = Mutex::new(Vec::new());
        let next = AtomicUsize::new(0);
        let concurrency = self.options().concurrency.min(batches.len()).max(1);
        let worker = || async {
            loop {
                if cancel.is_cancelled() {
                    return Err(cancelled());
                }
                let Some((start, batch)) = batches.get(next.fetch_add(1, Ordering::SeqCst)) else {
                    return Ok::<(), AppError>(());
                };
                let texts: Vec<String> = batch
                    .iter()
                    .map(|chunk| {
                        embedding_input(
                            &file.path,
                            &chunk.section,
                            &chunk.text,
                            &store.document_prefix,
                        )
                    })
                    .collect();
                let vectors = self.embed_batch(embedding, store, &texts, cancel).await?;
                let embedded = vectors.len();
                {
                    let mut rows = rows.lock().unwrap_or_else(PoisonError::into_inner);
                    for (offset, (vector, chunk)) in
                        vectors.into_iter().zip(batch.iter()).enumerate()
                    {
                        let index = start + offset;
                        rows.push(VectorRow {
                            id: format!("{document_id}:{index}"),
                            vector,
                            payload: json!({
                                "chunkCount": chunks.len(),
                                "byteStart": chunk.byte_start,
                                "byteEnd": chunk.byte_end,
                                "tokenCount": chunk.token_count,
                            }),
                            document_id: document_id.to_owned(),
                            chunk_index: u32::try_from(index).unwrap_or(u32::MAX),
                            path: file.path.clone(),
                            text: chunk.text.clone(),
                            section: chunk.section.join(SECTION_SEPARATOR),
                            page: chunk.page,
                        });
                    }
                }
                on_batch(embedded);
            }
        };
        try_join_all((0..concurrency).map(|_| worker())).await?;
        let mut rows = rows.into_inner().unwrap_or_else(PoisonError::into_inner);
        rows.sort_by_key(|row| row.chunk_index);
        Ok(rows)
    }

    async fn embed_batch(
        &self,
        embedding: &SharedEmbeddingDriver,
        store: &VectorStoreRow,
        texts: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        let attempts = self.options().max_attempts.max(1);
        let base = self.options().backoff_ms;
        let dimension = usize::try_from(store.dimension).unwrap_or(0);
        let mut attempt: u32 = 1;
        loop {
            if cancel.is_cancelled() {
                return Err(cancelled());
            }
            let outcome = embedding
                .embed(texts, &store.embedding_model_id, cancel)
                .await
                .and_then(|vectors| {
                    if vectors.len() != texts.len() {
                        return Err(AppError::new(
                            AppErrorCode::ValidationFailed,
                            UNEXPECTED_VECTORS,
                        ));
                    }
                    if vectors.iter().any(|vector| {
                        vector.len() != dimension || !vector.iter().all(|value| value.is_finite())
                    }) {
                        return Err(AppError::new(
                            AppErrorCode::ValidationFailed,
                            DIMENSION_MISMATCH,
                        ));
                    }
                    Ok(vectors)
                });
            match outcome {
                Ok(vectors) => return Ok(vectors),
                Err(error) => {
                    let limited = error.code == AppErrorCode::RateLimited;
                    if !limited || attempt >= attempts || cancel.is_cancelled() {
                        return Err(error);
                    }
                    let delay = base.saturating_mul(1_u64 << (attempt - 1).min(32));
                    self.options().logger.log(
                        LogLevel::Info,
                        "indexing",
                        "Backing off after a rate limit",
                        &[
                            ("storeId", json!(store.id)),
                            ("attempt", json!(attempt)),
                            ("delayMs", json!(delay)),
                        ],
                    );
                    (self.options().sleep)(Duration::from_millis(delay), cancel.clone()).await?;
                    attempt += 1;
                }
            }
        }
    }

    async fn upgrade(&self, store: VectorStoreRow, full: bool) -> Result<VectorStoreRow, AppError> {
        let path = self.stores().store_path(&store.id)?;
        let stats = self.stores().core().stats(&path).await?;
        if u64::from(stats.schema_version) >= VECTOR_SCHEMA_VERSION {
            return Ok(store);
        }
        if !full {
            return Err(AppError::new(AppErrorCode::Conflict, OLD_SCHEMA));
        }
        self.options().logger.log(
            LogLevel::Info,
            "indexing",
            "Rebuilding a v1 vector store as schema v2",
            &[
                ("storeId", json!(store.id)),
                ("from", json!(stats.schema_version)),
            ],
        );
        self.stores().recreate_table(&store.id).await?;
        self.stores().require_store(&store.id)
    }

    async fn maintain(&self, store_id: &str) {
        let outcome = async {
            let (row, _lease) = self.stores().lease(store_id)?;
            let path = self.stores().store_path(&row.id)?;
            self.stores()
                .core()
                .maintain(&path, self.options().ann_threshold)
                .await
        }
        .await;
        if let Err(error) = outcome {
            self.options().logger.log(
                LogLevel::Warn,
                "indexing",
                "Could not refresh the search indexes",
                &[
                    ("storeId", json!(store_id)),
                    ("error", json!(describe(&error))),
                ],
            );
        }
    }

    async fn forget(
        &self,
        store: &VectorStoreRow,
        document: &VectorDocumentRow,
    ) -> Result<(), AppError> {
        let (row, _lease) = self.stores().lease(&store.id)?;
        let path = self.stores().store_path(&row.id)?;
        self.stores()
            .core()
            .delete_by_source(&path, &document.id)
            .await?;
        self.database(|repositories| repositories.vector_documents.remove(&document.id))
    }

    async fn embedding(
        &self,
        store: &VectorStoreRow,
        cancel: &CancellationToken,
    ) -> Result<SharedEmbeddingDriver, AppError> {
        let provider = self
            .database(|repositories| {
                repositories
                    .providers
                    .find_by_id(&store.embedding_provider_id)
            })?
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, PROVIDER_NOT_FOUND))?;
        if !provider.enabled || !provider.cap_embedding {
            return Err(AppError::new(AppErrorCode::Conflict, PROVIDER_UNUSABLE));
        }
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let driver = self.options().drivers.ephemeral_driver(&provider).await?;
        driver
            .embedding
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, NO_EMBEDDINGS))
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn the_embedded_text_carries_the_title_and_section_path() {
        assert_eq!(
            embedding_input(
                "C:/docs/handbook.md",
                &["Отпуск".to_owned(), "Порядок".to_owned()],
                "Подайте заявление.",
                ""
            ),
            "handbook › Отпуск › Порядок\nПодайте заявление."
        );
        assert_eq!(
            embedding_input("/notes/readme", &[], "text", "passage: "),
            "passage: readme\ntext"
        );
        assert_eq!(embedding_input("/notes/.env", &[], "x", ""), ".env\nx");
    }

    #[test]
    fn the_chunk_estimate_never_drops_below_one() {
        assert_eq!(estimate_chunks(0, 4, 1), 1);
        assert_eq!(estimate_chunks(60, 4, 1), 4);
        assert_eq!(estimate_chunks(600, 256, 32), 1);
        assert_eq!(estimate_chunks(6000, 4, 4), 1000);
    }

    #[test]
    fn a_path_lies_within_its_root_but_not_a_sibling_with_the_same_prefix() {
        let root = format!("{MAIN_SEPARATOR}data{MAIN_SEPARATOR}drive");
        assert!(within(&root, &root));
        assert!(within(&format!("{root}{MAIN_SEPARATOR}a.md"), &root));
        assert!(!within(&format!("{root}2{MAIN_SEPARATOR}a.md"), &root));
        if cfg!(windows) {
            assert!(within(r"C:\Data\Drive\a.md", r"c:\data\drive"));
        }
    }
}
