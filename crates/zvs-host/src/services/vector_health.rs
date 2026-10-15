use zvs_contract::{VectorHealth, VectorMetric};

use crate::data::schema::VectorStoreRow;
use crate::drivers::core::{VectorMetric as CoreMetric, VectorStats};

pub const fn core_metric(metric: VectorMetric) -> CoreMetric {
    match metric {
        VectorMetric::Cosine => CoreMetric::Cosine,
        VectorMetric::L2 => CoreMetric::L2,
        VectorMetric::Dot => CoreMetric::Dot,
    }
}

pub const fn contract_metric(metric: CoreMetric) -> VectorMetric {
    match metric {
        CoreMetric::Cosine => VectorMetric::Cosine,
        CoreMetric::L2 => VectorMetric::L2,
        CoreMetric::Dot => VectorMetric::Dot,
    }
}

fn same(stored: i64, observed: impl TryInto<i64>) -> bool {
    observed.try_into().is_ok_and(|observed| observed == stored)
}

pub fn derive_vector_health(
    row: &VectorStoreRow,
    stats: Option<&VectorStats>,
    failed: bool,
) -> VectorHealth {
    if failed {
        return VectorHealth::Broken;
    }
    let Some(stats) = stats else {
        return if row.table_created_at.is_none() && row.status != VectorHealth::Broken {
            VectorHealth::Pending
        } else {
            VectorHealth::Broken
        };
    };
    if !same(row.dimension, stats.dimension) || core_metric(row.metric) != stats.metric {
        return VectorHealth::Broken;
    }
    if !same(row.vectors, stats.row_count)
        || !same(row.documents, stats.document_count)
        || !same(row.bytes, stats.on_disk_bytes)
    {
        return VectorHealth::Stale;
    }
    VectorHealth::Healthy
}

#[cfg(test)]
mod tests {
    use zvs_contract::{VectorBackend, VectorSearchMode};

    use super::*;

    pub(crate) fn row() -> VectorStoreRow {
        VectorStoreRow {
            id: "s1".to_owned(),
            name: "K".to_owned(),
            description: String::new(),
            backend: VectorBackend::Lancedb,
            embedding_provider_id: "p".to_owned(),
            embedding_model_id: "m".to_owned(),
            dimension: 3,
            metric: VectorMetric::Cosine,
            chunk_size: 256,
            chunk_overlap: 32,
            rerank: Default::default(),
            ocr: Default::default(),
            query_prefix: String::new(),
            document_prefix: String::new(),
            search_mode: VectorSearchMode::Hybrid,
            chat_min_score: 0.3,
            schema_version: 1,
            index_type: "FLAT".to_owned(),
            status: VectorHealth::Pending,
            last_eval: None,
            graph_stale: false,
            table_created_at: None,
            last_indexed_at: None,
            documents: 0,
            vectors: 0,
            bytes: 10,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn stats() -> VectorStats {
        VectorStats {
            row_count: 0,
            document_count: 0,
            dimension: 3,
            metric: CoreMetric::Cosine,
            on_disk_bytes: 10,
            index_type: "FLAT".to_owned(),
            schema_version: 2,
            keyword_index: true,
        }
    }

    #[test]
    fn health_distinguishes_uncreated_lost_mismatched_and_corrupt_tables() {
        let row = row();
        assert_eq!(
            derive_vector_health(&row, None, false),
            VectorHealth::Pending
        );
        assert_eq!(
            derive_vector_health(
                &VectorStoreRow {
                    table_created_at: Some(1),
                    ..row.clone()
                },
                None,
                false
            ),
            VectorHealth::Broken
        );
        assert_eq!(
            derive_vector_health(
                &VectorStoreRow {
                    status: VectorHealth::Broken,
                    ..row.clone()
                },
                None,
                false
            ),
            VectorHealth::Broken
        );
        assert_eq!(
            derive_vector_health(&row, Some(&stats()), false),
            VectorHealth::Healthy
        );
        assert_eq!(
            derive_vector_health(
                &row,
                Some(&VectorStats {
                    row_count: 1,
                    ..stats()
                }),
                false
            ),
            VectorHealth::Stale
        );
        assert_eq!(
            derive_vector_health(
                &row,
                Some(&VectorStats {
                    document_count: 1,
                    ..stats()
                }),
                false
            ),
            VectorHealth::Stale
        );
        assert_eq!(
            derive_vector_health(
                &row,
                Some(&VectorStats {
                    dimension: 4,
                    ..stats()
                }),
                false
            ),
            VectorHealth::Broken
        );
        assert_eq!(
            derive_vector_health(
                &row,
                Some(&VectorStats {
                    metric: CoreMetric::Dot,
                    ..stats()
                }),
                false
            ),
            VectorHealth::Broken
        );
        assert_eq!(derive_vector_health(&row, None, true), VectorHealth::Broken);
    }

    #[test]
    fn metrics_map_both_ways() {
        for metric in VectorMetric::ALL {
            assert_eq!(contract_metric(core_metric(*metric)), *metric);
        }
    }
}
