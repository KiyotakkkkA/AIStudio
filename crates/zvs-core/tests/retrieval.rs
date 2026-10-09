use std::sync::Arc;

use arrow_array::{
    FixedSizeListArray, RecordBatch, RecordBatchIterator, StringArray, UInt32Array,
    types::Float32Type,
};
use arrow_schema::{DataType, Field, Schema};
use zvs_core::{
    CancellationToken, Error,
    index::{
        ANN_INDEX_TYPE, Metric, Provenance, RRF_K, SCHEMA_VERSION, VectorIndex, VectorRow,
        reciprocal_rank_fusion,
    },
};

fn row(id: &str, document: &str, chunk: u32, vector: Vec<f32>, text: &str) -> VectorRow {
    VectorRow {
        id: id.into(),
        vector,
        payload: serde_json::json!({ "chunkCount": 3 }),
        document_id: document.into(),
        chunk_index: chunk,
        path: format!("C:/docs/{document}.md"),
        text: text.into(),
        section: "Раздел › Пункт".into(),
        page: if chunk == 0 { None } else { Some(chunk) },
    }
}

fn ids<T: AsRef<str>>(values: impl IntoIterator<Item = T>) -> Vec<String> {
    values.into_iter().map(|v| v.as_ref().to_owned()).collect()
}

#[test]
fn rrf_matches_hand_computed_scores() {
    let fused = reciprocal_rank_fusion(&["a", "b", "c"], &["c", "d", "a"], RRF_K);
    let order: Vec<_> = fused.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(order, ["a", "c", "d", "b"]);
    let a = 1.0 / 61.0 + 1.0 / 63.0;
    let c = 1.0 / 63.0 + 1.0 / 61.0;
    assert!((fused[0].score - a).abs() < 1e-7);
    assert!((fused[1].score - c).abs() < 1e-7);
    assert!((fused[2].score - 1.0 / 62.0).abs() < 1e-7);
    assert!((fused[3].score - 1.0 / 62.0).abs() < 1e-7);
    assert_eq!(fused[0].provenance(), Provenance::Both);
    assert_eq!(fused[2].provenance(), Provenance::Keyword);
    assert_eq!(fused[3].provenance(), Provenance::Vector);
    assert_eq!(fused[0].vector_rank, Some(1));
    assert_eq!(fused[0].keyword_rank, Some(3));
}

#[test]
fn rrf_edge_cases() {
    assert!(reciprocal_rank_fusion(&[], &[], RRF_K).is_empty());
    let only = reciprocal_rank_fusion(&[], &["x", "y"], RRF_K);
    assert_eq!(ids(only.iter().map(|e| &e.id)), ["x", "y"]);
    let duplicated = reciprocal_rank_fusion(&["x", "x"], &[], RRF_K);
    assert_eq!(duplicated.len(), 1);
    assert!((duplicated[0].score - 1.0 / 61.0).abs() < 1e-7);
    let tie = reciprocal_rank_fusion(&["a"], &["z"], RRF_K);
    assert_eq!(ids(tie.iter().map(|e| &e.id)), ["z", "a"]);
}

#[tokio::test]
async fn v2_columns_round_trip_and_chunks_by_index() {
    let temp = tempfile::tempdir().unwrap();
    let index = VectorIndex::create(temp.path(), 2, Metric::Cosine)
        .await
        .unwrap();
    let stats = index.stats().await.unwrap();
    assert_eq!(stats.schema_version, SCHEMA_VERSION);
    assert!(stats.keyword_index);
    let rows = [
        row("d:0", "d", 0, vec![1.0, 0.0], "Первый фрагмент."),
        row("d:1", "d", 1, vec![0.9, 0.1], "Второй фрагмент."),
        row("d:2", "d", 2, vec![0.8, 0.2], "Третий фрагмент."),
        row("e:1", "e", 1, vec![0.0, 1.0], "Чужой фрагмент."),
    ];
    index
        .upsert(&rows, &CancellationToken::new())
        .await
        .unwrap();
    let hit = &index.search(&[1.0, 0.0], 1, 0.0, None).await.unwrap()[0];
    assert_eq!(hit.text, "Первый фрагмент.");
    assert_eq!(hit.section, "Раздел › Пункт");
    assert_eq!(hit.page, None);
    let neighbours = index.chunks("d", &[2, 1, 7]).await.unwrap();
    assert_eq!(
        neighbours
            .iter()
            .map(|chunk| chunk.chunk_index)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(neighbours[0].text, "Второй фрагмент.");
    assert_eq!(neighbours[0].page, Some(1));
    assert!(index.chunks("d", &[]).await.unwrap().is_empty());
    assert!(index.chunks("x' OR '1'='1", &[1]).await.unwrap().is_empty());
}

#[tokio::test]
async fn keyword_search_finds_exact_codes_and_follows_upserts_and_deletes() {
    let temp = tempfile::tempdir().unwrap();
    let index = VectorIndex::create(temp.path(), 2, Metric::Cosine)
        .await
        .unwrap();
    let token = CancellationToken::new();
    index
        .upsert(
            &[
                row("a", "a", 0, vec![1.0, 0.0], "Общие положения о договоре."),
                row(
                    "b",
                    "b",
                    0,
                    vec![0.0, 1.0],
                    "Ошибка E-4512 возникает при сбое питания.",
                ),
                row("c", "c", 0, vec![0.7, 0.7], "Статья 228 описывает порядок."),
            ],
            &token,
        )
        .await
        .unwrap();
    let found = index.keyword_search("E-4512", 5, None).await.unwrap();
    assert_eq!(found[0].id, "b");
    assert!(found[0].keyword_score.unwrap() > 0.0);
    let found = index.keyword_search("договоров", 5, None).await.unwrap();
    assert_eq!(ids(found.iter().map(|h| &h.id)), ["a"]);
    assert!(
        index
            .keyword_search("   ", 5, None)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        index.keyword_search("x", 0, None).await,
        Err(Error::InvalidInput(_))
    ));

    index
        .upsert(
            &[row(
                "d",
                "d",
                0,
                vec![0.5, 0.5],
                "Новый код Z-9001 в журнале.",
            )],
            &token,
        )
        .await
        .unwrap();
    assert_eq!(
        index.keyword_search("Z-9001", 5, None).await.unwrap()[0].id,
        "d"
    );
    index.maintain(0).await.unwrap();
    assert_eq!(
        index.keyword_search("Z-9001", 5, None).await.unwrap()[0].id,
        "d"
    );
    index.delete_by_source("d").await.unwrap();
    assert!(
        index
            .keyword_search("Z-9001", 5, None)
            .await
            .unwrap()
            .is_empty()
    );
    let filtered = index
        .keyword_search("E-4512", 5, Some("document_id = 'a'"))
        .await
        .unwrap();
    assert!(filtered.is_empty());
}

#[tokio::test]
async fn hybrid_ranks_an_exact_identifier_first() {
    let temp = tempfile::tempdir().unwrap();
    let index = VectorIndex::create(temp.path(), 2, Metric::Cosine)
        .await
        .unwrap();
    let mut rows: Vec<_> = (0..20)
        .map(|i| {
            let angle = i as f32 * 0.02;
            row(
                &format!("n{i:02}"),
                &format!("n{i:02}"),
                0,
                vec![angle.cos(), angle.sin()],
                "Порядок подачи заявления в общем случае.",
            )
        })
        .collect();
    rows.push(row(
        "code",
        "code",
        0,
        vec![0.0, 1.0],
        "Форма КНД-1151020 подаётся отдельно.",
    ));
    index
        .upsert(&rows, &CancellationToken::new())
        .await
        .unwrap();
    let query = [1.0, 0.0];
    let vector = index.search(&query, 10, 0.0, None).await.unwrap();
    assert!(vector.iter().all(|hit| hit.id != "code"));
    let hybrid = index
        .hybrid_search(&query, "КНД-1151020", 10, 0.5, None)
        .await
        .unwrap();
    assert_eq!(hybrid.len(), 10);
    assert_eq!(hybrid[0].id, "code");
    assert_eq!(hybrid[0].provenance, Some(Provenance::Keyword));
    assert!(hybrid[0].score < 0.5);
    assert!(hybrid[0].keyword_score.unwrap() > 0.0);
    assert!(hybrid[0].fused_score.unwrap() > 0.0);
    assert_eq!(hybrid[0].text, "Форма КНД-1151020 подаётся отдельно.");
    assert!(
        hybrid[1..]
            .iter()
            .all(|hit| hit.provenance == Some(Provenance::Vector))
    );
    let both = index
        .hybrid_search(&query, "порядок подачи", 3, 0.0, None)
        .await
        .unwrap();
    assert!(
        both.iter()
            .all(|hit| hit.provenance == Some(Provenance::Both))
    );
}

#[tokio::test]
async fn ann_index_is_built_only_past_the_threshold() {
    let temp = tempfile::tempdir().unwrap();
    let index = VectorIndex::create(temp.path(), 4, Metric::Cosine)
        .await
        .unwrap();
    let rows: Vec<_> = (0..600)
        .map(|i| {
            let a = i as f32 * 0.01;
            row(
                &format!("r{i}"),
                &format!("doc{}", i % 7),
                i as u32,
                vec![a.cos(), a.sin(), (a * 2.0).cos(), 0.5],
                "текст",
            )
        })
        .collect();
    index
        .upsert(&rows, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(index.maintain(1_000).await.unwrap().index_type, "FLAT");
    let stats = index.maintain(500).await.unwrap();
    assert_eq!(stats.index_type, ANN_INDEX_TYPE);
    assert!(stats.keyword_index);
    let hits = index
        .search(&[1.0, 0.0, 1.0, 0.5], 5, 0.0, None)
        .await
        .unwrap();
    assert_eq!(hits.len(), 5);
    assert_eq!(hits[0].id, "r0");
    let reopened = VectorIndex::open(temp.path()).await.unwrap();
    assert_eq!(reopened.stats().await.unwrap().index_type, ANN_INDEX_TYPE);
    assert_eq!(reopened.maintain(500).await.unwrap().row_count, 600);
}

#[tokio::test]
async fn a_v1_table_is_read_only_and_searchable() {
    let temp = tempfile::tempdir().unwrap();
    let schema = Arc::new(Schema::new_with_metadata(
        vec![
            Field::new("id", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 2),
                false,
            ),
            Field::new("payload", DataType::Utf8, false),
            Field::new("document_id", DataType::Utf8, false),
            Field::new("chunk_index", DataType::UInt32, false),
            Field::new("path", DataType::Utf8, false),
        ],
        [("zvs.metric".to_owned(), "\"cosine\"".to_owned())].into(),
    ));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(StringArray::from(vec!["old"])),
            Arc::new(
                FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                    [Some([Some(1.0), Some(0.0)])],
                    2,
                ),
            ),
            Arc::new(StringArray::from(vec![r#"{"text":"старый текст"}"#])),
            Arc::new(StringArray::from(vec!["doc"])),
            Arc::new(UInt32Array::from(vec![0])),
            Arc::new(StringArray::from(vec!["C:/old.txt"])),
        ],
    )
    .unwrap();
    let db = lancedb::connect(temp.path().to_str().unwrap())
        .execute()
        .await
        .unwrap();
    db.create_table(
        "vectors",
        Box::new(RecordBatchIterator::new(vec![Ok(batch)], schema))
            as Box<dyn arrow_array::RecordBatchReader + Send>,
    )
    .execute()
    .await
    .unwrap();

    let index = VectorIndex::open(temp.path()).await.unwrap();
    assert_eq!(index.schema_version(), 1);
    let stats = index.stats().await.unwrap();
    assert_eq!(stats.schema_version, 1);
    assert!(!stats.keyword_index);
    let hit = &index.search(&[1.0, 0.0], 1, 0.0, None).await.unwrap()[0];
    assert_eq!(hit.text, "старый текст");
    assert_eq!(hit.section, "");
    assert!(
        index
            .keyword_search("старый", 5, None)
            .await
            .unwrap()
            .is_empty()
    );
    let hybrid = index
        .hybrid_search(&[1.0, 0.0], "старый", 5, 0.0, None)
        .await
        .unwrap();
    assert_eq!(hybrid[0].provenance, Some(Provenance::Vector));
    assert_eq!(
        index.chunks("doc", &[0]).await.unwrap()[0].text,
        "старый текст"
    );
    assert!(matches!(
        index
            .upsert(
                &[row("new", "doc", 0, vec![1.0, 0.0], "x")],
                &CancellationToken::new()
            )
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(index.maintain(1).await.unwrap().schema_version, 1);
    assert!(
        VectorIndex::create(temp.path(), 2, Metric::Cosine)
            .await
            .is_ok()
    );
    VectorIndex::remove(temp.path()).unwrap();
    let rebuilt = VectorIndex::create(temp.path(), 2, Metric::Cosine)
        .await
        .unwrap();
    assert_eq!(rebuilt.schema_version(), SCHEMA_VERSION);
}
