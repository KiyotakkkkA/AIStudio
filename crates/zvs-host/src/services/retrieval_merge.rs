use std::cmp::Ordering;
use std::collections::HashMap;

use zvs_contract::{VectorSearchHitDto, below_relevance_floor};

pub const CHAT_FETCH_K: u64 = 20;
pub const CHAT_SOURCES_MAX: usize = 6;
pub const CHAT_CHUNKS_PER_DOCUMENT: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct StoreHits {
    pub store_id: String,
    pub hits: Vec<VectorSearchHitDto>,
    pub floor: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergeOptions {
    pub limit: usize,
    pub per_document: usize,
}

impl Default for MergeOptions {
    fn default() -> Self {
        Self {
            limit: CHAT_SOURCES_MAX,
            per_document: CHAT_CHUNKS_PER_DOCUMENT,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MergedHit {
    pub store_id: String,
    pub hit: VectorSearchHitDto,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MergeStats {
    pub fetched: usize,
    pub below_floor: usize,
    pub duplicates: usize,
    pub over_cap: usize,
    pub over_limit: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Merged {
    pub hits: Vec<MergedHit>,
    pub stats: MergeStats,
}

struct Candidate<'a> {
    store_id: &'a str,
    hit: &'a VectorSearchHitDto,
    key: f64,
    store: usize,
    rank: usize,
}

pub fn merge_retrieval(hits_by_store: &[StoreHits], options: MergeOptions) -> Merged {
    let mut stats = MergeStats::default();
    let mut candidates = Vec::new();
    for (store, entry) in hits_by_store.iter().enumerate() {
        stats.fetched += entry.hits.len();
        let kept: Vec<&VectorSearchHitDto> = entry
            .hits
            .iter()
            .filter(|hit| !below_relevance_floor(hit.score, hit.provenance, entry.floor))
            .collect();
        stats.below_floor += entry.hits.len() - kept.len();
        let mut keys: Vec<f64> = kept.iter().map(|hit| hit.score).collect();
        let mut key: f64 = 0.0;
        for slot in keys.iter_mut().rev() {
            key = key.max(*slot);
            *slot = key;
        }
        candidates.extend(
            kept.into_iter()
                .zip(keys)
                .enumerate()
                .map(|(rank, (hit, key))| Candidate {
                    store_id: &entry.store_id,
                    hit,
                    key,
                    store,
                    rank,
                }),
        );
    }
    candidates.sort_by(|left, right| {
        right
            .key
            .partial_cmp(&left.key)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.rank.cmp(&right.rank))
            .then_with(|| left.store.cmp(&right.store))
    });

    let mut taken: HashMap<(&str, &str), Vec<u64>> = HashMap::new();
    let mut hits = Vec::new();
    for candidate in candidates {
        let indices = taken
            .entry((candidate.store_id, candidate.hit.document_id.as_str()))
            .or_default();
        if indices
            .iter()
            .any(|index| index.abs_diff(candidate.hit.chunk_index) <= 1)
        {
            stats.duplicates += 1;
            continue;
        }
        if indices.len() >= options.per_document {
            stats.over_cap += 1;
            continue;
        }
        if hits.len() >= options.limit {
            stats.over_limit += 1;
            continue;
        }
        indices.push(candidate.hit.chunk_index);
        hits.push(MergedHit {
            store_id: candidate.store_id.to_owned(),
            hit: candidate.hit.clone(),
        });
    }
    Merged { hits, stats }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zvs_contract::VectorHitProvenance;

    use super::*;

    const STORE_A: &str = "0199bb11-1111-7111-8111-00000000000a";
    const STORE_B: &str = "0199bb11-1111-7111-8111-00000000000b";

    fn hit(document_id: &str, chunk_index: u64, score: f64) -> VectorSearchHitDto {
        VectorSearchHitDto {
            id: format!("{document_id}:{chunk_index}"),
            score,
            payload: json!({}),
            document_id: document_id.to_owned(),
            chunk_index,
            path: format!("docs/{document_id}.md"),
            text: format!("{document_id} {chunk_index}"),
            section: String::new(),
            page: None,
            vector_score: None,
            rerank_score: None,
            keyword_score: None,
            fused_score: None,
            provenance: None,
        }
    }

    fn keyword(document_id: &str, chunk_index: u64, score: f64) -> VectorSearchHitDto {
        VectorSearchHitDto {
            provenance: Some(VectorHitProvenance::Keyword),
            ..hit(document_id, chunk_index, score)
        }
    }

    fn store(store_id: &str, hits: Vec<VectorSearchHitDto>, floor: f64) -> StoreHits {
        StoreHits {
            store_id: store_id.to_owned(),
            hits,
            floor,
        }
    }

    fn ids(merged: &Merged) -> Vec<&str> {
        merged
            .hits
            .iter()
            .map(|entry| entry.hit.id.as_str())
            .collect()
    }

    #[test]
    fn merge_orders_across_stores_by_score_so_a_stronger_second_store_comes_first() {
        let merged = merge_retrieval(
            &[
                store(STORE_A, vec![hit("a", 0, 0.5), hit("b", 0, 0.4)], 0.3),
                store(STORE_B, vec![hit("c", 0, 0.9), hit("d", 0, 0.45)], 0.3),
            ],
            MergeOptions::default(),
        );
        assert_eq!(ids(&merged), ["c:0", "a:0", "d:0", "b:0"]);
        assert_eq!(merged.hits[0].store_id, STORE_B);
    }

    #[test]
    fn merge_drops_hits_below_each_store_floor_except_keyword_only_hits() {
        let merged = merge_retrieval(
            &[
                store(
                    STORE_A,
                    vec![hit("a", 0, 0.5), hit("b", 0, 0.1), keyword("k", 0, 0.05)],
                    0.3,
                ),
                store(STORE_B, vec![hit("c", 0, 0.5)], 0.6),
            ],
            MergeOptions::default(),
        );
        assert_eq!(ids(&merged), ["a:0", "k:0"]);
        assert_eq!(merged.stats.fetched, 4);
        assert_eq!(merged.stats.below_floor, 2);
    }

    #[test]
    fn an_off_topic_turn_merges_to_nothing() {
        let merged = merge_retrieval(
            &[store(
                STORE_A,
                vec![hit("a", 0, 0.12), hit("b", 0, 0.08)],
                0.3,
            )],
            MergeOptions::default(),
        );
        assert!(merged.hits.is_empty());
        assert_eq!(
            merged.stats,
            MergeStats {
                fetched: 2,
                below_floor: 2,
                duplicates: 0,
                over_cap: 0,
                over_limit: 0,
            }
        );
    }

    #[test]
    fn merge_keeps_the_store_order_lifting_fused_hits_to_the_score_below_them() {
        let merged = merge_retrieval(
            &[
                store(
                    STORE_A,
                    vec![keyword("code", 0, 0.2), hit("a", 0, 0.7)],
                    0.3,
                ),
                store(STORE_B, vec![hit("b", 0, 0.6)], 0.3),
            ],
            MergeOptions::default(),
        );
        assert_eq!(ids(&merged), ["code:0", "a:0", "b:0"]);
    }

    #[test]
    fn merge_collapses_adjacent_chunks_of_one_document_and_caps_chunks_per_document() {
        let merged = merge_retrieval(
            &[
                store(
                    STORE_A,
                    vec![
                        hit("a", 4, 0.9),
                        hit("a", 5, 0.85),
                        hit("a", 3, 0.8),
                        hit("a", 8, 0.75),
                        hit("a", 12, 0.7),
                        hit("a", 20, 0.65),
                        hit("b", 4, 0.6),
                    ],
                    0.3,
                ),
                store(STORE_B, vec![hit("a", 5, 0.88)], 0.3),
            ],
            MergeOptions {
                per_document: 3,
                ..MergeOptions::default()
            },
        );
        assert_eq!(
            merged
                .hits
                .iter()
                .map(|entry| (entry.store_id.as_str(), entry.hit.id.as_str()))
                .collect::<Vec<_>>(),
            [
                (STORE_A, "a:4"),
                (STORE_B, "a:5"),
                (STORE_A, "a:8"),
                (STORE_A, "a:12"),
                (STORE_A, "b:4"),
            ]
        );
        assert_eq!(merged.stats.duplicates, 2);
        assert_eq!(merged.stats.over_cap, 1);
    }

    #[test]
    fn merge_keeps_the_top_n() {
        let hits = (0..10)
            .map(|index| hit(&format!("d{index}"), 0, 0.9 - f64::from(index) / 100.0))
            .collect();
        let merged = merge_retrieval(
            &[store(STORE_A, hits, 0.3)],
            MergeOptions {
                limit: 6,
                ..MergeOptions::default()
            },
        );
        assert_eq!(merged.hits.len(), 6);
        assert_eq!(merged.stats.over_limit, 4);
    }
}
