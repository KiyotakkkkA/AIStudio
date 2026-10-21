#[cfg(test)]
pub(crate) mod tests;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant};

use serde_json::json;
use tokio_util::sync::CancellationToken;
use zvs_contract::validate::{js_slice, js_trim};
use zvs_contract::{
    AppError, AppErrorCode, CreateVectorStoreInput, ProviderId, UpdateVectorStoreInput,
    VECTOR_SCHEMA_VERSION, VectorBackend, VectorChunkDto, VectorDocumentDto, VectorHealth,
    VectorHitProvenance, VectorOcrConfig, VectorRerankConfig, VectorRerankOutcome,
    VectorSearchHitDto, VectorSearchInput, VectorSearchMode, VectorSearchResultDto, VectorStoreDto,
    VectorStoreId,
};

use super::providers::DriverSource;
use super::rerank::{MAX_RERANK_QUERY_CHARS, RerankerSource, apply_rerank, passage_of};
use super::stored::{stored_count, stored_id, unsigned};
use super::vector_health::{contract_metric, core_metric, derive_vector_health};
use crate::data::repositories::{VectorStoreDraft, VectorStorePatch};
use crate::data::schema::{ProviderRow, VectorStoreRow};
use crate::data::{Database, Repositories};
use crate::drivers::ai::errors::timed_out;
use crate::drivers::core::{
    SharedVectorCore, VectorChunk, VectorHit, VectorProvenance, VectorSearchMode as CoreMode,
    VectorSearchRequest, VectorStats, cancelled_error,
};
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger};
use crate::platform::paths::vector_store_path;

pub const DEFAULT_RERANK_TIMEOUT: Duration = Duration::from_secs(20);
pub const DEFAULT_EMBEDDING_TIMEOUT_SECONDS: f64 = 30.0;

pub const ONLY_LANCEDB: &str = "Поддерживается только LanceDB";
pub const STORE_NOT_FOUND: &str = "Векторное хранилище не найдено";
pub const STORE_BUSY: &str = "Векторное хранилище занято другой операцией";
pub const NAME_TAKEN: &str = "Хранилище с таким названием уже существует";
pub const PROVIDER_NOT_FOUND: &str = "Провайдер эмбеддингов не найден";
pub const PROVIDER_UNUSABLE: &str =
    "Провайдер эмбеддингов должен быть включён и поддерживать эмбеддинги";
pub const NO_EMBEDDINGS: &str = "Провайдер не поддерживает эмбеддинги";
pub const DIMENSION_MISMATCH: &str = "Размерность эмбеддинга не совпадает с размерностью хранилища";
pub const STORE_CHANGED: &str = "Хранилище изменилось во время поиска";
pub const OVERLAP_TOO_LARGE: &str = "Перекрытие фрагмента должно быть меньше его размера";
pub const SETTINGS_FROZEN: &str =
    "Параметры фрагментов и префиксов изменяются только до индексации";
pub const INDEX_FROZEN: &str = "Для этого индекса нельзя изменить параметры фрагментов";
pub const CREATED_MISMATCH: &str =
    "Конфигурация созданного индекса не совпадает с настройками хранилища";
pub const CREATION_CLEANUP_FAILED: &str =
    "Не удалось создать хранилище и очистить данные; метаданные сохранены для удаления";
pub const REMOVAL_FAILED: &str = "Не удалось удалить векторное хранилище; метаданные сохранены";
pub const CLEAR_FAILED: &str = "Не удалось очистить хранилище";
pub const RERANK_MODEL_REQUIRED: &str = "Выберите модель переранжирования";
pub const OCR_MODEL_REQUIRED: &str = "Выберите модель OCR";
pub const RERANK_MALFORMED: &str = "Модель переранжирования вернула некорректный ответ";
pub const RERANK_UNAVAILABLE: &str = "Reranker unavailable, using vector order";
pub const RERANK_FAILED: &str = "Reranking failed, using vector order";

const SCOPE: &str = "vectorStores";

pub type Stopwatch = Arc<dyn Fn() -> f64 + Send + Sync>;

pub fn monotonic_stopwatch() -> Stopwatch {
    let origin = Instant::now();
    Arc::new(move || origin.elapsed().as_secs_f64() * 1000.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VectorSearchOptions {
    pub k: Option<u64>,
    pub min_score: Option<f64>,
    pub rerank: bool,
    pub mode: Option<VectorSearchMode>,
}

impl Default for VectorSearchOptions {
    fn default() -> Self {
        Self {
            k: None,
            min_score: None,
            rerank: true,
            mode: None,
        }
    }
}

pub struct VectorStoreServiceOptions {
    pub database: Arc<Database>,
    pub core: SharedVectorCore,
    pub drivers: Arc<dyn DriverSource>,
    pub directory: PathBuf,
    pub logger: SharedLogger,
    pub clock: SharedClock,
    pub stopwatch: Stopwatch,
    pub rerankers: Option<RerankerSource>,
    pub rerank_timeout: Duration,
}

impl VectorStoreServiceOptions {
    pub fn new(
        database: Arc<Database>,
        core: SharedVectorCore,
        drivers: Arc<dyn DriverSource>,
        directory: PathBuf,
        logger: SharedLogger,
        clock: SharedClock,
    ) -> Self {
        Self {
            database,
            core,
            drivers,
            directory,
            logger,
            clock,
            stopwatch: monotonic_stopwatch(),
            rerankers: None,
            rerank_timeout: DEFAULT_RERANK_TIMEOUT,
        }
    }
}

struct Inner {
    database: Arc<Database>,
    core: SharedVectorCore,
    drivers: Arc<dyn DriverSource>,
    directory: PathBuf,
    logger: SharedLogger,
    clock: SharedClock,
    stopwatch: Stopwatch,
    rerankers: RwLock<Option<RerankerSource>>,
    rerank_timeout: Duration,
    busy: Mutex<HashSet<String>>,
}

#[derive(Clone)]
pub struct VectorStoreService {
    inner: Arc<Inner>,
}

pub struct StoreLease {
    inner: Arc<Inner>,
    id: String,
}

impl Drop for StoreLease {
    fn drop(&mut self) {
        self.inner
            .busy
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
    }
}

struct Observed {
    stats: Option<VectorStats>,
    health: VectorHealth,
}

pub fn effective_mode(
    search_mode: VectorSearchMode,
    schema_version: i64,
    requested: Option<VectorSearchMode>,
) -> VectorSearchMode {
    let mode = requested.unwrap_or(search_mode);
    let current = i64::try_from(VECTOR_SCHEMA_VERSION).unwrap_or(i64::MAX);
    if mode == VectorSearchMode::Hybrid && schema_version < current {
        VectorSearchMode::Vector
    } else {
        mode
    }
}

const fn core_mode(mode: VectorSearchMode) -> CoreMode {
    match mode {
        VectorSearchMode::Hybrid => CoreMode::Hybrid,
        VectorSearchMode::Vector => CoreMode::Vector,
    }
}

const fn provenance(provenance: VectorProvenance) -> VectorHitProvenance {
    match provenance {
        VectorProvenance::Vector => VectorHitProvenance::Vector,
        VectorProvenance::Keyword => VectorHitProvenance::Keyword,
        VectorProvenance::Both => VectorHitProvenance::Both,
    }
}

pub fn hit_dto(hit: VectorHit) -> VectorSearchHitDto {
    VectorSearchHitDto {
        id: hit.id,
        score: f64::from(hit.score),
        payload: hit.payload,
        document_id: hit.document_id,
        chunk_index: u64::from(hit.chunk_index),
        path: hit.path,
        text: hit.text,
        section: hit.section,
        page: hit.page.map(u64::from),
        vector_score: None,
        rerank_score: None,
        keyword_score: hit.keyword_score.map(f64::from),
        fused_score: hit.fused_score.map(f64::from),
        provenance: hit.provenance.map(provenance),
    }
}

fn chunk_dto(chunk: VectorChunk) -> VectorChunkDto {
    VectorChunkDto {
        id: chunk.id,
        payload: chunk.payload,
        document_id: chunk.document_id,
        chunk_index: u64::from(chunk.chunk_index),
        path: chunk.path,
        text: chunk.text,
        section: chunk.section,
        page: chunk.page.map(u64::from),
    }
}

pub fn store_dto(row: &VectorStoreRow) -> Result<VectorStoreDto, AppError> {
    Ok(VectorStoreDto {
        id: stored_id(VectorStoreId::parse, &row.id)?,
        name: row.name.clone(),
        description: row.description.clone(),
        backend: row.backend,
        embedding_provider_id: stored_id(ProviderId::parse, &row.embedding_provider_id)?,
        embedding_model_id: row.embedding_model_id.clone(),
        dimension: unsigned(row.dimension)?,
        metric: row.metric,
        chunk_size: unsigned(row.chunk_size)?,
        chunk_overlap: unsigned(row.chunk_overlap)?,
        rerank: row.rerank.clone(),
        ocr: row.ocr.clone(),
        query_prefix: row.query_prefix.clone(),
        document_prefix: row.document_prefix.clone(),
        search_mode: row.search_mode,
        chat_min_score: row.chat_min_score,
        schema_version: unsigned(row.schema_version)?,
        index_type: row.index_type.clone(),
        status: row.status,
        graph_stale: row.graph_stale,
        last_indexed_at: row.last_indexed_at,
        documents: unsigned(row.documents)?,
        vectors: unsigned(row.vectors)?,
        bytes: unsigned(row.bytes)?,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn trimmed_rerank(config: &VectorRerankConfig) -> VectorRerankConfig {
    VectorRerankConfig {
        model_ref: js_trim(&config.model_ref).to_owned(),
        ..config.clone()
    }
}

fn trimmed_ocr(config: &VectorOcrConfig) -> VectorOcrConfig {
    VectorOcrConfig {
        model_ref: js_trim(&config.model_ref).to_owned(),
        ..config.clone()
    }
}

fn require_advanced(rerank: &VectorRerankConfig, ocr: &VectorOcrConfig) -> Result<(), AppError> {
    if rerank.enabled && js_trim(&rerank.model_ref).is_empty() {
        return Err(AppError::new(
            AppErrorCode::ValidationFailed,
            RERANK_MODEL_REQUIRED,
        ));
    }
    if ocr.enabled && js_trim(&ocr.model_ref).is_empty() {
        return Err(AppError::new(
            AppErrorCode::ValidationFailed,
            OCR_MODEL_REQUIRED,
        ));
    }
    Ok(())
}

fn require_backend(row: &VectorStoreRow) -> Result<(), AppError> {
    if row.backend == VectorBackend::Lancedb {
        Ok(())
    } else {
        Err(AppError::new(AppErrorCode::UnsupportedFormat, ONLY_LANCEDB))
    }
}

fn integer(value: u64) -> Result<i64, AppError> {
    i64::try_from(value)
        .map_err(|_| AppError::new(AppErrorCode::ValidationFailed, "Значение слишком велико"))
}

fn native_error(message: &str, cause: &AppError) -> AppError {
    AppError::new(AppErrorCode::NativeError, message).with_detail("cause", cause.to_string())
}

impl VectorStoreService {
    pub fn new(options: VectorStoreServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                database: options.database,
                core: options.core,
                drivers: options.drivers,
                directory: options.directory,
                logger: options.logger,
                clock: options.clock,
                stopwatch: options.stopwatch,
                rerankers: RwLock::new(options.rerankers),
                rerank_timeout: options.rerank_timeout,
                busy: Mutex::new(HashSet::new()),
            }),
        }
    }

    pub fn attach_rerankers(&self, source: RerankerSource) {
        *self
            .inner
            .rerankers
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(source);
    }

    pub async fn create(&self, input: &CreateVectorStoreInput) -> Result<VectorStoreDto, AppError> {
        zvs_contract::Validate::validate(input)?;
        if input.backend() != VectorBackend::Lancedb {
            return Err(AppError::new(AppErrorCode::UnsupportedFormat, ONLY_LANCEDB));
        }
        let provider_id = input.embedding_provider_id.to_string();
        self.require_provider(&provider_id)?;
        let name = js_trim(&input.name).to_owned();
        self.require_name(&name, None)?;
        let rerank = trimmed_rerank(&input.rerank());
        let ocr = trimmed_ocr(&input.ocr());
        require_advanced(&rerank, &ocr)?;
        let now = self.now();
        let draft = VectorStoreDraft {
            id: new_id().hyphenated().to_string(),
            name,
            description: input.description().to_owned(),
            backend: input.backend(),
            embedding_provider_id: provider_id,
            embedding_model_id: js_trim(&input.embedding_model_id).to_owned(),
            dimension: integer(input.dimension())?,
            metric: input.metric(),
            chunk_size: integer(input.chunk_size())?,
            chunk_overlap: integer(input.chunk_overlap())?,
            rerank,
            ocr,
            query_prefix: input.query_prefix().to_owned(),
            document_prefix: input.document_prefix().to_owned(),
            search_mode: input.search_mode(),
            chat_min_score: input.chat_min_score(),
            index_type: input.index_type().to_owned(),
            created_at: now,
            updated_at: now,
        };
        let row = self.inner.database.with_connection(|connection| {
            Ok(Repositories::new(connection).vector_stores.create(&draft)?)
        })?;
        let _lease = self.exclusive(&row.id)?;
        let path = self.path(&row.id)?;
        match self.build_index(&row, &path).await {
            Ok(dto) => Ok(dto),
            Err(cause) => {
                if let Err(cleanup) = self.inner.core.remove_index(&path).await {
                    self.patch_row(
                        &row.id,
                        &VectorStorePatch::status(VectorHealth::Broken, self.now()),
                    )?;
                    self.inner.logger.log(
                        LogLevel::Error,
                        SCOPE,
                        "Could not clean up failed creation",
                        &[
                            ("storeId", json!(row.id)),
                            ("error", json!(cleanup.to_string())),
                        ],
                    );
                    return Err(native_error(CREATION_CLEANUP_FAILED, &cause));
                }
                self.inner.database.with_connection(|connection| {
                    Ok(Repositories::new(connection)
                        .vector_stores
                        .remove(&row.id)?)
                })?;
                Err(cause)
            }
        }
    }

    async fn build_index(
        &self,
        row: &VectorStoreRow,
        path: &Path,
    ) -> Result<VectorStoreDto, AppError> {
        let dimension = u32::try_from(row.dimension)
            .map_err(|_| AppError::new(AppErrorCode::ValidationFailed, DIMENSION_MISMATCH))?;
        let stats = self
            .inner
            .core
            .create_index(path, dimension, core_metric(row.metric))
            .await?;
        if stats.dimension != dimension || contract_metric(stats.metric) != row.metric {
            return Err(AppError::new(AppErrorCode::NativeError, CREATED_MISMATCH));
        }
        self.repair(row, &stats)
    }

    pub async fn list(&self) -> Result<Vec<VectorStoreDto>, AppError> {
        let rows = self.inner.database.with_connection(|connection| {
            Ok(Repositories::new(connection).vector_stores.list()?)
        })?;
        let observed = futures::future::join_all(rows.iter().map(|row| async move {
            if self.is_busy(&row.id) {
                None
            } else {
                Some(self.observe(row).await.health)
            }
        }))
        .await;
        rows.into_iter()
            .zip(observed)
            .map(|(row, health)| {
                store_dto(&VectorStoreRow {
                    status: health.unwrap_or(row.status),
                    ..row
                })
            })
            .collect()
    }

    pub async fn get(&self, id: &str) -> Result<VectorStoreDto, AppError> {
        self.reconcile(id).await
    }

    pub async fn update(&self, input: &UpdateVectorStoreInput) -> Result<VectorStoreDto, AppError> {
        zvs_contract::Validate::validate(input)?;
        let id = input.id.to_string();
        let _lease = self.exclusive(&id)?;
        let row = self.require(&id)?;
        let chunk_size = input.chunk_size.map(integer).transpose()?;
        let chunk_overlap = input.chunk_overlap.map(integer).transpose()?;
        if chunk_overlap.unwrap_or(row.chunk_overlap) >= chunk_size.unwrap_or(row.chunk_size) {
            return Err(AppError::new(
                AppErrorCode::ValidationFailed,
                OVERLAP_TOO_LARGE,
            ));
        }
        let changes_index = chunk_size.is_some_and(|size| size != row.chunk_size)
            || chunk_overlap.is_some_and(|overlap| overlap != row.chunk_overlap)
            || input
                .query_prefix
                .as_ref()
                .is_some_and(|prefix| *prefix != row.query_prefix)
            || input
                .document_prefix
                .as_ref()
                .is_some_and(|prefix| *prefix != row.document_prefix);
        if changes_index {
            let documents = self.inner.database.with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .vector_documents
                    .list_by_store(&id)?
                    .len())
            })?;
            if row.vectors > 0 || row.last_indexed_at.is_some() || documents > 0 {
                return Err(AppError::new(AppErrorCode::Conflict, SETTINGS_FROZEN));
            }
            let observed = self.observe(&row).await;
            if observed.health == VectorHealth::Broken
                || observed
                    .stats
                    .as_ref()
                    .is_some_and(|stats| stats.row_count > 0)
            {
                return Err(AppError::new(AppErrorCode::Conflict, INDEX_FROZEN));
            }
        }
        let name = input.name.as_deref().map(|name| js_trim(name).to_owned());
        if let Some(name) = &name {
            self.require_name(name, Some(&id))?;
        }
        let rerank = input.rerank.as_ref().map(trimmed_rerank);
        let ocr = input.ocr.as_ref().map(trimmed_ocr);
        require_advanced(
            rerank.as_ref().unwrap_or(&row.rerank),
            ocr.as_ref().unwrap_or(&row.ocr),
        )?;
        let updated = self.patch_row(
            &id,
            &VectorStorePatch {
                name,
                description: input.description.clone(),
                chunk_size,
                chunk_overlap,
                rerank,
                ocr,
                query_prefix: input.query_prefix.clone(),
                document_prefix: input.document_prefix.clone(),
                search_mode: input.search_mode,
                chat_min_score: input.chat_min_score,
                updated_at: Some(self.now()),
                ..VectorStorePatch::default()
            },
        )?;
        let observed = self.observe(&updated).await;
        store_dto(&VectorStoreRow {
            status: observed.health,
            ..updated
        })
    }

    pub async fn remove(&self, id: &str) -> Result<(), AppError> {
        let _lease = self.exclusive(id)?;
        let row = self.require(id)?;
        require_backend(&row)?;
        if let Err(cause) = self.inner.core.remove_index(&self.path(id)?).await {
            self.inner.logger.log(
                LogLevel::Error,
                SCOPE,
                "Could not remove vector store",
                &[("storeId", json!(id)), ("error", json!(cause.to_string()))],
            );
            return Err(native_error(REMOVAL_FAILED, &cause));
        }
        self.inner.database.with_connection(|connection| {
            Ok(Repositories::new(connection).vector_stores.remove(id)?)
        })
    }

    pub async fn clear(&self, id: &str) -> Result<VectorStoreDto, AppError> {
        let _lease = self.exclusive(id)?;
        let row = self.require(id)?;
        require_backend(&row)?;
        self.replace_table(&row).await?;
        let now = self.now();
        let (removed, updated) = self.inner.database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            let removed = repositories.vector_documents.remove_by_store(id)?;
            let updated = repositories.vector_stores.update(
                id,
                &VectorStorePatch {
                    documents: Some(0),
                    vectors: Some(0),
                    bytes: Some(0),
                    last_indexed_at: Some(None),
                    status: Some(VectorHealth::Pending),
                    updated_at: Some(now),
                    ..VectorStorePatch::default()
                },
            )?;
            Ok((removed, updated))
        })?;
        self.inner.logger.log(
            LogLevel::Info,
            SCOPE,
            "Reset a vector store",
            &[("storeId", json!(id)), ("documents", json!(removed))],
        );
        store_dto(&updated.ok_or_else(|| AppError::new(AppErrorCode::NotFound, STORE_NOT_FOUND))?)
    }

    pub async fn recreate_table(&self, id: &str) -> Result<VectorStoreDto, AppError> {
        let _lease = self.exclusive(id)?;
        let row = self.require(id)?;
        require_backend(&row)?;
        let stats = self.replace_table(&row).await?;
        self.inner.logger.log(
            LogLevel::Info,
            SCOPE,
            "Recreated the vector table",
            &[
                ("storeId", json!(id)),
                ("schemaVersion", json!(stats.schema_version)),
            ],
        );
        self.repair(&row, &stats)
    }

    async fn replace_table(&self, row: &VectorStoreRow) -> Result<VectorStats, AppError> {
        let path = self.path(&row.id)?;
        let dimension = u32::try_from(row.dimension).unwrap_or(0);
        let replaced = match self.inner.core.remove_index(&path).await {
            Ok(()) => {
                self.inner
                    .core
                    .create_index(&path, dimension, core_metric(row.metric))
                    .await
            }
            Err(error) => Err(error),
        };
        replaced.map_err(|cause| {
            let marked = self.patch_row(
                &row.id,
                &VectorStorePatch::status(VectorHealth::Broken, self.now()),
            );
            self.inner.logger.log(
                LogLevel::Error,
                SCOPE,
                "Could not reset the vector store",
                &[
                    ("storeId", json!(row.id)),
                    ("error", json!(cause.to_string())),
                ],
            );
            marked
                .err()
                .unwrap_or_else(|| native_error(CLEAR_FAILED, &cause))
        })
    }

    pub async fn reconcile(&self, id: &str) -> Result<VectorStoreDto, AppError> {
        let _lease = self.exclusive(id)?;
        let row = self.require(id)?;
        let observed = self.observe(&row).await;
        match observed.stats {
            Some(stats) if observed.health != VectorHealth::Broken => self.repair(&row, &stats),
            _ => store_dto(
                &self.patch_row(id, &VectorStorePatch::status(observed.health, self.now()))?,
            ),
        }
    }

    pub async fn search(
        &self,
        store_id: &str,
        query: &str,
        options: VectorSearchOptions,
        cancel: &CancellationToken,
    ) -> Result<Vec<VectorSearchHitDto>, AppError> {
        Ok(self
            .search_timed(store_id, query, options, cancel)
            .await?
            .hits)
    }

    pub async fn search_timed(
        &self,
        store_id: &str,
        query: &str,
        options: VectorSearchOptions,
        cancel: &CancellationToken,
    ) -> Result<VectorSearchResultDto, AppError> {
        let input = VectorSearchInput {
            k: options.k,
            min_score: options.min_score,
            mode: options.mode,
            ..VectorSearchInput::new(
                VectorStoreId::parse(store_id)
                    .ok_or_else(|| AppError::new(AppErrorCode::NotFound, STORE_NOT_FOUND))?,
                query,
            )
        };
        zvs_contract::Validate::validate(&input)?;
        let query = js_trim(&input.query).to_owned();
        let k = input.k();
        let min_score = input.min_score();
        Self::check(cancel)?;
        let embedded = self.require(store_id)?;
        require_backend(&embedded)?;
        let embedding_start = self.elapsed();
        let vector = self.embed_query(&embedded, &query, cancel).await?;
        let embedding_ms = self.elapsed() - embedding_start;

        let (row, hits, search_ms, mode, reranking) = {
            let _lease = self.exclusive(store_id)?;
            Self::check(cancel)?;
            let row = self.require_embedded_with(store_id, &embedded)?;
            let mode = effective_mode(row.search_mode, row.schema_version, input.mode);
            let reranking = options.rerank && row.rerank.enabled;
            let search_start = self.elapsed();
            let fetch = if reranking {
                k.max(row.rerank.candidates)
            } else {
                k
            };
            let hits = self
                .inner
                .core
                .search(
                    &self.path(store_id)?,
                    VectorSearchRequest {
                        vector,
                        k: usize::try_from(fetch).unwrap_or(usize::MAX),
                        min_score: if reranking { 0.0 } else { min_score as f32 },
                        mode: Some(core_mode(mode)),
                        query: Some(query.clone()),
                        ..VectorSearchRequest::default()
                    },
                )
                .await?
                .into_iter()
                .map(hit_dto)
                .collect::<Vec<_>>();
            (row, hits, self.elapsed() - search_start, mode, reranking)
        };
        Self::check(cancel)?;
        if !reranking {
            return Ok(VectorSearchResultDto {
                hits,
                embedding_ms,
                search_ms,
                rerank_ms: None,
                rerank: None,
                mode: Some(mode),
            });
        }
        let rerank_start = self.elapsed();
        let reranked = self.rerank_hits(&row, &query, &hits, cancel).await?;
        let rerank_ms = self.elapsed() - rerank_start;
        let outcome = if reranked.is_some() {
            VectorRerankOutcome::Applied
        } else {
            VectorRerankOutcome::Fallback
        };
        let limit = usize::try_from(k).unwrap_or(usize::MAX);
        Ok(VectorSearchResultDto {
            hits: reranked
                .unwrap_or(hits)
                .into_iter()
                .filter(|hit| hit.score >= min_score)
                .take(limit)
                .collect(),
            embedding_ms,
            search_ms,
            rerank_ms: Some(rerank_ms),
            rerank: Some(outcome),
            mode: Some(mode),
        })
    }

    pub async fn chunks(
        &self,
        store_id: &str,
        document_id: &str,
        indices: &[u64],
    ) -> Result<Vec<VectorChunkDto>, AppError> {
        let _lease = self.exclusive(store_id)?;
        let row = self.require(store_id)?;
        require_backend(&row)?;
        let mut seen = HashSet::new();
        let wanted: Vec<u32> = indices
            .iter()
            .filter_map(|index| u32::try_from(*index).ok())
            .filter(|index| seen.insert(*index))
            .collect();
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .inner
            .core
            .chunks(&self.path(store_id)?, document_id, &wanted)
            .await?
            .into_iter()
            .map(chunk_dto)
            .collect())
    }

    pub fn document(
        &self,
        store_id: &str,
        document_id: &str,
    ) -> Result<Option<VectorDocumentDto>, AppError> {
        let row = self.inner.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .vector_documents
                .find_by_id(document_id)?)
        })?;
        row.filter(|row| row.store_id == store_id)
            .map(|row| crate::services::vector_sources::document_dto(&row))
            .transpose()
    }

    pub async fn related_chunks(
        &self,
        store_id: &str,
        query: &str,
        document_ids: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<VectorSearchHitDto>, AppError> {
        let embedded = self.require(store_id)?;
        require_backend(&embedded)?;
        let mut seen = HashSet::new();
        let ids: Vec<&String> = document_ids.iter().filter(|id| seen.insert(*id)).collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let vector = self.embed_query(&embedded, query, cancel).await?;
        let _lease = self.exclusive(store_id)?;
        self.require_embedded_with(store_id, &embedded)?;
        let path = self.path(store_id)?;
        let mut hits = Vec::new();
        for document_id in ids {
            Self::check(cancel)?;
            let best = self
                .inner
                .core
                .search(
                    &path,
                    VectorSearchRequest {
                        vector: vector.clone(),
                        k: 1,
                        min_score: 0.0,
                        mode: Some(CoreMode::Vector),
                        document_ids: Some(vec![document_id.clone()]),
                        ..VectorSearchRequest::default()
                    },
                )
                .await?
                .into_iter()
                .next();
            hits.extend(best.map(hit_dto));
        }
        Ok(hits)
    }

    async fn embed_query(
        &self,
        row: &VectorStoreRow,
        query: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<f32>, AppError> {
        let provider = self.require_provider(&row.embedding_provider_id)?;
        let driver = self.inner.drivers.ephemeral_driver(&provider).await?;
        Self::check(cancel)?;
        let embedding = driver
            .embedding
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, NO_EMBEDDINGS))?;
        let seconds = provider
            .settings
            .timeout_seconds()
            .unwrap_or(DEFAULT_EMBEDDING_TIMEOUT_SECONDS);
        let scope = cancel.child_token();
        let texts = [format!("{}{query}", row.query_prefix)];
        let vectors = tokio::select! {
            result = embedding.embed(&texts, &row.embedding_model_id, &scope) => result,
            () = tokio::time::sleep(Duration::from_secs_f64(seconds.max(0.0))) => {
                scope.cancel();
                Err(timed_out(seconds))
            }
        };
        let vectors = vectors?;
        Self::check(cancel)?;
        let dimension = usize::try_from(row.dimension).unwrap_or(usize::MAX);
        match <[Vec<f32>; 1]>::try_from(vectors) {
            Ok([vector])
                if vector.len() == dimension && vector.iter().all(|value| value.is_finite()) =>
            {
                Ok(vector)
            }
            _ => Err(AppError::new(
                AppErrorCode::ValidationFailed,
                DIMENSION_MISMATCH,
            )),
        }
    }

    async fn rerank_hits(
        &self,
        row: &VectorStoreRow,
        query: &str,
        hits: &[VectorSearchHitDto],
        cancel: &CancellationToken,
    ) -> Result<Option<Vec<VectorSearchHitDto>>, AppError> {
        if hits.is_empty() {
            return Ok(Some(Vec::new()));
        }
        let source = self
            .inner
            .rerankers
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let Some(source) = source else {
            self.inner.logger.log(
                LogLevel::Warn,
                SCOPE,
                RERANK_UNAVAILABLE,
                &[
                    ("storeId", json!(row.id)),
                    ("modelRef", json!(row.rerank.model_ref)),
                ],
            );
            return Ok(None);
        };
        let reranker = source(&row.rerank.model_ref);
        let passages: Vec<String> = hits.iter().map(passage_of).collect();
        let scope = cancel.child_token();
        let mut timed = false;
        let answer = tokio::select! {
            result = reranker.rerank(js_slice(query, MAX_RERANK_QUERY_CHARS), &passages, &scope) => result,
            () = tokio::time::sleep(self.inner.rerank_timeout) => {
                timed = true;
                scope.cancel();
                Err(timed_out(self.inner.rerank_timeout.as_secs_f64()))
            }
        };
        let checked = answer.and_then(|scores| {
            if scores.len() == hits.len() && scores.iter().all(|score| score.is_finite()) {
                Ok(scores)
            } else {
                Err(
                    AppError::new(AppErrorCode::ValidationFailed, RERANK_MALFORMED)
                        .with_detail("expected", hits.len())
                        .with_detail("received", scores.len()),
                )
            }
        });
        match checked {
            Ok(scores) => Ok(Some(apply_rerank(hits, &scores))),
            Err(error) if cancel.is_cancelled() => Err(error),
            Err(error) => {
                self.inner.logger.log(
                    LogLevel::Warn,
                    SCOPE,
                    RERANK_FAILED,
                    &[
                        ("storeId", json!(row.id)),
                        ("modelRef", json!(row.rerank.model_ref)),
                        ("timedOut", json!(timed)),
                        ("error", json!(error.to_string())),
                    ],
                );
                Ok(None)
            }
        }
    }

    pub fn lease(&self, id: &str) -> Result<(VectorStoreRow, StoreLease), AppError> {
        let lease = self.exclusive(id)?;
        Ok((self.require(id)?, lease))
    }

    pub fn require_store(&self, id: &str) -> Result<VectorStoreRow, AppError> {
        self.require(id)
    }

    pub fn store_path(&self, id: &str) -> Result<PathBuf, AppError> {
        self.path(id)
    }

    pub fn core(&self) -> &SharedVectorCore {
        &self.inner.core
    }

    async fn observe(&self, row: &VectorStoreRow) -> Observed {
        let observed = match require_backend(row).and_then(|()| self.path(&row.id)) {
            Ok(path) => self.inner.core.stats(&path).await,
            Err(error) => Err(error),
        };
        match observed {
            Ok(stats) => Observed {
                health: derive_vector_health(row, Some(&stats), false),
                stats: Some(stats),
            },
            Err(error) => {
                let missing = error.code == AppErrorCode::NotFound;
                if !missing {
                    self.inner.logger.log(
                        LogLevel::Warn,
                        SCOPE,
                        "Could not inspect vector store",
                        &[
                            ("storeId", json!(row.id)),
                            ("error", json!(error.to_string())),
                        ],
                    );
                }
                Observed {
                    stats: None,
                    health: derive_vector_health(row, None, !missing),
                }
            }
        }
    }

    fn repair(
        &self,
        row: &VectorStoreRow,
        stats: &VectorStats,
    ) -> Result<VectorStoreDto, AppError> {
        let now = self.now();
        let repaired = self.patch_row(
            &row.id,
            &VectorStorePatch {
                documents: Some(stored_count(stats.document_count)?),
                vectors: Some(stored_count(stats.row_count)?),
                bytes: Some(stored_count(stats.on_disk_bytes)?),
                index_type: Some(stats.index_type.clone()),
                schema_version: Some(i64::from(stats.schema_version)),
                status: Some(VectorHealth::Healthy),
                table_created_at: Some(Some(row.table_created_at.unwrap_or(now))),
                updated_at: Some(now),
                ..VectorStorePatch::default()
            },
        )?;
        store_dto(&repaired)
    }

    fn patch_row(&self, id: &str, patch: &VectorStorePatch) -> Result<VectorStoreRow, AppError> {
        self.inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .vector_stores
                    .update(id, patch)?)
            })?
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, STORE_NOT_FOUND))
    }

    fn require_provider(&self, id: &str) -> Result<ProviderRow, AppError> {
        let provider = self
            .inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).providers.find_by_id(id)?)
            })?
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, PROVIDER_NOT_FOUND))?;
        if !provider.enabled || !provider.cap_embedding {
            return Err(AppError::new(AppErrorCode::Conflict, PROVIDER_UNUSABLE));
        }
        Ok(provider)
    }

    fn require_embedded_with(
        &self,
        id: &str,
        embedded: &VectorStoreRow,
    ) -> Result<VectorStoreRow, AppError> {
        let row = self.require(id)?;
        require_backend(&row)?;
        if row.embedding_provider_id != embedded.embedding_provider_id
            || row.embedding_model_id != embedded.embedding_model_id
            || row.dimension != embedded.dimension
            || row.query_prefix != embedded.query_prefix
        {
            return Err(AppError::new(AppErrorCode::Conflict, STORE_CHANGED));
        }
        Ok(row)
    }

    fn require_name(&self, name: &str, id: Option<&str>) -> Result<(), AppError> {
        let existing = self.inner.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .vector_stores
                .find_by_name(name)?)
        })?;
        match existing {
            Some(existing) if Some(existing.id.as_str()) != id => {
                Err(AppError::new(AppErrorCode::Conflict, NAME_TAKEN))
            }
            _ => Ok(()),
        }
    }

    fn require(&self, id: &str) -> Result<VectorStoreRow, AppError> {
        self.inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).vector_stores.find_by_id(id)?)
            })?
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, STORE_NOT_FOUND))
    }

    fn exclusive(&self, id: &str) -> Result<StoreLease, AppError> {
        let mut busy = self
            .inner
            .busy
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !busy.insert(id.to_owned()) {
            return Err(AppError::new(AppErrorCode::Conflict, STORE_BUSY));
        }
        Ok(StoreLease {
            inner: Arc::clone(&self.inner),
            id: id.to_owned(),
        })
    }

    pub fn is_busy(&self, id: &str) -> bool {
        self.inner
            .busy
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(id)
    }

    fn check(cancel: &CancellationToken) -> Result<(), AppError> {
        if cancel.is_cancelled() {
            Err(cancelled_error())
        } else {
            Ok(())
        }
    }

    fn path(&self, id: &str) -> Result<PathBuf, AppError> {
        vector_store_path(&self.inner.directory, id)
            .map_err(|error| AppError::new(AppErrorCode::ValidationFailed, error.to_string()))
    }

    fn now(&self) -> i64 {
        self.inner.clock.now()
    }

    fn elapsed(&self) -> f64 {
        (self.inner.stopwatch)()
    }
}
