use std::path::{MAIN_SEPARATOR, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use async_trait::async_trait;
use regex::Regex;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AddVectorEvalCaseInput, AddVectorSourceInput, AppError, CreateVectorStoreInput, DocumentId,
    ProviderId, VectorEvalRunDto, VectorEvalRunInput, VectorIndexInput, VectorIndexReportDto,
    VectorOcrConfig, VectorOcrLanguage,
};
use zvs_host::data::schema::ProviderRow;
use zvs_host::drivers::ai::capabilities::AdapterCapabilities;
use zvs_host::drivers::ai::ports::{AiDriver, DiscoveredModel, EmbeddingDriver};
use zvs_host::drivers::core::{InProcessCore, VectorCore};
use zvs_host::drivers::sidecar::{SidecarDriver, SidecarOptions};
use zvs_host::indexing::{
    DocumentExtractorOptions, ImageExtractor, IndexContext, IndexingService,
    IndexingServiceOptions, OcrPort, PdfExtractor, image_extensions,
};
use zvs_host::platform::clock::FakeClock;
use zvs_host::platform::logger::silent_logger;
use zvs_host::services::providers::DriverSource;
use zvs_host::services::{
    VectorEvalService, VectorEvalServiceOptions, VectorSourceService, VectorSourceServiceOptions,
    VectorStoreService, VectorStoreServiceOptions,
};
use zvs_host::testing::{FAKE_CAPABILITIES, seed_embedding_provider, temporary_database};

fn repository() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|crates| crates.parent())
        .expect("the repository root")
        .to_path_buf()
}

fn spec() -> Value {
    let path = repository().join("test/fixtures/indexing/parity.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("the parity spec"))
        .expect("a JSON spec")
}

static TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{L}\p{N}]+").expect("the token pattern compiles"));

fn hashed(text: &str, dimension: usize) -> Vec<f32> {
    let mut counts = vec![0.0_f64; dimension];
    for token in TOKEN.find_iter(&text.to_lowercase()) {
        let mut hash: u32 = 0x811c_9dc5;
        for byte in token.as_str().bytes() {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(16_777_619);
        }
        counts[usize::try_from(hash).expect("fits") % dimension] += 1.0;
    }
    let norm = counts.iter().map(|value| value * value).sum::<f64>().sqrt();
    if norm == 0.0 {
        counts[0] = 1.0;
        return counts.iter().map(|value| *value as f32).collect();
    }
    counts.iter().map(|value| (value / norm) as f32).collect()
}

struct HashingEmbedder(usize);

#[async_trait]
impl EmbeddingDriver for HashingEmbedder {
    fn capabilities(&self) -> AdapterCapabilities {
        FAKE_CAPABILITIES
    }

    async fn embed(
        &self,
        texts: &[String],
        _model: &str,
        _cancel: &CancellationToken,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        Ok(texts.iter().map(|text| hashed(text, self.0)).collect())
    }

    fn dimensions(&self) -> Option<usize> {
        Some(self.0)
    }

    async fn list_models(
        &self,
        _cancel: &CancellationToken,
    ) -> Result<Vec<DiscoveredModel>, AppError> {
        Ok(Vec::new())
    }
}

struct Drivers(Arc<HashingEmbedder>);

#[async_trait]
impl DriverSource for Drivers {
    async fn ephemeral_driver(&self, _row: &ProviderRow) -> Result<AiDriver, AppError> {
        Ok(AiDriver {
            text: None,
            embedding: Some(self.0.clone()),
            image: None,
        })
    }

    fn invalidate(&self, _provider_id: &str) {}
}

struct AnsweringOcr(Value);

#[async_trait]
impl OcrPort for AnsweringOcr {
    async fn read_image(
        &self,
        _model_ref: &str,
        media_type: &str,
        _bytes: &[u8],
        _language: VectorOcrLanguage,
        _cancel: &CancellationToken,
    ) -> Result<String, AppError> {
        Ok(self.0[media_type].as_str().unwrap_or_default().to_owned())
    }
}

fn relative(path: &str, corpus: &str) -> String {
    path.replace(&format!("{corpus}{MAIN_SEPARATOR}"), "")
        .replace('\\', "/")
}

fn report_summary(report: &VectorIndexReportDto, corpus: &str) -> Value {
    json!({
        "files": report.files,
        "indexed": report.indexed,
        "unchanged": report.unchanged,
        "removed": report.removed,
        "skipped": report.skipped,
        "failed": report.failed,
        "chunks": report.chunks,
        "cancelled": report.cancelled,
        "notes": report.notes.iter().map(|note| relative(note, corpus)).collect::<Vec<_>>(),
    })
}

fn eval_summary(run: &VectorEvalRunDto) -> Value {
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

async fn index_corpus() -> Value {
    let spec = spec();
    let dimension =
        usize::try_from(spec["dimension"].as_u64().expect("a dimension")).expect("fits");
    let corpus = spec["corpus"]
        .as_str()
        .expect("a corpus")
        .split('/')
        .fold(repository(), |path, part| path.join(part));
    let corpus_text = corpus.to_string_lossy().into_owned();
    let database = temporary_database();
    let provider = database
        .with_connection(|connection| Ok(seed_embedding_provider(connection)))
        .expect("seeds a provider");
    let clock = Arc::new(FakeClock::new(1000));
    let drivers: Arc<dyn DriverSource> = Arc::new(Drivers(Arc::new(HashingEmbedder(dimension))));
    let core = InProcessCore::shared();
    let stores = VectorStoreService::new(VectorStoreServiceOptions::new(
        database.shared(),
        core.clone(),
        drivers.clone(),
        database.directory().join("stores"),
        silent_logger(),
        clock.clone(),
    ));
    let sources = VectorSourceService::new(VectorSourceServiceOptions::new(
        database.shared(),
        stores.clone(),
        silent_logger(),
        clock.clone(),
    ));
    let indexing = IndexingService::new(IndexingServiceOptions::new(
        database.shared(),
        core.clone(),
        drivers,
        stores.clone(),
        silent_logger(),
        clock.clone(),
    ));
    let sidecar = SidecarDriver::new(SidecarOptions {
        logger: silent_logger(),
        ping_interval: Duration::from_secs(60),
        ..SidecarOptions::new(PathBuf::from(env!("CARGO_BIN_EXE_zvs-jobd")))
    });
    let documents = DocumentExtractorOptions {
        jobs: Arc::new(sidecar.clone()),
        ocr: Some(Arc::new(AnsweringOcr(spec["ocrAnswers"].clone()))),
        scratch_dir: database.directory().join("scratch"),
        logger: silent_logger(),
    };
    indexing
        .registry()
        .register(&[".pdf"], Arc::new(PdfExtractor::new(documents.clone())))
        .register(
            &image_extensions(),
            Arc::new(ImageExtractor::new(documents)),
        );

    let ocr = &spec["ocr"];
    let store = stores
        .create(&CreateVectorStoreInput {
            dimension: spec["dimension"].as_u64(),
            chunk_size: spec["chunkSize"].as_u64(),
            chunk_overlap: spec["chunkOverlap"].as_u64(),
            ocr: Some(VectorOcrConfig {
                enabled: ocr["enabled"].as_bool().expect("a flag"),
                model_ref: ocr["modelRef"].as_str().expect("a model").to_owned(),
                language: VectorOcrLanguage::Rus,
                min_chars_per_page: ocr["minCharsPerPage"].as_u64().expect("a threshold"),
            }),
            ..CreateVectorStoreInput::new(
                "Parity",
                ProviderId::parse(&provider).expect("an id"),
                "parity-hashing",
            )
        })
        .await
        .expect("creates the store");
    let store_id = store.id.to_string();
    sources
        .add_source(&AddVectorSourceInput::new(store.id, corpus_text.clone()))
        .expect("adds the corpus");

    let report = indexing
        .index(
            &VectorIndexInput::new(store.id, false),
            IndexContext::default(),
        )
        .await
        .expect("indexes the corpus");

    let listed = sources.list_documents(&store_id).expect("lists documents");
    let mut documents: Vec<(String, u64, String)> = listed
        .iter()
        .map(|document| {
            (
                relative(&document.source_path, &corpus_text),
                document.chunk_count,
                document.id.to_string(),
            )
        })
        .collect();
    documents.sort();
    let path = stores.store_path(&store_id).expect("a path");
    let mut rows = Vec::new();
    for (relative_path, chunk_count, id) in &documents {
        let indices: Vec<u32> = (0..u32::try_from(*chunk_count).expect("fits")).collect();
        for chunk in core
            .chunks(&path, id, &indices)
            .await
            .expect("reads chunks")
        {
            rows.push(json!({
                "path": relative_path,
                "chunkIndex": chunk.chunk_index,
                "section": chunk.section,
                "page": chunk.page,
                "text": chunk.text,
            }));
        }
    }

    let evaluation = VectorEvalService::new(VectorEvalServiceOptions::new(
        database.shared(),
        Arc::new(stores.clone()),
        silent_logger(),
        Arc::new(FakeClock::new(5000)),
    ));
    let ids: std::collections::HashMap<&str, &str> = documents
        .iter()
        .map(|(relative_path, _, id)| (relative_path.as_str(), id.as_str()))
        .collect();
    for case in spec["cases"].as_array().expect("cases") {
        evaluation
            .add(&AddVectorEvalCaseInput {
                store_id: store.id,
                question: case["question"].as_str().expect("a question").to_owned(),
                expected_document_ids: case["expected"]
                    .as_array()
                    .expect("expected documents")
                    .iter()
                    .map(|name| {
                        DocumentId::parse(ids[name.as_str().expect("a name")]).expect("an id")
                    })
                    .collect(),
            })
            .expect("adds a case");
    }
    let run = evaluation
        .run(
            &VectorEvalRunInput {
                store_id: store.id,
                k: spec["k"].as_u64(),
            },
            &CancellationToken::new(),
        )
        .await
        .expect("runs the evaluation");
    sidecar.dispose().await;

    json!({
        "report": report_summary(&report, &corpus_text),
        "documents": documents
            .iter()
            .map(|(relative_path, chunk_count, _)| json!({ "path": relative_path, "chunkCount": chunk_count }))
            .collect::<Vec<_>>(),
        "rows": rows,
        "eval": eval_summary(&run),
    })
}

#[tokio::test]
async fn the_mixed_corpus_indexes_to_what_the_typescript_host_indexed() {
    let actual = index_corpus().await;
    let expected = spec()["expected"].clone();
    assert!(
        close(&actual, &expected),
        "the Rust host produced\n{}\nbut the spec pins\n{}",
        serde_json::to_string_pretty(&actual).expect("prints"),
        serde_json::to_string_pretty(&expected).expect("prints")
    );
}
