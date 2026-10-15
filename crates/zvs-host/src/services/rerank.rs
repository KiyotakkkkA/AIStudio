use std::cmp::Ordering;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use zvs_contract::validate::js_slice;
use zvs_contract::{AppError, VectorSearchHitDto};

pub const MAX_RERANK_QUERY_CHARS: usize = 1_000;
pub const MAX_RERANK_PASSAGE_CHARS: usize = 2_000;

#[async_trait]
pub trait Reranker: Send + Sync {
    async fn rerank(
        &self,
        query: &str,
        passages: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<f64>, AppError>;
}

pub type SharedReranker = Arc<dyn Reranker>;
pub type RerankerSource = Arc<dyn Fn(&str) -> SharedReranker + Send + Sync>;

#[async_trait]
pub trait LocalRerankEngine: Send + Sync {
    async fn rerank(
        &self,
        model_ref: &str,
        query: &str,
        passages: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<f64>, AppError>;
}

struct BoundReranker {
    engine: Arc<dyn LocalRerankEngine>,
    model_ref: String,
}

#[async_trait]
impl Reranker for BoundReranker {
    async fn rerank(
        &self,
        query: &str,
        passages: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<f64>, AppError> {
        self.engine
            .rerank(&self.model_ref, query, passages, cancel)
            .await
    }
}

pub fn local_rerankers(engine: Arc<dyn LocalRerankEngine>) -> RerankerSource {
    Arc::new(move |model_ref: &str| {
        Arc::new(BoundReranker {
            engine: Arc::clone(&engine),
            model_ref: model_ref.to_owned(),
        }) as SharedReranker
    })
}

pub fn normalise_rerank_score(raw: f64) -> f64 {
    if raw >= 0.0 {
        1.0 / (1.0 + (-raw).exp())
    } else {
        let exp = raw.exp();
        exp / (1.0 + exp)
    }
}

pub fn hit_text(text: &str, payload: &Value) -> String {
    if !text.is_empty() {
        return text.to_owned();
    }
    match payload {
        Value::String(text) => text.clone(),
        Value::Object(entries) => entries
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        _ => String::new(),
    }
}

pub fn passage_of(hit: &VectorSearchHitDto) -> String {
    let text = hit_text(&hit.text, &hit.payload);
    let passage = if text.is_empty() { &hit.path } else { &text };
    js_slice(passage, MAX_RERANK_PASSAGE_CHARS).to_owned()
}

pub fn apply_rerank(hits: &[VectorSearchHitDto], raw_scores: &[f64]) -> Vec<VectorSearchHitDto> {
    let mut reranked: Vec<VectorSearchHitDto> = hits
        .iter()
        .enumerate()
        .map(|(index, hit)| {
            let rerank_score =
                normalise_rerank_score(raw_scores.get(index).copied().unwrap_or(f64::NEG_INFINITY));
            VectorSearchHitDto {
                vector_score: Some(hit.score),
                rerank_score: Some(rerank_score),
                score: rerank_score,
                ..hit.clone()
            }
        })
        .collect();
    reranked.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.id.cmp(&right.id))
    });
    reranked
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;

    use super::*;

    pub(crate) fn hit(id: &str, score: f64, text: &str) -> VectorSearchHitDto {
        VectorSearchHitDto {
            id: id.to_owned(),
            score,
            payload: json!({ "chunkCount": 1 }),
            document_id: id.to_owned(),
            chunk_index: 0,
            path: format!("{id}.md"),
            text: text.to_owned(),
            section: String::new(),
            page: None,
            vector_score: None,
            rerank_score: None,
            keyword_score: None,
            fused_score: None,
            provenance: None,
        }
    }

    fn candidates() -> Vec<VectorSearchHitDto> {
        vec![
            hit("a", 0.9, "about cats"),
            hit("b", 0.8, "about dogs"),
            hit("c", 0.7, "the answer"),
            hit("d", 0.2, "noise"),
        ]
    }

    #[test]
    fn logits_are_squashed_into_the_unit_interval_monotonically_and_without_overflow() {
        assert!((normalise_rerank_score(0.0) - 0.5).abs() < f64::EPSILON);
        assert!((normalise_rerank_score(0.941_786_885_261_535_6) - 0.7195).abs() < 0.0005);
        assert!(normalise_rerank_score(-7.699_778_079_986_572) < 0.001);
        assert!((normalise_rerank_score(1000.0) - 1.0).abs() < f64::EPSILON);
        assert!(normalise_rerank_score(-1000.0).abs() < f64::EPSILON);
        assert!(normalise_rerank_score(2.0) > normalise_rerank_score(1.0));
    }

    #[test]
    fn the_passage_is_the_chunk_text_capped_so_one_long_chunk_cannot_fill_the_context() {
        assert_eq!(passage_of(&hit("a", 1.0, "column text")), "column text");
        assert_eq!(
            passage_of(&VectorSearchHitDto {
                payload: json!("raw string"),
                ..hit("a", 1.0, "")
            }),
            "raw string"
        );
        assert_eq!(
            passage_of(&VectorSearchHitDto {
                payload: json!({ "text": "v1 payload" }),
                ..hit("a", 1.0, "")
            }),
            "v1 payload"
        );
        assert_eq!(
            passage_of(&hit("a", 1.0, &"x".repeat(10_000)))
                .chars()
                .count(),
            MAX_RERANK_PASSAGE_CHARS
        );
        assert_eq!(
            passage_of(&VectorSearchHitDto {
                payload: Value::Null,
                ..hit("a", 1.0, "")
            }),
            "a.md"
        );
    }

    #[test]
    fn reranked_hits_keep_both_scores_and_use_the_rerank_one_as_the_score() {
        let reranked = apply_rerank(&candidates(), &[0.0, 0.0, 3.0, -3.0]);
        assert_eq!(
            reranked
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            ["c", "a", "b", "d"]
        );
        assert_eq!(reranked[0].vector_score, Some(0.7));
        assert_eq!(reranked[0].rerank_score, Some(normalise_rerank_score(3.0)));
        assert_eq!(Some(reranked[0].score), reranked[0].rerank_score);
    }

    #[test]
    fn a_missing_score_sinks_the_hit() {
        let reranked = apply_rerank(&candidates(), &[1.0]);
        assert_eq!(reranked[0].id, "a");
        assert_eq!(reranked[3].rerank_score, Some(0.0));
    }

    struct Engine(Mutex<Vec<(String, String, Vec<String>)>>);

    #[async_trait]
    impl LocalRerankEngine for Engine {
        async fn rerank(
            &self,
            model_ref: &str,
            query: &str,
            passages: &[String],
            _cancel: &CancellationToken,
        ) -> Result<Vec<f64>, AppError> {
            self.0.lock().expect("calls").push((
                model_ref.to_owned(),
                query.to_owned(),
                passages.to_vec(),
            ));
            Ok(vec![1.0])
        }
    }

    #[tokio::test]
    async fn the_local_reranker_binds_the_store_model_to_the_engine_call() {
        let engine = Arc::new(Engine(Mutex::new(Vec::new())));
        let source = local_rerankers(engine.clone());
        let scores = source("curated:model:x")
            .rerank("q", &["p".to_owned()], &CancellationToken::new())
            .await
            .expect("scores");
        assert_eq!(scores, [1.0]);
        assert_eq!(
            engine.0.lock().expect("calls").clone(),
            [(
                "curated:model:x".to_owned(),
                "q".to_owned(),
                vec!["p".to_owned()]
            )]
        );
    }
}
