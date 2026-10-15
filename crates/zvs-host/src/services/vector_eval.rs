#[cfg(test)]
mod parity;
#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use zvs_contract::validate::js_trim;
use zvs_contract::{
    AddVectorEvalCaseInput, AppError, AppErrorCode, DocumentId, VECTOR_SCHEMA_VERSION,
    VectorEvalCaseDto, VectorEvalCaseId, VectorEvalCaseResultDto, VectorEvalMode,
    VectorEvalModeResultDto, VectorEvalRunDto, VectorEvalRunInput, VectorEvalSetDto,
    VectorRerankOutcome, VectorSearchMode, VectorSearchResultDto, VectorStoreId,
};

use super::retrieval_metrics::{ranked_documents, retrieval_metrics, score_case};
use super::stored::stored_id;
use super::vector_stores::{
    Stopwatch, VectorSearchOptions, VectorStoreService, monotonic_stopwatch,
};
use crate::data::repositories::{VectorEvalCaseDraft, VectorStorePatch};
use crate::data::schema::{VectorEvalCaseRow, VectorStoreRow};
use crate::data::{Database, Repositories};
use crate::drivers::core::cancelled_error;
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger};

pub const EXPECTED_MISSING: &str = "Ожидаемый документ не найден в хранилище";
pub const CASE_NOT_FOUND: &str = "Вопрос проверки не найден";
pub const NO_CASES: &str = "Добавьте хотя бы один вопрос для проверки";
pub const ALREADY_RUNNING: &str = "Проверка уже идёт";

#[async_trait]
pub trait EvalSearch: Send + Sync {
    fn require_store(&self, id: &str) -> Result<VectorStoreRow, AppError>;

    async fn search_timed(
        &self,
        store_id: &str,
        query: &str,
        options: VectorSearchOptions,
        cancel: &CancellationToken,
    ) -> Result<VectorSearchResultDto, AppError>;
}

#[async_trait]
impl EvalSearch for VectorStoreService {
    fn require_store(&self, id: &str) -> Result<VectorStoreRow, AppError> {
        Self::require_store(self, id)
    }

    async fn search_timed(
        &self,
        store_id: &str,
        query: &str,
        options: VectorSearchOptions,
        cancel: &CancellationToken,
    ) -> Result<VectorSearchResultDto, AppError> {
        Self::search_timed(self, store_id, query, options, cancel).await
    }
}

pub fn eval_modes(hybrid: bool, rerank: bool) -> Vec<VectorEvalMode> {
    match (hybrid, rerank) {
        (false, false) => vec![VectorEvalMode::Vector],
        (false, true) => vec![VectorEvalMode::Vector, VectorEvalMode::Rerank],
        (true, false) => vec![VectorEvalMode::Vector, VectorEvalMode::Hybrid],
        (true, true) => vec![
            VectorEvalMode::Vector,
            VectorEvalMode::Hybrid,
            VectorEvalMode::HybridRerank,
        ],
    }
}

pub const fn mode_options(mode: VectorEvalMode, k: u64) -> VectorSearchOptions {
    VectorSearchOptions {
        k: Some(k),
        min_score: Some(0.0),
        rerank: matches!(mode, VectorEvalMode::Rerank | VectorEvalMode::HybridRerank),
        mode: Some(match mode {
            VectorEvalMode::Hybrid | VectorEvalMode::HybridRerank => VectorSearchMode::Hybrid,
            VectorEvalMode::Vector | VectorEvalMode::Rerank => VectorSearchMode::Vector,
        }),
    }
}

fn case_dto(row: &VectorEvalCaseRow) -> Result<VectorEvalCaseDto, AppError> {
    Ok(VectorEvalCaseDto {
        id: stored_id(VectorEvalCaseId::parse, &row.id)?,
        store_id: stored_id(VectorStoreId::parse, &row.store_id)?,
        question: row.question.clone(),
        expected_document_ids: row
            .expected_document_ids
            .iter()
            .map(|id| stored_id(DocumentId::parse, id))
            .collect::<Result<_, _>>()?,
        created_at: row.created_at,
    })
}

fn unique(ids: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.iter()
        .filter(|id| seen.insert(id.as_str()))
        .cloned()
        .collect()
}

pub struct VectorEvalServiceOptions {
    pub database: Arc<Database>,
    pub stores: Arc<dyn EvalSearch>,
    pub logger: SharedLogger,
    pub clock: SharedClock,
    pub stopwatch: Stopwatch,
}

impl VectorEvalServiceOptions {
    pub fn new(
        database: Arc<Database>,
        stores: Arc<dyn EvalSearch>,
        logger: SharedLogger,
        clock: SharedClock,
    ) -> Self {
        Self {
            database,
            stores,
            logger,
            clock,
            stopwatch: monotonic_stopwatch(),
        }
    }
}

struct Inner {
    options: VectorEvalServiceOptions,
    running: Mutex<HashSet<String>>,
}

#[derive(Clone)]
pub struct VectorEvalService {
    inner: Arc<Inner>,
}

struct Running<'a> {
    set: &'a Mutex<HashSet<String>>,
    id: String,
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.set
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
    }
}

impl VectorEvalService {
    pub fn new(options: VectorEvalServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                running: Mutex::new(HashSet::new()),
            }),
        }
    }

    pub fn list(&self, store_id: &str) -> Result<VectorEvalSetDto, AppError> {
        let store = self.inner.options.stores.require_store(store_id)?;
        let cases = self.cases(store_id)?;
        Ok(VectorEvalSetDto {
            cases: cases.iter().map(case_dto).collect::<Result<_, _>>()?,
            last_run: store.last_eval,
        })
    }

    pub fn add(&self, input: &AddVectorEvalCaseInput) -> Result<VectorEvalCaseDto, AppError> {
        zvs_contract::Validate::validate(input)?;
        let store_id = input.store_id.to_string();
        self.inner.options.stores.require_store(&store_id)?;
        let known: HashSet<String> = self
            .inner
            .options
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .vector_documents
                    .list_by_store(&store_id)?)
            })?
            .into_iter()
            .map(|document| document.id)
            .collect();
        let expected = unique(
            &input
                .expected_document_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        );
        let missing: Vec<&String> = expected.iter().filter(|id| !known.contains(*id)).collect();
        if !missing.is_empty() {
            return Err(
                AppError::new(AppErrorCode::ValidationFailed, EXPECTED_MISSING)
                    .with_detail("missing", json!(missing)),
            );
        }
        let draft = VectorEvalCaseDraft {
            id: new_id().hyphenated().to_string(),
            store_id,
            question: js_trim(&input.question).to_owned(),
            expected_document_ids: expected,
            created_at: self.inner.options.clock.now(),
        };
        let row = self.inner.options.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .vector_eval_cases
                .add(&draft)?)
        })?;
        case_dto(&row)
    }

    pub fn remove(&self, store_id: &str, id: &str) -> Result<(), AppError> {
        let removed = self.inner.options.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .vector_eval_cases
                .remove(store_id, id)?)
        })?;
        if removed == 0 {
            return Err(AppError::new(AppErrorCode::NotFound, CASE_NOT_FOUND));
        }
        Ok(())
    }

    pub async fn run(
        &self,
        input: &VectorEvalRunInput,
        cancel: &CancellationToken,
    ) -> Result<VectorEvalRunDto, AppError> {
        zvs_contract::Validate::validate(input)?;
        let store_id = input.store_id.to_string();
        let k = input.k();
        let store = self.inner.options.stores.require_store(&store_id)?;
        let cases = self.cases(&store_id)?;
        if cases.is_empty() {
            return Err(AppError::new(AppErrorCode::ValidationFailed, NO_CASES));
        }
        let _running = self.start(&store_id)?;
        let started = (self.inner.options.stopwatch)();
        let hybrid = u64::try_from(store.schema_version)
            .is_ok_and(|version| version >= VECTOR_SCHEMA_VERSION);
        let mut modes = Vec::new();
        for mode in eval_modes(hybrid, store.rerank.enabled) {
            modes.push(self.run_mode(&store_id, mode, &cases, k, cancel).await?);
        }
        let run = VectorEvalRunDto {
            store_id: input.store_id,
            k,
            ran_at: self.inner.options.clock.now(),
            duration_ms: (self.inner.options.stopwatch)() - started,
            modes,
        };
        let stored = run.clone();
        self.inner.options.database.with_connection(|connection| {
            Repositories::new(connection).vector_stores.update(
                &store_id,
                &VectorStorePatch {
                    last_eval: Some(Some(stored)),
                    ..VectorStorePatch::default()
                },
            )?;
            Ok(())
        })?;
        self.inner.options.logger.log(
            LogLevel::Info,
            "vectorStores",
            "Ran a retrieval evaluation",
            &[
                ("storeId", json!(store_id)),
                ("cases", json!(cases.len())),
                (
                    "modes",
                    json!(
                        run.modes
                            .iter()
                            .map(|result| json!({ "mode": result.mode, "mrr": result.metrics.mrr }))
                            .collect::<Vec<_>>()
                    ),
                ),
            ],
        );
        Ok(run)
    }

    async fn run_mode(
        &self,
        store_id: &str,
        mode: VectorEvalMode,
        cases: &[VectorEvalCaseRow],
        k: u64,
        cancel: &CancellationToken,
    ) -> Result<VectorEvalModeResultDto, AppError> {
        let depth = usize::try_from(k).unwrap_or(usize::MAX);
        let mut results = Vec::new();
        let mut scores = Vec::new();
        let mut fallbacks = 0;
        for entry in cases {
            if cancel.is_cancelled() {
                return Err(cancelled_error());
            }
            let case_id = stored_id(VectorEvalCaseId::parse, &entry.id)?;
            match self
                .inner
                .options
                .stores
                .search_timed(store_id, &entry.question, mode_options(mode, k), cancel)
                .await
            {
                Ok(result) => {
                    if result.rerank == Some(VectorRerankOutcome::Fallback) {
                        fallbacks += 1;
                    }
                    let ranked =
                        ranked_documents(result.hits.iter().map(|hit| hit.document_id.as_str()));
                    let score = score_case(&ranked, &entry.expected_document_ids, depth);
                    scores.push(score);
                    results.push(VectorEvalCaseResultDto {
                        case_id,
                        rank: score.rank,
                        found: score.found,
                        expected: score.expected,
                        error: None,
                    });
                }
                Err(error) if cancel.is_cancelled() => return Err(error),
                Err(error) => {
                    let score = super::retrieval_metrics::CaseScore {
                        rank: None,
                        found: 0,
                        expected: unique(&entry.expected_document_ids).len() as u64,
                    };
                    scores.push(score);
                    results.push(VectorEvalCaseResultDto {
                        case_id,
                        rank: None,
                        found: 0,
                        expected: score.expected,
                        error: Some(error.message),
                    });
                }
            }
        }
        Ok(VectorEvalModeResultDto {
            mode,
            metrics: retrieval_metrics(&scores),
            cases: results,
            fallbacks,
        })
    }

    fn cases(&self, store_id: &str) -> Result<Vec<VectorEvalCaseRow>, AppError> {
        self.inner.options.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .vector_eval_cases
                .list_by_store(store_id)?)
        })
    }

    fn start(&self, store_id: &str) -> Result<Running<'_>, AppError> {
        let mut running = self
            .inner
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !running.insert(store_id.to_owned()) {
            return Err(AppError::new(AppErrorCode::Conflict, ALREADY_RUNNING));
        }
        Ok(Running {
            set: &self.inner.running,
            id: store_id.to_owned(),
        })
    }
}
