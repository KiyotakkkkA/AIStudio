use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::json;
use tokio_util::sync::CancellationToken;
use zvs_contract::AppErrorCode;

use super::in_process::{blocking, guarded};
use super::*;
use crate::testing::temporary_directory;

fn core() -> InProcessCore {
    InProcessCore::new()
}

fn config(size: usize, overlap: usize) -> ChunkConfig {
    ChunkConfig { size, overlap }
}

fn document(id: &str) -> GraphNode {
    GraphNode {
        id: id.to_owned(),
        kind: GraphNodeKind::Document,
    }
}

fn chunk_node(id: &str) -> GraphNode {
    GraphNode {
        id: id.to_owned(),
        kind: GraphNodeKind::Chunk,
    }
}

fn edge(from: &str, to: &str, relation: &str) -> GraphEdge {
    GraphEdge {
        from: from.to_owned(),
        to: to.to_owned(),
        relation: relation.to_owned(),
        weight: None,
        metadata: serde_json::Value::Null,
    }
}

#[tokio::test]
async fn chunks_own_unicode_text_with_byte_ranges_and_hashes_bytes() {
    let core = core();
    let source = "Привет 🌍 мир";
    let chunks = core
        .chunk(source.to_owned(), config(2, 1))
        .await
        .expect("chunks");
    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk.text.as_str())
            .collect::<Vec<_>>(),
        ["Привет 🌍", "🌍 мир"]
    );
    for chunk in &chunks {
        assert_eq!(
            &source[chunk.byte_start as usize..chunk.byte_end as usize],
            chunk.text
        );
        assert_eq!(chunk.token_count, 2);
    }
    assert!(
        core.chunk(String::new(), config(2, 0))
            .await
            .expect("chunks")
            .is_empty()
    );
    assert_eq!(
        core.hash(b"abc".to_vec()).await.expect("hashes"),
        "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
    );
    assert_eq!(
        core.chunk("word ".repeat(600), ChunkConfig::default())
            .await
            .expect("chunks")
            .len(),
        3
    );
}

#[tokio::test]
async fn chunk_configs_are_refused_before_the_core_is_reached() {
    let core = core();
    for invalid in [
        config(0, 0),
        config(usize::try_from(u64::from(u32::MAX) + 1).expect("64-bit"), 0),
        config(2, 2),
        config(2, 3),
    ] {
        let error = core
            .chunk("text".to_owned(), invalid)
            .await
            .expect_err("refused");
        assert_eq!(error.code, AppErrorCode::ValidationFailed, "{invalid:?}");
        assert_eq!(error.message, CHUNK_CONFIG_MESSAGE);
        let error = core
            .chunk_segments(Vec::new(), invalid)
            .await
            .expect_err("refused");
        assert_eq!(error.code, AppErrorCode::ValidationFailed);
    }
    assert!(config(u32::MAX as usize, 0).validate().is_ok());
}

#[tokio::test]
async fn segments_keep_their_section_and_page() {
    let chunks = core()
        .chunk_segments(
            vec![
                TextSegment {
                    text: "Первый раздел. Второе предложение.".to_owned(),
                    section: vec!["Глава 1".to_owned()],
                    page: Some(1),
                },
                TextSegment {
                    text: "Другая глава.".to_owned(),
                    section: vec!["Глава 2".to_owned()],
                    page: None,
                },
            ],
            ChunkConfig::default(),
        )
        .await
        .expect("chunks");
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].section, ["Глава 1"]);
    assert_eq!(chunks[0].page, Some(1));
    assert_eq!(chunks[1].section, ["Глава 2"]);
    assert_eq!(chunks[1].page, None);
}

#[test]
fn core_errors_keep_their_shared_codes() {
    for (error, code) in [
        (
            zvs_core::Error::InvalidInput("invalid".into()),
            AppErrorCode::ValidationFailed,
        ),
        (zvs_core::Error::Cancelled, AppErrorCode::RunCancelled),
        (
            zvs_core::Error::Backend("backend".into()),
            AppErrorCode::NativeError,
        ),
        (
            zvs_core::Error::LimitExceeded("limit".into()),
            AppErrorCode::LimitExceeded,
        ),
        (
            zvs_core::Error::Io(std::io::ErrorKind::NotFound.into()),
            AppErrorCode::NotFound,
        ),
        (
            zvs_core::Error::Io(std::io::ErrorKind::PermissionDenied.into()),
            AppErrorCode::PermissionDenied,
        ),
        (
            zvs_core::Error::Io(std::io::ErrorKind::Other.into()),
            AppErrorCode::NativeError,
        ),
    ] {
        let converted = core_error(&error);
        assert_eq!(converted.code, code);
        assert_eq!(converted.message, error.to_string());
    }
}

#[tokio::test]
async fn a_panic_costs_one_call_and_the_next_call_succeeds() {
    let error = blocking(|| -> Result<(), zvs_contract::AppError> { panic!("test panic") })
        .await
        .expect_err("the panic is an error");
    assert_eq!(error.code, AppErrorCode::NativeError);
    assert_eq!(error.message, PANIC_MESSAGE);
    let error = guarded(async { panic!("test future panic") as zvs_core::Result<()> })
        .await
        .expect_err("the panic is an error");
    assert_eq!(error.code, AppErrorCode::NativeError);
    assert_eq!(blocking(|| Ok(42)).await.expect("still alive"), 42);
    assert_eq!(
        core()
            .chunk("still alive".to_owned(), config(1, 0))
            .await
            .expect("chunks")
            .len(),
        2
    );
}

#[tokio::test(flavor = "current_thread")]
async fn core_work_leaves_the_async_host_responsive() {
    let ticks = Arc::new(AtomicUsize::new(0));
    let ticker = {
        let ticks = Arc::clone(&ticks);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(1)).await;
                ticks.fetch_add(1, Ordering::SeqCst);
            }
        })
    };
    core()
        .chunk("word ".repeat(500_000), ChunkConfig::default())
        .await
        .expect("chunks");
    ticker.abort();
    assert!(ticks.load(Ordering::SeqCst) > 0);
}

fn rows(count: usize, dimension: usize) -> Vec<VectorRow> {
    let mut seed: u32 = 19;
    let mut random = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f64::from(seed) as f32 / 4_294_967_296.0 - 0.5
    };
    (0..count)
        .map(|index| VectorRow {
            id: format!("row-{index}"),
            vector: (0..dimension).map(|_| random()).collect(),
            payload: json!({ "text": format!("Фрагмент {index}") }),
            document_id: if index < count / 2 { "doc'1" } else { "doc2" }.to_owned(),
            chunk_index: u32::try_from(index).expect("small"),
            path: "C:/docs/source.txt".to_owned(),
            text: String::new(),
            section: String::new(),
            page: None,
        })
        .collect()
}

#[tokio::test]
async fn vectors_persist_search_replace_delete_and_report_stats() {
    let directory = temporary_directory("zvs-vector-core-");
    let path = directory.join("store");
    let core = core();
    let created = core
        .create_index(&path, 16, VectorMetric::Cosine)
        .await
        .expect("creates");
    assert_eq!(created.row_count, 0);
    assert_eq!(created.dimension, 16);
    assert_eq!(created.index_type, "FLAT");

    let rows = rows(400, 16);
    assert_eq!(
        core.upsert(&path, rows.clone(), &CancellationToken::new())
            .await
            .expect("upserts"),
        400
    );
    assert!(path.join("vectors.lance").exists());

    let request = |k: usize, min_score: f32| VectorSearchRequest {
        vector: rows[42].vector.clone(),
        k,
        min_score,
        ..VectorSearchRequest::default()
    };
    let hits = core.search(&path, request(5, 0.0)).await.expect("searches");
    assert_eq!(hits.len(), 5);
    assert_eq!(hits[0].id, "row-42");
    assert!(hits[0].score > 0.99999);
    assert_eq!(hits[0].payload, rows[42].payload);

    let filtered = core
        .search(
            &path,
            VectorSearchRequest {
                filter: Some("document_id = 'doc2'".to_owned()),
                ..request(5, 0.0)
            },
        )
        .await
        .expect("searches");
    assert!(filtered.iter().all(|hit| hit.document_id == "doc2"));
    let scoped = core
        .search(
            &path,
            VectorSearchRequest {
                document_ids: Some(vec!["doc'1".to_owned()]),
                ..request(5, 0.0)
            },
        )
        .await
        .expect("searches");
    assert!(scoped.iter().all(|hit| hit.document_id == "doc'1"));

    let mut replacement = rows[42].clone();
    replacement.payload = json!({ "text": "updated" });
    core.upsert(&path, vec![replacement.clone()], &CancellationToken::new())
        .await
        .expect("replaces");
    let updated = core
        .search(&path, request(1, 0.99))
        .await
        .expect("searches");
    assert_eq!(updated[0].payload, replacement.payload);

    let stats = core.open_index(&path).await.expect("opens");
    assert_eq!(stats.row_count, 400);
    assert!(stats.on_disk_bytes > 0);
    let mismatch = core
        .create_index(&path, 8, VectorMetric::Cosine)
        .await
        .expect_err("refuses a new dimension");
    assert_eq!(mismatch.code, AppErrorCode::ValidationFailed);

    let stored = core
        .chunks(&path, "doc2", &[201, 202])
        .await
        .expect("reads");
    assert_eq!(stored.len(), 2);

    core.delete_by_source(&path, "doc'1")
        .await
        .expect("deletes");
    assert_eq!(core.stats(&path).await.expect("stats").row_count, 200);
    core.delete_by_ids(&path, &["row-200".to_owned(), "x') OR true --".to_owned()])
        .await
        .expect("deletes");
    assert_eq!(core.stats(&path).await.expect("stats").row_count, 199);

    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let error = core
        .upsert(&path, rows, &cancelled)
        .await
        .expect_err("cancelled");
    assert_eq!(error.code, AppErrorCode::RunCancelled);
    assert_eq!(error.message, CANCELLED_MESSAGE);
    assert_eq!(core.stats(&path).await.expect("stats").row_count, 199);

    core.maintain(&path, 100_000).await.expect("maintains");
    core.remove_index(&path).await.expect("removes");
    assert_eq!(
        core.open_index(&path).await.expect_err("gone").code,
        AppErrorCode::NotFound
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_upsert_cancelled_mid_flight_keeps_only_whole_batches() {
    let directory = temporary_directory("zvs-vector-cancel-");
    let path = directory.join("store");
    let core = Arc::new(core());
    core.create_index(&path, 8, VectorMetric::Cosine)
        .await
        .expect("creates");
    let rows = rows(zvs_core::index::UPSERT_BATCH_SIZE * 40, 8);
    let total = rows.len();
    let token = CancellationToken::new();
    let upsert = {
        let core = Arc::clone(&core);
        let path = path.clone();
        let token = token.clone();
        tokio::spawn(async move { core.upsert(&path, rows, &token).await })
    };
    tokio::time::sleep(Duration::from_millis(20)).await;
    token.cancel();
    let outcome = upsert.await.expect("joins");
    let count = core.stats(&path).await.expect("stats").row_count;
    match outcome {
        Ok(written) => assert_eq!(written, total),
        Err(error) => {
            assert_eq!(error.code, AppErrorCode::RunCancelled);
            assert!(count < total);
        }
    }
    assert_eq!(count % zvs_core::index::UPSERT_BATCH_SIZE, 0);
}

#[tokio::test]
async fn graphs_are_stored_per_id_and_answer_every_traversal() {
    let core = core();
    let store = "native-graph";
    assert!(core.add_node(store, document("a")).await.expect("adds"));
    assert!(!core.add_node(store, document(" a ")).await.expect("adds"));
    for id in ["b", "c", "d"] {
        core.add_node(store, chunk_node(id)).await.expect("adds");
    }
    let metadata = json!({ "note": "Связь", "nested": { "list": [1, 2, 3] } });
    assert!(
        core.add_edge(
            store,
            GraphEdge {
                weight: Some(0.5),
                metadata: metadata.clone(),
                ..edge("a", "b", "cites")
            }
        )
        .await
        .expect("adds")
    );
    core.add_edge(store, edge("b", "c", "cites"))
        .await
        .expect("adds");
    core.add_edge(store, edge("c", "d", "co_occurs"))
        .await
        .expect("adds");

    let neighbors = core
        .neighbors(
            store,
            NeighborQuery {
                direction: GraphDirection::Outgoing,
                ..NeighborQuery::new("a")
            },
        )
        .await
        .expect("neighbors");
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].node.id, "b");
    assert_eq!(neighbors[0].direction, GraphDirection::Outgoing);
    assert_eq!(neighbors[0].edge.weight, Some(0.5));
    assert_eq!(neighbors[0].edge.metadata, metadata);

    let path = core
        .shortest_path(store, PathQuery::new("a", "d"))
        .await
        .expect("a path")
        .expect("exists");
    assert_eq!(
        path.nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "c", "d"]
    );
    assert_eq!(path.edges.len(), 3);
    assert!(
        core.shortest_path(
            store,
            PathQuery {
                relation: Some("cites".to_owned()),
                ..PathQuery::new("a", "d")
            }
        )
        .await
        .expect("answers")
        .is_none()
    );

    let subgraph = core
        .k_hop_subgraph(store, SubgraphQuery::new("a", 2))
        .await
        .expect("a subgraph");
    assert_eq!(subgraph.root, "a");
    assert_eq!(
        subgraph
            .nodes
            .iter()
            .map(|entry| (entry.node.id.as_str(), entry.hops))
            .collect::<Vec<_>>(),
        [("a", 0), ("b", 1), ("c", 2)]
    );

    assert_eq!(
        core.neighbors("other-graph-id", NeighborQuery::new("a"))
            .await
            .expect_err("unknown node")
            .code,
        AppErrorCode::ValidationFailed
    );
    assert_eq!(
        core.k_hop_subgraph(
            store,
            SubgraphQuery {
                budget: Some(1),
                ..SubgraphQuery::new("a", 2)
            }
        )
        .await
        .expect_err("over budget")
        .code,
        AppErrorCode::LimitExceeded
    );

    assert!(
        core.remove_edge(store, "b", "c", "cites")
            .await
            .expect("removes")
    );
    assert!(
        !core
            .remove_edge(store, "b", "c", "cites")
            .await
            .expect("answers")
    );
    assert_eq!(
        core.remove_node(store, "a").await.expect("removes"),
        Some(1)
    );
    assert_eq!(core.remove_node(store, "a").await.expect("answers"), None);
    assert!(
        core.neighbors(store, NeighborQuery::new("b"))
            .await
            .expect("neighbors")
            .is_empty()
    );
}

#[tokio::test]
async fn hydration_replaces_a_graph_and_stats_report_both_halves() {
    let core = core();
    let store = "native-graph-hydrate";
    core.add_node(store, document("stale")).await.expect("adds");
    let hydrated = core
        .hydrate(
            store,
            GraphContent {
                nodes: vec![document("a"), document("b"), chunk_node("c")],
                edges: vec![
                    GraphEdge {
                        weight: Some(0.5),
                        metadata: json!({ "note": "п" }),
                        ..edge("a", "b", "cites")
                    },
                    edge("b", "c", "derived_from"),
                ],
            },
        )
        .await
        .expect("hydrates");
    assert_eq!(
        serde_json::to_value(hydrated).expect("serialises"),
        json!({ "nodeCount": 3, "edgeCount": 2, "documentCount": 2, "chunkCount": 1 })
    );
    assert_eq!(core.graph_stats(store).await.expect("stats"), hydrated);
    assert_eq!(
        core.neighbors(store, NeighborQuery::new("stale"))
            .await
            .expect_err("replaced")
            .code,
        AppErrorCode::ValidationFailed
    );
    let neighbors = core
        .neighbors(
            store,
            NeighborQuery {
                direction: GraphDirection::Outgoing,
                ..NeighborQuery::new("a")
            },
        )
        .await
        .expect("neighbors");
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].node.id, "b");
    assert_eq!(neighbors[0].edge.metadata, json!({ "note": "п" }));
    let empty = core
        .graph_stats("native-graph-never-used")
        .await
        .expect("stats");
    assert_eq!(empty.node_count + empty.edge_count, 0);
    assert_eq!(core.graphs().len(), 1);
}

#[tokio::test]
async fn graph_requests_are_validated_before_the_core_is_reached() {
    let core = core();
    let refused = |result: Result<_, zvs_contract::AppError>| {
        let error = result.expect_err("refused");
        assert_eq!(error.code, AppErrorCode::ValidationFailed);
        assert_eq!(error.message, GRAPH_REQUEST_MESSAGE);
    };
    refused(core.graph_stats("  ").await.map(|_| ()));
    refused(core.add_node("g", document(" ")).await.map(|_| ()));
    refused(core.add_edge("g", edge("a", "", "cites")).await.map(|_| ()));
    refused(
        core.add_edge(
            "g",
            GraphEdge {
                weight: Some(f32::NAN),
                ..edge("a", "b", "cites")
            },
        )
        .await
        .map(|_| ()),
    );
    refused(core.remove_edge("g", "a", "b", " ").await.map(|_| ()));
    refused(core.remove_node("g", "").await.map(|_| ()));
    refused(
        core.neighbors(
            "g",
            NeighborQuery {
                relation: Some(" ".to_owned()),
                ..NeighborQuery::new("a")
            },
        )
        .await
        .map(|_| ()),
    );
    refused(
        core.shortest_path("g", PathQuery::new("a", " "))
            .await
            .map(|_| ()),
    );
    refused(
        core.k_hop_subgraph("g", SubgraphQuery::new("a", MAX_HOPS + 1))
            .await
            .map(|_| ()),
    );
    for budget in [0, MAX_NODE_BUDGET + 1] {
        refused(
            core.k_hop_subgraph(
                "g",
                SubgraphQuery {
                    budget: Some(budget),
                    ..SubgraphQuery::new("a", 1)
                },
            )
            .await
            .map(|_| ()),
        );
    }
    refused(
        core.hydrate(
            "g",
            GraphContent {
                nodes: vec![document("")],
                edges: Vec::new(),
            },
        )
        .await
        .map(|_| ()),
    );
    assert!(core.graphs().is_empty());
}

#[tokio::test]
async fn a_panic_under_the_registry_lock_leaves_the_graph_usable() {
    let core = core();
    let store = "graph-panic";
    core.add_node(store, document("a")).await.expect("adds");
    let graphs = Arc::clone(&core.graphs_handle());
    let error = blocking(move || {
        graphs
            .write::<()>(store, |_| panic!("under the lock"))
            .map_err(|error| core_error(&error))
    })
    .await
    .expect_err("the panic is an error");
    assert_eq!(error.code, AppErrorCode::NativeError);
    assert!(core.add_node(store, chunk_node("b")).await.expect("adds"));
    assert!(
        core.add_edge(store, edge("a", "b", "cites"))
            .await
            .expect("adds")
    );
    assert_eq!(
        core.neighbors(store, NeighborQuery::new("a"))
            .await
            .expect("neighbors")
            .len(),
        1
    );
}

#[cfg(feature = "debug-panic")]
#[tokio::test]
async fn the_debug_panic_exports_reject_and_the_core_keeps_working() {
    let core = core();
    for error in [
        core.debug_panic().await.expect_err("panics"),
        core.vector_debug_panic().await.expect_err("panics"),
        core.graph_debug_panic("debug-graph")
            .await
            .expect_err("panics"),
    ] {
        assert_eq!(error.code, AppErrorCode::NativeError);
    }
    assert!(
        core.add_node("debug-graph", document("a"))
            .await
            .expect("adds")
    );
    assert_eq!(
        core.chunk("still alive".to_owned(), config(1, 0))
            .await
            .expect("chunks")
            .len(),
        2
    );
}
