use std::sync::Mutex;

use serde_json::json;
use zvs_contract::{VectorRerankConfig, VectorSearchHitDto, VectorStoreDto};

use super::*;
use crate::services::vector_stores::tests::{Fixture, ProbeVectorCore, fixture};
use crate::testing::FakeClock;

type Script = Box<
    dyn Fn(&str, &VectorSearchOptions) -> Result<VectorSearchResultDto, AppError> + Send + Sync,
>;

struct ScriptedSearch {
    stores: VectorStoreService,
    script: Mutex<Option<Script>>,
    calls: Mutex<Vec<(String, VectorSearchOptions)>>,
}

impl ScriptedSearch {
    fn script(&self, script: Script) {
        *self.script.lock().expect("script") = Some(script);
    }

    fn calls(&self) -> Vec<(String, VectorSearchOptions)> {
        self.calls.lock().expect("calls").clone()
    }
}

#[async_trait]
impl EvalSearch for ScriptedSearch {
    fn require_store(&self, id: &str) -> Result<VectorStoreRow, AppError> {
        self.stores.require_store(id)
    }

    async fn search_timed(
        &self,
        store_id: &str,
        query: &str,
        options: VectorSearchOptions,
        cancel: &CancellationToken,
    ) -> Result<VectorSearchResultDto, AppError> {
        self.calls
            .lock()
            .expect("calls")
            .push((query.to_owned(), options));
        let answer = self
            .script
            .lock()
            .expect("script")
            .as_ref()
            .map(|script| script(query, &options));
        match answer {
            Some(answer) => answer,
            None => {
                self.stores
                    .search_timed(store_id, query, options, cancel)
                    .await
            }
        }
    }
}

struct Evaluation {
    base: Fixture,
    search: Arc<ScriptedSearch>,
    service: VectorEvalService,
    store: VectorStoreDto,
    cats: String,
    dogs: String,
    birds: String,
}

async fn evaluation() -> Evaluation {
    let base = fixture();
    let store = base.store().await;
    let id = store.id.to_string();
    let cats = base.record_document(&id, "C:/docs/cats.md");
    let dogs = base.record_document(&id, "C:/docs/dogs.md");
    let birds = base.record_document(&id, "C:/docs/birds.md");
    let search = Arc::new(ScriptedSearch {
        stores: base.service.clone(),
        script: Mutex::new(None),
        calls: Mutex::new(Vec::new()),
    });
    let mut options = VectorEvalServiceOptions::new(
        base.database.shared(),
        search.clone(),
        crate::platform::logger::silent_logger(),
        Arc::new(FakeClock::new(5000)),
    );
    options.stopwatch = Arc::new(|| 0.0);
    Evaluation {
        service: VectorEvalService::new(options),
        base,
        search,
        store,
        cats,
        dogs,
        birds,
    }
}

fn result(order: &[&str], rerank: Option<VectorRerankOutcome>) -> VectorSearchResultDto {
    VectorSearchResultDto {
        hits: order
            .iter()
            .enumerate()
            .map(|(index, document_id)| VectorSearchHitDto {
                id: format!("{document_id}:{index}"),
                score: 1.0 - index as f64 / 10.0,
                payload: serde_json::Value::Null,
                document_id: (*document_id).to_owned(),
                chunk_index: 0,
                path: (*document_id).to_owned(),
                text: String::new(),
                section: String::new(),
                page: None,
                vector_score: None,
                rerank_score: None,
                keyword_score: None,
                fused_score: None,
                provenance: None,
            })
            .collect(),
        embedding_ms: 1.0,
        search_ms: 1.0,
        rerank_ms: None,
        rerank,
        mode: None,
    }
}

impl Evaluation {
    fn add(&self, question: &str, expected: &[&str]) -> VectorEvalCaseDto {
        self.service
            .add(&AddVectorEvalCaseInput {
                store_id: self.store.id,
                question: question.to_owned(),
                expected_document_ids: expected
                    .iter()
                    .map(|id| DocumentId::parse(id).expect("an id"))
                    .collect(),
            })
            .expect("adds")
    }

    async fn run(&self, k: u64) -> Result<VectorEvalRunDto, AppError> {
        self.service
            .run(
                &VectorEvalRunInput {
                    store_id: self.store.id,
                    k: Some(k),
                },
                &CancellationToken::new(),
            )
            .await
    }

    async fn v1(&self) {
        *ProbeVectorCore::lock(&self.base.core.schema_version) = Some(1);
        self.base
            .service
            .reconcile(&self.store.id.to_string())
            .await
            .expect("reconciles");
    }

    async fn enable_rerank(&self) {
        self.base
            .service
            .update(&zvs_contract::UpdateVectorStoreInput {
                rerank: Some(VectorRerankConfig {
                    enabled: true,
                    model_ref: "curated:model:bge-reranker-v2-m3-f16".to_owned(),
                    candidates: 50,
                }),
                ..zvs_contract::UpdateVectorStoreInput::new(self.store.id)
            })
            .await
            .expect("updates");
    }
}

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() < 1e-12
}

#[tokio::test]
async fn cases_are_written_listed_in_order_and_removed_per_store() {
    let setup = evaluation().await;
    let cats = setup.cats.clone();
    let first = setup.add("Who purrs?", &[&cats, &cats]);
    setup.add("Who barks?", &[&setup.dogs]);
    assert_eq!(
        first.expected_document_ids,
        [DocumentId::parse(&cats).expect("an id")]
    );
    let store = setup.store.id.to_string();
    let set = setup.service.list(&store).expect("lists");
    assert_eq!(
        set.cases
            .iter()
            .map(|entry| entry.question.as_str())
            .collect::<Vec<_>>(),
        ["Who purrs?", "Who barks?"]
    );
    assert_eq!(set.last_run, None);
    setup
        .service
        .remove(&store, &first.id.to_string())
        .expect("removes");
    assert_eq!(setup.service.list(&store).expect("lists").cases.len(), 1);
    let error = setup
        .service
        .remove(&store, &first.id.to_string())
        .expect_err("gone");
    assert_eq!(error.code, AppErrorCode::NotFound);
    assert_eq!(error.message, CASE_NOT_FOUND);
}

#[tokio::test]
async fn a_question_is_trimmed_when_it_is_stored() {
    let setup = evaluation().await;
    assert_eq!(setup.add("  Who?  ", &[&setup.cats]).question, "Who?");
}

#[tokio::test]
async fn a_case_naming_a_document_outside_the_store_is_refused() {
    let setup = evaluation().await;
    let stranger = new_id().hyphenated().to_string();
    let error = setup
        .service
        .add(&AddVectorEvalCaseInput {
            store_id: setup.store.id,
            question: "Who?".to_owned(),
            expected_document_ids: vec![DocumentId::parse(&stranger).expect("an id")],
        })
        .expect_err("refused");
    assert_eq!(error.code, AppErrorCode::ValidationFailed);
    assert_eq!(
        error.details.expect("details")["missing"],
        json!([stranger])
    );
}

#[tokio::test]
async fn running_with_nothing_to_check_says_so() {
    let setup = evaluation().await;
    let error = setup.run(10).await.expect_err("refused");
    assert_eq!(error.code, AppErrorCode::ValidationFailed);
    assert_eq!(error.message, NO_CASES);
}

#[test]
fn the_modes_compared_follow_what_the_store_can_do() {
    use VectorEvalMode::{Hybrid, HybridRerank, Rerank, Vector};
    assert_eq!(eval_modes(false, false), [Vector]);
    assert_eq!(eval_modes(false, true), [Vector, Rerank]);
    assert_eq!(eval_modes(true, false), [Vector, Hybrid]);
    assert_eq!(eval_modes(true, true), [Vector, Hybrid, HybridRerank]);
}

#[tokio::test]
async fn a_v1_store_without_reranking_reports_vector_only_metrics_and_is_kept_with_the_store() {
    let setup = evaluation().await;
    setup.v1().await;
    setup.add("cats", &[&setup.cats]);
    setup.add("dogs", &[&setup.dogs]);
    let (cats, dogs, birds) = (setup.cats.clone(), setup.dogs.clone(), setup.birds.clone());
    setup.search.script(Box::new(move |query, _| {
        Ok(if query == "cats" {
            result(&[&birds, &cats], None)
        } else {
            result(&[&dogs, &dogs], None)
        })
    }));
    let run = setup.run(10).await.expect("runs");
    let calls = setup.search.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0].1,
        VectorSearchOptions {
            k: Some(10),
            min_score: Some(0.0),
            rerank: false,
            mode: Some(VectorSearchMode::Vector),
        }
    );
    assert_eq!(
        run.modes.iter().map(|mode| mode.mode).collect::<Vec<_>>(),
        [VectorEvalMode::Vector]
    );
    assert_eq!(
        run.modes[0]
            .cases
            .iter()
            .map(|entry| entry.rank)
            .collect::<Vec<_>>(),
        [Some(2), Some(1)]
    );
    let metrics = run.modes[0].metrics;
    assert_eq!(metrics.cases, 2);
    assert!(close(metrics.hit_at1, 0.5));
    assert!(close(metrics.hit_at5, 1.0));
    assert!(close(metrics.mrr, 0.75));
    assert_eq!(run.ran_at, 5000);
    assert_eq!(
        setup
            .service
            .list(&setup.store.id.to_string())
            .expect("lists")
            .last_run,
        Some(run)
    );
}

#[tokio::test]
async fn a_v2_store_is_measured_as_vector_hybrid_and_hybrid_with_reranking() {
    let setup = evaluation().await;
    setup.enable_rerank().await;
    setup.add("cats", &[&setup.cats]);
    let (cats, dogs, birds) = (setup.cats.clone(), setup.dogs.clone(), setup.birds.clone());
    setup.search.script(Box::new(move |_, options| {
        Ok(if options.rerank {
            result(&[&cats], Some(VectorRerankOutcome::Applied))
        } else if options.mode == Some(VectorSearchMode::Hybrid) {
            result(&[&dogs, &cats], None)
        } else {
            result(&[&dogs, &birds, &cats], None)
        })
    }));
    let run = setup.run(5).await.expect("runs");
    assert_eq!(
        setup
            .search
            .calls()
            .into_iter()
            .map(|(_, options)| (options.rerank, options.mode))
            .collect::<Vec<_>>(),
        [
            (false, Some(VectorSearchMode::Vector)),
            (false, Some(VectorSearchMode::Hybrid)),
            (true, Some(VectorSearchMode::Hybrid)),
        ]
    );
    let mrr: Vec<(VectorEvalMode, f64)> = run
        .modes
        .iter()
        .map(|mode| (mode.mode, mode.metrics.mrr))
        .collect();
    assert_eq!(mrr[0].0, VectorEvalMode::Vector);
    assert!(close(mrr[0].1, 1.0 / 3.0));
    assert_eq!(mrr[1].0, VectorEvalMode::Hybrid);
    assert!(close(mrr[1].1, 0.5));
    assert_eq!(mrr[2].0, VectorEvalMode::HybridRerank);
    assert!(close(mrr[2].1, 1.0));
}

#[tokio::test]
async fn a_reranking_v1_store_is_measured_both_ways_so_the_gain_is_visible() {
    let setup = evaluation().await;
    setup.v1().await;
    setup.enable_rerank().await;
    setup.add("cats", &[&setup.cats]);
    let (cats, dogs, birds) = (setup.cats.clone(), setup.dogs.clone(), setup.birds.clone());
    setup.search.script(Box::new(move |_, options| {
        Ok(if options.rerank {
            result(&[&cats, &dogs], Some(VectorRerankOutcome::Applied))
        } else {
            result(&[&dogs, &birds, &cats], None)
        })
    }));
    let run = setup.run(5).await.expect("runs");
    assert_eq!(
        run.modes.iter().map(|mode| mode.mode).collect::<Vec<_>>(),
        [VectorEvalMode::Vector, VectorEvalMode::Rerank]
    );
    assert!(close(run.modes[0].metrics.mrr, 1.0 / 3.0));
    assert!(close(run.modes[1].metrics.mrr, 1.0));
    assert_eq!(run.modes[1].fallbacks, 0);
}

#[tokio::test]
async fn fallbacks_and_failed_searches_are_reported_per_case_instead_of_sinking_the_run() {
    let setup = evaluation().await;
    setup.enable_rerank().await;
    setup.add("cats", &[&setup.cats]);
    setup.add("boom", &[&setup.dogs]);
    let cats = setup.cats.clone();
    setup.search.script(Box::new(move |query, options| {
        if query == "boom" {
            return Err(AppError::new(
                AppErrorCode::ProviderUnreachable,
                "embedding provider is down",
            ));
        }
        Ok(result(
            &[&cats],
            options.rerank.then_some(VectorRerankOutcome::Fallback),
        ))
    }));
    let run = setup.run(5).await.expect("runs");
    let reranked = run
        .modes
        .iter()
        .find(|mode| mode.mode == VectorEvalMode::HybridRerank)
        .expect("a reranked mode");
    assert_eq!(reranked.fallbacks, 1);
    assert_eq!(
        reranked.cases[1],
        VectorEvalCaseResultDto {
            case_id: reranked.cases[1].case_id,
            rank: None,
            found: 0,
            expected: 1,
            error: Some("embedding provider is down".to_owned()),
        }
    );
    assert!(close(reranked.metrics.mrr, 0.5));
}

#[tokio::test]
async fn a_cancelled_run_stops_instead_of_scoring_the_rest() {
    let setup = evaluation().await;
    setup.add("cats", &[&setup.cats]);
    let cancel = CancellationToken::new();
    cancel.cancel();
    let error = setup
        .service
        .run(
            &VectorEvalRunInput {
                store_id: setup.store.id,
                k: None,
            },
            &cancel,
        )
        .await
        .expect_err("cancelled");
    assert_eq!(error.code, AppErrorCode::RunCancelled);
    assert!(setup.search.calls().is_empty());
}

#[tokio::test]
async fn cases_and_the_kept_result_go_with_the_store() {
    let setup = evaluation().await;
    setup.add("cats", &[&setup.cats]);
    let store = setup.store.id.to_string();
    setup.base.service.remove(&store).await.expect("removes");
    assert!(
        setup
            .base
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .vector_eval_cases
                    .list_by_store(&store)?)
            })
            .expect("reads")
            .is_empty()
    );
}
