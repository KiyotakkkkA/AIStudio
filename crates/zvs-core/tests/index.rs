use std::path::Path;
use zvs_core::{
    CancellationToken, Error,
    index::{Metric, UPSERT_BATCH_SIZE, VectorIndex, VectorRow},
};

fn row(id: &str, document: &str, vector: Vec<f32>) -> VectorRow {
    VectorRow {
        id: id.into(),
        vector,
        payload: serde_json::json!({"text": format!("chunk {id}")}),
        document_id: document.into(),
        chunk_index: 7,
        path: "C:/docs/Привет.txt".into(),
    }
}

fn bytes(path: &Path) -> u64 {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                bytes(&path)
            } else {
                path.metadata().unwrap().len()
            }
        })
        .sum()
}

#[tokio::test]
async fn round_trip_upsert_filter_delete_and_stats() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("host-store");
    let index = VectorIndex::create(&path, 2, Metric::Cosine).await.unwrap();
    let empty = index.stats().await.unwrap();
    assert_eq!(empty.row_count, 0);
    assert_eq!(empty.dimension, 2);
    assert_eq!(empty.index_type, "FLAT");
    let token = CancellationToken::new();
    let rows = [
        row("a", "doc'1", vec![1.0, 0.0]),
        row("b", "doc'1", vec![0.8, 0.6]),
        row("c", "doc2", vec![0.0, 1.0]),
    ];
    assert_eq!(index.upsert(&rows, &token).await.unwrap(), 3);
    let hits = index.search(&[1.0, 0.0], 3, 0.7, None).await.unwrap();
    assert_eq!(
        hits.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert!((hits[0].score - 1.0).abs() < 1e-6);
    assert!((hits[1].score - 0.8).abs() < 1e-6);
    assert_eq!(hits[0].payload, rows[0].payload);
    assert_eq!(hits[0].document_id, "doc'1");
    assert_eq!(hits[0].chunk_index, 7);
    assert_eq!(hits[0].path, rows[0].path);
    let filtered = index
        .search(&[1.0, 0.0], 1, 0.0, Some("document_id = 'doc2'"))
        .await
        .unwrap();
    assert_eq!(filtered[0].id, "c");
    index
        .upsert(&[row("a", "changed", vec![0.0, 1.0])], &token)
        .await
        .unwrap();
    assert_eq!(index.stats().await.unwrap().row_count, 3);
    assert_eq!(
        index.search(&[1.0, 0.0], 1, 0.0, None).await.unwrap()[0].id,
        "b"
    );
    index.delete_by_source("doc'1").await.unwrap();
    assert_eq!(index.stats().await.unwrap().row_count, 2);
    index
        .delete_by_ids(&["c".into(), "x') OR true --".into()])
        .await
        .unwrap();
    index.delete_by_ids(&[]).await.unwrap();
    let reopened = VectorIndex::open(&path).await.unwrap();
    let stats = reopened.stats().await.unwrap();
    assert_eq!(stats.row_count, 1);
    assert_eq!(stats.metric, Metric::Cosine);
    assert_eq!(stats.on_disk_bytes, bytes(&path.join("vectors.lance")));
    assert!(stats.on_disk_bytes > 0);
    assert_eq!(
        reopened.search(&[0.0, 1.0], 10, 0.5, None).await.unwrap()[0].id,
        "a"
    );
    assert!(VectorIndex::create(&path, 2, Metric::Cosine).await.is_ok());
}

#[tokio::test]
async fn rejects_mismatches_and_invalid_inputs_before_writing() {
    let temp = tempfile::tempdir().unwrap();
    let index = VectorIndex::create(temp.path(), 2, Metric::Cosine)
        .await
        .unwrap();
    assert!(matches!(
        VectorIndex::create(temp.path(), 3, Metric::Cosine).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        VectorIndex::create(temp.path(), 2, Metric::L2).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(
        VectorIndex::create(temp.path(), 0, Metric::Cosine)
            .await
            .is_err()
    );
    assert!(
        VectorIndex::create(Path::new("relative"), 2, Metric::Cosine)
            .await
            .is_err()
    );
    for vector in [
        vec![1.0],
        vec![f32::NAN, 0.0],
        vec![0.0, 0.0],
        vec![f32::MAX, 1.0],
    ] {
        let rows = [
            row("ok", "doc", vec![1.0, 0.0]),
            row("bad", "doc", vector.clone()),
        ];
        assert!(matches!(
            index.upsert(&rows, &CancellationToken::new()).await,
            Err(Error::InvalidInput(_))
        ));
        assert!(index.search(&vector, 1, 0.0, None).await.is_err());
    }
    let duplicate = row("same", "doc", vec![1.0, 0.0]);
    assert!(
        index
            .upsert(&[duplicate.clone(), duplicate], &CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(index.stats().await.unwrap().row_count, 0);
    assert!(index.search(&[1.0, 0.0], 0, 0.0, None).await.is_err());
    assert!(index.search(&[1.0, 0.0], 1, f32::NAN, None).await.is_err());
    assert!(index.search(&[1.0, 0.0], 1, 1.1, None).await.is_err());
    let missing = temp.path().join("missing");
    assert!(matches!(
        VectorIndex::open(&missing).await,
        Err(Error::Io(_))
    ));
    assert!(!missing.exists());
}

#[tokio::test]
async fn score_semantics_for_all_metrics() {
    for (metric, expected) in [
        (Metric::Cosine, 1.0),
        (Metric::L2, 0.5),
        (Metric::Dot, 1.0 / (1.0 + (-2.0_f32).exp())),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let index = VectorIndex::create(temp.path(), 2, metric).await.unwrap();
        index
            .upsert(
                &[row("near", "d", vec![2.0, 0.0])],
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        let hits = index.search(&[1.0, 0.0], 1, 0.0, None).await.unwrap();
        assert!(
            (hits[0].score - expected).abs() < 1e-6,
            "{metric:?}: {}",
            hits[0].score
        );
    }
}

#[tokio::test]
async fn cancellation_preserves_only_committed_batches() {
    let temp = tempfile::tempdir().unwrap();
    let index = VectorIndex::create(temp.path(), 2, Metric::Cosine)
        .await
        .unwrap();
    let token = CancellationToken::new();
    token.cancel();
    assert!(matches!(
        index.upsert(&[row("a", "d", vec![1.0, 0.0])], &token).await,
        Err(Error::Cancelled)
    ));
    assert_eq!(index.stats().await.unwrap().row_count, 0);
    let token = CancellationToken::new();
    let rows: Vec<_> = (0..UPSERT_BATCH_SIZE * 32)
        .map(|i| row(&i.to_string(), "doc", vec![1.0, 0.0]))
        .collect();
    let observe = async {
        loop {
            if index.stats().await.unwrap().row_count > 0 {
                token.cancel();
                break;
            }
            tokio::task::yield_now().await;
        }
    };
    let (result, ()) = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        tokio::join!(index.upsert(&rows, &token), observe)
    })
    .await
    .unwrap();
    assert!(matches!(result, Err(Error::Cancelled)));
    let count = index.stats().await.unwrap().row_count;
    assert!(count >= UPSERT_BATCH_SIZE && count < rows.len(), "{count}");
    assert_eq!(count % UPSERT_BATCH_SIZE, 0);
}
