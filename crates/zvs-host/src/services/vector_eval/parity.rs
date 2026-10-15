use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use regex::Regex;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AddVectorEvalCaseInput, AppError, CreateVectorStoreInput, DocumentId, ProviderId,
    VectorEvalRunDto, VectorEvalRunInput, VectorRerankConfig,
};

use crate::data::Repositories;
use crate::data::repositories::VectorDocumentDraft;
use crate::data::schema::ProviderRow;
use crate::drivers::ai::capabilities::AdapterCapabilities;
use crate::drivers::ai::ports::{AiDriver, DiscoveredModel, EmbeddingDriver};
use crate::drivers::core::{InProcessCore, VectorCore, VectorRow};
use crate::platform::clock::FakeClock;
use crate::platform::logger::silent_logger;
use crate::services::providers::DriverSource;
use crate::services::rerank::Reranker;
use crate::services::vector_sources::DEFAULT_ANN_THRESHOLD;
use crate::services::{
    VectorEvalService, VectorEvalServiceOptions, VectorStoreService, VectorStoreServiceOptions,
};
use crate::testing::{FAKE_CAPABILITIES, seed_embedding_provider, temporary_database};

pub const CORPUS: &str = "test/fixtures/retrieval/eval-corpus.json";

pub fn corpus() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(CORPUS);
    serde_json::from_str(&std::fs::read_to_string(&path).expect("the corpus fixture"))
        .expect("a JSON corpus")
}

fn floats(value: &Value) -> Vec<f32> {
    value
        .as_array()
        .expect("a vector")
        .iter()
        .map(|number| number.as_f64().expect("a number") as f32)
        .collect()
}

struct CorpusEmbedder(HashMap<String, Vec<f32>>);

#[async_trait]
impl EmbeddingDriver for CorpusEmbedder {
    fn capabilities(&self) -> AdapterCapabilities {
        FAKE_CAPABILITIES
    }

    async fn embed(
        &self,
        texts: &[String],
        _model: &str,
        _cancel: &CancellationToken,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        Ok(texts
            .iter()
            .map(|text| self.0.get(text).cloned().expect("a fixture question"))
            .collect())
    }

    fn dimensions(&self) -> Option<usize> {
        None
    }

    async fn list_models(
        &self,
        _cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        Ok(Vec::new())
    }
}

struct CorpusDrivers(Arc<CorpusEmbedder>);

#[async_trait]
impl DriverSource for CorpusDrivers {
    async fn ephemeral_driver(&self, _row: &ProviderRow) -> Result<AiDriver, AppError> {
        Ok(AiDriver {
            text: None,
            embedding: Some(self.0.clone()),
            image: None,
        })
    }

    fn invalidate(&self, _provider_id: &str) {}
}

static TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{L}\p{N}]+").expect("the token pattern compiles"));

fn tokens(text: &str) -> HashSet<String> {
    TOKEN
        .find_iter(&text.to_lowercase())
        .map(|token| token.as_str().to_owned())
        .collect()
}

pub fn token_overlap(query: &str, passage: &str) -> f64 {
    let passage = tokens(passage);
    f64::from(
        u32::try_from(
            tokens(query)
                .iter()
                .filter(|token| passage.contains(*token))
                .count(),
        )
        .unwrap_or(u32::MAX),
    )
}

struct TokenOverlap;

#[async_trait]
impl Reranker for TokenOverlap {
    async fn rerank(
        &self,
        query: &str,
        passages: &[String],
        _cancel: &CancellationToken,
    ) -> Result<Vec<f64>, AppError> {
        Ok(passages
            .iter()
            .map(|passage| token_overlap(query, passage))
            .collect())
    }
}

pub async fn run_corpus() -> VectorEvalRunDto {
    let corpus = corpus();
    let database = temporary_database();
    let provider = database
        .with_connection(|connection| Ok(seed_embedding_provider(connection)))
        .expect("seeds a provider");
    let embedder = CorpusEmbedder(
        corpus["cases"]
            .as_array()
            .expect("cases")
            .iter()
            .map(|case| {
                (
                    case["question"].as_str().expect("a question").to_owned(),
                    floats(&case["vector"]),
                )
            })
            .collect(),
    );
    let core = InProcessCore::shared();
    let mut options = VectorStoreServiceOptions::new(
        database.shared(),
        core.clone(),
        Arc::new(CorpusDrivers(Arc::new(embedder))),
        database.directory().join("stores"),
        silent_logger(),
        Arc::new(FakeClock::new(1000)),
    );
    options.rerankers = Some(Arc::new(|_: &str| {
        Arc::new(TokenOverlap) as Arc<dyn Reranker>
    }));
    let stores = VectorStoreService::new(options);
    let store = stores
        .create(&CreateVectorStoreInput {
            dimension: corpus["dimension"].as_u64(),
            rerank: Some(VectorRerankConfig {
                enabled: true,
                model_ref: corpus["rerankModel"].as_str().expect("a model").to_owned(),
                candidates: 50,
            }),
            ..CreateVectorStoreInput::new(
                "Parity",
                ProviderId::parse(&provider).expect("an id"),
                "fixture-embedding",
            )
        })
        .await
        .expect("creates the store");
    let store_id = store.id.to_string();
    database
        .with_connection(|connection| {
            let documents = Repositories::new(connection).vector_documents;
            for document in corpus["documents"].as_array().expect("documents") {
                documents.record_indexed(&VectorDocumentDraft {
                    id: document["id"].as_str().expect("an id").to_owned(),
                    store_id: store_id.clone(),
                    source_path: document["path"].as_str().expect("a path").to_owned(),
                    content_hash: document["key"].as_str().expect("a key").to_owned(),
                    chunk_count: document["chunks"].as_i64().expect("a count"),
                    bytes: 1,
                    indexed_at: 1,
                })?;
            }
            Ok(())
        })
        .expect("records the documents");
    let rows = corpus["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| VectorRow {
            id: row["id"].as_str().expect("an id").to_owned(),
            vector: floats(&row["vector"]),
            payload: json!({ "chunkCount": 2 }),
            document_id: row["documentId"].as_str().expect("a document").to_owned(),
            chunk_index: u32::try_from(row["chunkIndex"].as_u64().expect("an index"))
                .expect("fits"),
            path: row["path"].as_str().expect("a path").to_owned(),
            text: row["text"].as_str().expect("a text").to_owned(),
            section: String::new(),
            page: None,
        })
        .collect();
    let path = stores.store_path(&store_id).expect("a path");
    core.upsert(&path, rows, &CancellationToken::new())
        .await
        .expect("upserts the corpus");
    core.maintain(&path, DEFAULT_ANN_THRESHOLD)
        .await
        .expect("builds the keyword index");
    stores.reconcile(&store_id).await.expect("reconciles");

    let evaluation = VectorEvalService::new(VectorEvalServiceOptions::new(
        database.shared(),
        Arc::new(stores.clone()),
        silent_logger(),
        Arc::new(FakeClock::new(5000)),
    ));
    for case in corpus["cases"].as_array().expect("cases") {
        evaluation
            .add(&AddVectorEvalCaseInput {
                store_id: store.id,
                question: case["question"].as_str().expect("a question").to_owned(),
                expected_document_ids: case["expected"]
                    .as_array()
                    .expect("expected documents")
                    .iter()
                    .map(|id| DocumentId::parse(id.as_str().expect("an id")).expect("a uuid"))
                    .collect(),
            })
            .expect("adds a case");
    }
    evaluation
        .run(
            &VectorEvalRunInput {
                store_id: store.id,
                k: corpus["k"].as_u64(),
            },
            &CancellationToken::new(),
        )
        .await
        .expect("runs the evaluation")
}

pub fn summary(run: &VectorEvalRunDto) -> Value {
    Value::Object(
        run.modes
            .iter()
            .map(|mode| {
                (
                    mode.mode.as_str().to_owned(),
                    json!({
                        "hitAt1": mode.metrics.hit_at1,
                        "hitAt5": mode.metrics.hit_at5,
                        "recallAtK": mode.metrics.recall_at_k,
                        "mrr": mode.metrics.mrr,
                        "ranks": mode.cases.iter().map(|entry| entry.rank).collect::<Vec<_>>(),
                        "fallbacks": mode.fallbacks,
                    }),
                )
            })
            .collect(),
    )
}

fn close(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Number(left), Value::Number(right)) => {
            (left.as_f64().unwrap_or(f64::NAN) - right.as_f64().unwrap_or(f64::NAN)).abs() < 1e-9
        }
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len() && left.iter().zip(right).all(|(a, b)| close(a, b))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .all(|(key, value)| right.get(key).is_some_and(|other| close(value, other)))
        }
        _ => actual == expected,
    }
}

#[test]
fn the_overlap_reranker_counts_shared_words_once() {
    assert!(
        (token_overlap("Ставка НДФЛ, ставка", "Ставка НДФЛ для резидентов") - 2.0).abs() < 1e-12
    );
    assert!(token_overlap("грипп", "Борщ").abs() < 1e-12);
}

#[tokio::test]
async fn the_fixture_corpus_scores_what_the_typescript_host_scored() {
    let run = run_corpus().await;
    let actual = summary(&run);
    let expected = corpus()["expected"].clone();
    assert!(
        close(&actual, &expected),
        "the Rust host scored\n{}\nbut the corpus pins\n{}",
        serde_json::to_string_pretty(&actual).expect("prints"),
        serde_json::to_string_pretty(&expected).expect("prints")
    );
}
