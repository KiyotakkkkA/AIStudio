use std::collections::HashSet;

use zvs_contract::VectorEvalMetricsDto;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaseScore {
    pub rank: Option<u64>,
    pub found: u64,
    pub expected: u64,
}

pub fn ranked_documents<'a>(document_ids: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut seen = HashSet::new();
    document_ids
        .into_iter()
        .filter(|id| seen.insert(*id))
        .map(str::to_owned)
        .collect()
}

pub fn score_case(ranked: &[String], expected: &[String], k: usize) -> CaseScore {
    let wanted: HashSet<&str> = expected.iter().map(String::as_str).collect();
    let top = &ranked[..ranked.len().min(k)];
    let position = top.iter().position(|id| wanted.contains(id.as_str()));
    let found = top.iter().filter(|id| wanted.contains(id.as_str())).count();
    CaseScore {
        rank: position.map(|index| index as u64 + 1),
        found: found as u64,
        expected: wanted.len() as u64,
    }
}

pub fn retrieval_metrics(scores: &[CaseScore]) -> VectorEvalMetricsDto {
    let cases = scores.len();
    if cases == 0 {
        return VectorEvalMetricsDto {
            cases: 0,
            hit_at1: 0.0,
            hit_at5: 0.0,
            recall_at_k: 0.0,
            mrr: 0.0,
        };
    }
    let mut hit_at1 = 0_u32;
    let mut hit_at5 = 0_u32;
    let mut recall = 0.0;
    let mut reciprocal = 0.0;
    for score in scores {
        if let Some(rank) = score.rank {
            if rank <= 1 {
                hit_at1 += 1;
            }
            if rank <= 5 {
                hit_at5 += 1;
            }
            reciprocal += 1.0 / rank as f64;
        }
        recall += if score.expected == 0 {
            0.0
        } else {
            (score.found as f64 / score.expected as f64).min(1.0)
        };
    }
    let total = cases as f64;
    VectorEvalMetricsDto {
        cases: cases as u64,
        hit_at1: f64::from(hit_at1) / total,
        hit_at5: f64::from(hit_at5) / total,
        recall_at_k: recall / total,
        mrr: reciprocal / total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    fn close(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-12
    }

    #[test]
    fn chunk_hits_collapse_to_documents_in_first_seen_order() {
        assert_eq!(ranked_documents(["a", "b", "a", "c", "b"]), ["a", "b", "c"]);
    }

    #[test]
    fn a_case_is_scored_by_the_rank_of_its_first_expected_document_within_k() {
        let ranked = owned(&["x", "y", "a", "b"]);
        assert_eq!(
            score_case(&ranked, &owned(&["b", "a"]), 10),
            CaseScore {
                rank: Some(3),
                found: 2,
                expected: 2
            }
        );
        assert_eq!(
            score_case(&ranked, &owned(&["b", "a"]), 3),
            CaseScore {
                rank: Some(3),
                found: 1,
                expected: 2
            }
        );
        assert_eq!(
            score_case(&owned(&["x", "y"]), &owned(&["a"]), 10),
            CaseScore {
                rank: None,
                found: 0,
                expected: 1
            }
        );
    }

    #[test]
    fn duplicate_expected_documents_count_once() {
        assert_eq!(
            score_case(&owned(&["a"]), &owned(&["a", "a"]), 5),
            CaseScore {
                rank: Some(1),
                found: 1,
                expected: 1
            }
        );
    }

    #[test]
    fn aggregate_metrics_follow_their_textbook_definitions() {
        let score = |rank, found, expected| CaseScore {
            rank,
            found,
            expected,
        };
        let metrics = retrieval_metrics(&[
            score(Some(1), 1, 1),
            score(Some(2), 1, 2),
            score(Some(5), 2, 2),
            score(Some(6), 1, 1),
            score(None, 0, 1),
        ]);
        assert_eq!(metrics.cases, 5);
        assert!(close(metrics.hit_at1, 1.0 / 5.0));
        assert!(close(metrics.hit_at5, 3.0 / 5.0));
        assert!(close(metrics.recall_at_k, (1.0 + 0.5 + 1.0 + 1.0) / 5.0));
        assert!(close(
            metrics.mrr,
            (1.0 + 1.0 / 2.0 + 1.0 / 5.0 + 1.0 / 6.0) / 5.0
        ));
    }

    #[test]
    fn an_empty_set_scores_zero_rather_than_dividing_by_it() {
        assert_eq!(
            retrieval_metrics(&[]),
            VectorEvalMetricsDto {
                cases: 0,
                hit_at1: 0.0,
                hit_at5: 0.0,
                recall_at_k: 0.0,
                mrr: 0.0,
            }
        );
    }

    #[test]
    fn a_better_ranking_moves_every_metric_the_right_way() {
        let before = retrieval_metrics(&[
            score_case(&owned(&["x", "y", "z", "q", "w", "a"]), &owned(&["a"]), 10),
            score_case(&owned(&["x", "b"]), &owned(&["b"]), 10),
        ]);
        let after = retrieval_metrics(&[
            score_case(&owned(&["a", "x"]), &owned(&["a"]), 10),
            score_case(&owned(&["b", "x"]), &owned(&["b"]), 10),
        ]);
        assert!(after.mrr > before.mrr);
        assert!(after.hit_at1 > before.hit_at1);
        assert!(after.hit_at5 > before.hit_at5);
        assert!(close(after.mrr, 1.0));
    }
}
