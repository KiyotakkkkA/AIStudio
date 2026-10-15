use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;
use serde_json::{Value, json};
use zvs_contract::{GraphPathDto, Json};

use super::*;
use crate::data::repositories::{VectorDocumentDraft, VectorStoreDraft};
use crate::drivers::core::{
    GraphCore, GraphNeighbor, GraphPath, GraphStats, GraphSubgraph, InProcessCore,
};
use crate::testing::{
    FakeClock, MemoryLogger, TemporaryDatabase, seed_embedding_provider, temporary_database,
};

#[derive(Default)]
pub(crate) struct ProbeGraphCore {
    pub inner: InProcessCore,
    pub hydrations: AtomicUsize,
    pub neighbor_calls: AtomicUsize,
    pub fail_add_edge: AtomicBool,
    pub fail_remove_edge: AtomicBool,
}

impl ProbeGraphCore {
    pub fn edges_in(&self, store_id: &str) -> Vec<GraphEdge> {
        self.inner
            .graphs()
            .read(store_id, |store| Ok(store.edges()))
            .expect("reads the graph")
    }
}

fn engine_down(what: &str) -> AppError {
    AppError::new(AppErrorCode::NativeError, what)
}

#[async_trait]
impl GraphCore for ProbeGraphCore {
    async fn hydrate(&self, graph_id: &str, content: GraphContent) -> Result<GraphStats, AppError> {
        self.hydrations.fetch_add(1, Ordering::SeqCst);
        self.inner.hydrate(graph_id, content).await
    }

    async fn graph_stats(&self, graph_id: &str) -> Result<GraphStats, AppError> {
        self.inner.graph_stats(graph_id).await
    }

    async fn add_node(&self, graph_id: &str, node: GraphNode) -> Result<bool, AppError> {
        self.inner.add_node(graph_id, node).await
    }

    async fn add_edge(&self, graph_id: &str, edge: GraphEdge) -> Result<bool, AppError> {
        if self.fail_add_edge.swap(false, Ordering::SeqCst) {
            return Err(engine_down("engine down"));
        }
        self.inner.add_edge(graph_id, edge).await
    }

    async fn remove_edge(
        &self,
        graph_id: &str,
        from: &str,
        to: &str,
        relation: &str,
    ) -> Result<bool, AppError> {
        if self.fail_remove_edge.swap(false, Ordering::SeqCst) {
            return Err(engine_down("locked"));
        }
        self.inner.remove_edge(graph_id, from, to, relation).await
    }

    async fn remove_node(&self, graph_id: &str, node_id: &str) -> Result<Option<usize>, AppError> {
        self.inner.remove_node(graph_id, node_id).await
    }

    async fn neighbors(
        &self,
        graph_id: &str,
        query: NeighborQuery,
    ) -> Result<Vec<GraphNeighbor>, AppError> {
        self.neighbor_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.neighbors(graph_id, query).await
    }

    async fn shortest_path(
        &self,
        graph_id: &str,
        query: PathQuery,
    ) -> Result<Option<GraphPath>, AppError> {
        self.inner.shortest_path(graph_id, query).await
    }

    async fn k_hop_subgraph(
        &self,
        graph_id: &str,
        query: SubgraphQuery,
    ) -> Result<GraphSubgraph, AppError> {
        self.inner.k_hop_subgraph(graph_id, query).await
    }
}

struct Fixture {
    service: GraphService,
    core: Arc<ProbeGraphCore>,
    logger: Arc<MemoryLogger>,
    store: VectorStoreId,
    other: VectorStoreId,
    documents: Vec<DocumentId>,
    foreign: DocumentId,
    database: TemporaryDatabase,
}

fn fresh_id() -> String {
    new_id().hyphenated().to_string()
}

fn fixture() -> Fixture {
    let database = temporary_database();
    let (store, other, documents, foreign) = database
        .with_connection(|connection| {
            let provider = seed_embedding_provider(connection);
            let repositories = Repositories::new(connection);
            let store = |name: &str| -> Result<String, crate::data::DataError> {
                let id = fresh_id();
                repositories
                    .vector_stores
                    .create(&VectorStoreDraft::new(&id, name, &provider, "embed", 3, 1))?;
                Ok(id)
            };
            let knowledge = store("Knowledge")?;
            let archive = store("Archive")?;
            let document = |store_id: &str, path: &str| -> Result<String, crate::data::DataError> {
                Ok(repositories
                    .vector_documents
                    .record_indexed(&VectorDocumentDraft {
                        id: fresh_id(),
                        store_id: store_id.to_owned(),
                        source_path: path.to_owned(),
                        content_hash: path.to_owned(),
                        chunk_count: 1,
                        bytes: 10,
                        indexed_at: 1,
                    })?
                    .id)
            };
            let documents = ["a.md", "b.md", "c.md"]
                .iter()
                .map(|path| document(&knowledge, path))
                .collect::<Result<Vec<_>, _>>()?;
            let foreign = document(&archive, "a.md")?;
            Ok((knowledge, archive, documents, foreign))
        })
        .expect("seeds the stores");
    let core = Arc::new(ProbeGraphCore::default());
    let logger = MemoryLogger::shared();
    let service = GraphService::new(GraphServiceOptions {
        database: database.shared(),
        core: core.clone(),
        logger: logger.clone(),
        clock: Arc::new(FakeClock::new(1000)),
    });
    Fixture {
        service,
        core,
        logger,
        store: VectorStoreId::parse(&store).expect("an id"),
        other: VectorStoreId::parse(&other).expect("an id"),
        documents: documents
            .iter()
            .map(|id| DocumentId::parse(id).expect("an id"))
            .collect(),
        foreign: DocumentId::parse(&foreign).expect("an id"),
        database,
    }
}

impl Fixture {
    fn relation(&self, name: &str) -> GraphRelationTypeId {
        let row = self
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .graph_relations
                    .find_by_name(name)?)
            })
            .expect("reads")
            .expect("a seeded relation");
        GraphRelationTypeId::parse(&row.id).expect("an id")
    }

    fn key(&self, from: usize, to: usize, name: &str) -> GraphEdgeKey {
        GraphEdgeKey {
            store_id: self.store,
            from_document_id: self.documents[from],
            to_document_id: self.documents[to],
            relation_type_id: self.relation(name),
        }
    }

    async fn link(&self, from: usize, to: usize, name: &str) -> Result<GraphEdgeDto, AppError> {
        self.link_with(from, to, name, Json::Null).await
    }

    async fn link_with(
        &self,
        from: usize,
        to: usize,
        name: &str,
        metadata: Json,
    ) -> Result<GraphEdgeDto, AppError> {
        self.service
            .add_edge(&AddGraphEdgeInput {
                metadata: Some(metadata),
                ..AddGraphEdgeInput::new(self.key(from, to, name))
            })
            .await
    }

    fn rows(&self) -> Vec<GraphEdgeRow> {
        let store = self.store.to_string();
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .graph_edges
                    .list_by_store(&store)?)
            })
            .expect("reads")
    }

    fn stale(&self) -> bool {
        let store = self.store.to_string();
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .vector_stores
                    .find_by_id(&store)?)
            })
            .expect("reads")
            .expect("the store")
            .graph_stale
    }

    fn id(&self, index: usize) -> String {
        self.documents[index].to_string()
    }
}

fn round_trips<T: serde::Serialize + serde::de::DeserializeOwned + zvs_contract::Validate>(
    value: &T,
) -> bool {
    let wire = serde_json::to_value(value).expect("serialises");
    zvs_contract::parse::<T>(wire.clone())
        .map(|parsed| serde_json::to_value(parsed).expect("serialises") == wire)
        .unwrap_or(false)
}

#[test]
fn the_seeded_catalogue_carries_the_four_starter_relations() {
    let fixture = fixture();
    let relations = fixture.service.list_relations().expect("lists");
    assert_eq!(
        relations
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["cites", "co_occurs", "derived_from", "user_linked"]
    );
    assert!(
        !relations
            .iter()
            .find(|row| row.name == "co_occurs")
            .expect("seeded")
            .directed
    );
    assert!(
        relations
            .iter()
            .find(|row| row.name == "cites")
            .expect("seeded")
            .directed
    );
    assert!(round_trips(&relations));
}

#[tokio::test]
async fn adding_an_edge_writes_the_row_and_the_engine_half_metadata_intact() {
    let fixture = fixture();
    let edge = fixture
        .link_with(0, 1, "user_linked", json!({ "note": "вручную" }))
        .await
        .expect("links");
    assert_eq!(fixture.rows().len(), 1);
    assert_eq!(
        fixture.core.edges_in(&fixture.store.to_string()),
        [GraphEdge {
            from: fixture.id(0),
            to: fixture.id(1),
            relation: "user_linked".to_owned(),
            weight: None,
            metadata: json!({ "note": "вручную" }),
        }]
    );
    assert!(round_trips(&edge));
}

#[tokio::test]
async fn listing_edges_is_scoped_to_one_store_and_validates_against_the_channel_output() {
    let fixture = fixture();
    fixture.link(0, 1, "user_linked").await.expect("links");
    fixture.link(1, 2, "cites").await.expect("links");
    let edges = fixture
        .service
        .list_edges(&fixture.store.to_string())
        .expect("lists");
    assert_eq!(
        edges
            .iter()
            .map(|row| (row.from_document_id, row.to_document_id))
            .collect::<Vec<_>>(),
        [
            (fixture.documents[0], fixture.documents[1]),
            (fixture.documents[1], fixture.documents[2])
        ]
    );
    assert!(round_trips(&edges));
    assert!(
        fixture
            .service
            .list_edges(&fixture.other.to_string())
            .expect("lists")
            .is_empty()
    );
    assert_eq!(
        fixture
            .service
            .list_edges(&fresh_id())
            .expect_err("missing")
            .code,
        AppErrorCode::NotFound
    );
}

#[tokio::test]
async fn a_failing_engine_rolls_the_row_back_instead_of_leaving_a_half_written_edge() {
    let fixture = fixture();
    fixture.core.fail_add_edge.store(true, Ordering::SeqCst);
    let error = fixture.link(0, 1, "cites").await.expect_err("fails");
    assert_eq!(error.message, "engine down");
    assert!(fixture.rows().is_empty());
    assert_eq!(fixture.logger.matching(ENGINE_WRITE_FAILED).len(), 1);
}

#[tokio::test]
async fn removing_an_edge_deletes_both_halves() {
    let fixture = fixture();
    fixture.link(0, 1, "cites").await.expect("links");
    fixture
        .service
        .remove_edge(&fixture.key(0, 1, "cites"))
        .await
        .expect("removes");
    assert!(fixture.rows().is_empty());
    assert!(fixture.core.edges_in(&fixture.store.to_string()).is_empty());
}

#[tokio::test]
async fn a_failed_engine_removal_keeps_the_row_flags_the_store_and_surfaces_the_failure() {
    let fixture = fixture();
    fixture.link(0, 1, "cites").await.expect("links");
    fixture.core.fail_remove_edge.store(true, Ordering::SeqCst);
    let error = fixture
        .service
        .remove_edge(&fixture.key(0, 1, "cites"))
        .await
        .expect_err("fails");
    assert_eq!(error.code, AppErrorCode::NativeError);
    assert_eq!(error.message, ENGINE_REMOVE_FAILED);
    assert_eq!(fixture.rows().len(), 1);
    assert!(fixture.stale());
    assert_eq!(fixture.logger.matching(ENGINE_REMOVE_FAILED).len(), 1);
}

#[tokio::test]
async fn edges_may_not_cross_stores_or_point_at_a_document_that_is_not_there() {
    let fixture = fixture();
    let crossing = fixture
        .service
        .add_edge(&AddGraphEdgeInput::new(GraphEdgeKey {
            to_document_id: fixture.foreign,
            ..fixture.key(0, 1, "cites")
        }))
        .await
        .expect_err("refused");
    assert_eq!(crossing.code, AppErrorCode::ValidationFailed);
    assert_eq!(crossing.message, CROSS_STORE);
    let missing = fixture
        .service
        .add_edge(&AddGraphEdgeInput::new(GraphEdgeKey {
            to_document_id: DocumentId::new(new_id()),
            ..fixture.key(0, 1, "cites")
        }))
        .await
        .expect_err("refused");
    assert_eq!(missing.code, AppErrorCode::NotFound);
    let itself = fixture.link(0, 0, "cites").await.expect_err("refused");
    assert_eq!(itself.message, SELF_LINK);
    assert!(fixture.rows().is_empty());
}

#[tokio::test]
async fn the_same_relation_between_the_same_pair_cannot_be_drawn_twice() {
    let fixture = fixture();
    fixture.link(0, 1, "cites").await.expect("links");
    assert_eq!(
        fixture.link(0, 1, "cites").await.expect_err("refused").code,
        AppErrorCode::Conflict
    );
    fixture.link(0, 1, "user_linked").await.expect("links");
    assert_eq!(fixture.rows().len(), 2);
}

#[tokio::test]
async fn neighbors_shortest_path_and_k_hop_map_onto_the_engine_calls() {
    let fixture = fixture();
    fixture.link(0, 1, "cites").await.expect("links");
    fixture.link(1, 2, "derived_from").await.expect("links");
    let neighbors = fixture
        .service
        .neighbors(&GraphNeighborsInput {
            direction: Some(GraphDirection::Incoming),
            ..GraphNeighborsInput::new(fixture.store, fixture.documents[1])
        })
        .await
        .expect("answers");
    assert_eq!(
        neighbors,
        [GraphNeighborDto {
            node: GraphNodeDto {
                id: fixture.id(0),
                kind: GraphNodeKind::Document,
            },
            edge: GraphLinkDto {
                from: fixture.id(0),
                to: fixture.id(1),
                relation: "cites".to_owned(),
                weight: None,
                metadata: Value::Null,
            },
            direction: GraphDirection::Incoming,
        }]
    );

    let outgoing = |relation: Option<&str>| GraphShortestPathInput {
        relation: relation.map(str::to_owned),
        direction: Some(GraphDirection::Outgoing),
        ..GraphShortestPathInput::new(fixture.store, fixture.documents[0], fixture.documents[2])
    };
    let path = fixture
        .service
        .shortest_path(&outgoing(None))
        .await
        .expect("answers")
        .expect("a path");
    assert_eq!(
        path.nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<Vec<_>>(),
        [fixture.id(0), fixture.id(1), fixture.id(2)]
    );
    assert_eq!(
        path.edges
            .iter()
            .map(|edge| edge.relation.as_str())
            .collect::<Vec<_>>(),
        ["cites", "derived_from"]
    );
    let filtered: Option<GraphPathDto> = fixture
        .service
        .shortest_path(&outgoing(Some("cites")))
        .await
        .expect("answers");
    assert_eq!(filtered, None);

    let subgraph = fixture
        .service
        .k_hop_subgraph(&GraphSubgraphInput {
            hops: Some(1),
            ..GraphSubgraphInput::new(fixture.store, fixture.documents[0])
        })
        .await
        .expect("answers");
    assert_eq!(subgraph.root, fixture.id(0));
    assert_eq!(
        subgraph
            .nodes
            .iter()
            .map(|entry| entry.hops)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert!(round_trips(&subgraph));
}

#[tokio::test]
async fn an_unknown_relation_name_is_rejected_before_the_engine_sees_the_query() {
    let fixture = fixture();
    let error = fixture
        .service
        .neighbors(&GraphNeighborsInput {
            relation: Some("invented".to_owned()),
            ..GraphNeighborsInput::new(fixture.store, fixture.documents[0])
        })
        .await
        .expect_err("refused");
    assert_eq!(error.code, AppErrorCode::NotFound);
    assert_eq!(error.message, RELATION_NOT_IN_CATALOGUE);
    assert_eq!(fixture.core.neighbor_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn reconcile_flags_drift_when_the_ledger_loses_a_row_the_engine_still_holds() {
    let fixture = fixture();
    fixture.link(0, 1, "cites").await.expect("links");
    let store = fixture.store.to_string();
    assert_eq!(
        fixture.service.reconcile(&store).await.expect("reconciles"),
        GraphReconcileDto {
            store_id: fixture.store,
            recorded_edges: 1,
            engine_edges: 1,
            graph_stale: false,
        }
    );
    assert!(!fixture.stale());

    let row = fixture.rows().remove(0);
    fixture
        .database
        .with_connection(|connection| {
            Ok(Repositories::new(connection).graph_edges.remove(&row.id)?)
        })
        .expect("removes");
    let drifted = fixture.service.reconcile(&store).await.expect("reconciles");
    assert_eq!(
        drifted,
        GraphReconcileDto {
            store_id: fixture.store,
            recorded_edges: 0,
            engine_edges: 1,
            graph_stale: true,
        }
    );
    assert!(fixture.stale());
    assert_eq!(fixture.logger.matching(GRAPH_DRIFT).len(), 1);
    assert!(round_trips(&drifted));
}

#[tokio::test]
async fn reconcile_clears_the_flag_once_the_two_halves_agree_again() {
    let fixture = fixture();
    fixture.link(0, 1, "cites").await.expect("links");
    let store = fixture.store.to_string();
    let row = fixture.rows().remove(0);
    fixture
        .database
        .with_connection(|connection| {
            Ok(Repositories::new(connection).graph_edges.remove(&row.id)?)
        })
        .expect("removes");
    fixture.service.reconcile(&store).await.expect("reconciles");
    fixture
        .core
        .inner
        .remove_edge(&store, &fixture.id(0), &fixture.id(1), "cites")
        .await
        .expect("removes");
    assert!(
        !fixture
            .service
            .reconcile(&store)
            .await
            .expect("reconciles")
            .graph_stale
    );
    assert!(!fixture.stale());
}

#[tokio::test]
async fn a_store_is_hydrated_from_the_ledger_once_before_the_first_traversal() {
    let fixture = fixture();
    let co_occurs = fixture.relation("co_occurs").to_string();
    let draft = GraphEdgeDraft {
        id: fresh_id(),
        store_id: fixture.store.to_string(),
        from_document_id: fixture.id(0),
        to_document_id: fixture.id(1),
        relation_type_id: co_occurs,
        weight: Some(0.25),
        metadata: Value::Null,
        created_at: 1,
    };
    fixture
        .database
        .with_connection(|connection| Ok(Repositories::new(connection).graph_edges.link(&draft)?))
        .expect("links");
    assert_eq!(fixture.core.hydrations.load(Ordering::SeqCst), 0);
    let neighbors = fixture
        .service
        .neighbors(&GraphNeighborsInput::new(
            fixture.store,
            fixture.documents[0],
        ))
        .await
        .expect("answers");
    assert_eq!(
        neighbors
            .iter()
            .map(|entry| entry.edge.relation.as_str())
            .collect::<Vec<_>>(),
        ["co_occurs"]
    );
    assert_eq!(neighbors[0].edge.weight, Some(0.25));
    fixture
        .service
        .neighbors(&GraphNeighborsInput::new(
            fixture.store,
            fixture.documents[1],
        ))
        .await
        .expect("answers");
    assert_eq!(fixture.core.hydrations.load(Ordering::SeqCst), 1);
    assert!(
        !fixture
            .service
            .reconcile(&fixture.store.to_string())
            .await
            .expect("reconciles")
            .graph_stale
    );
    fixture.service.forget(&fixture.store.to_string());
    fixture
        .service
        .reconcile(&fixture.store.to_string())
        .await
        .expect("reconciles");
    assert_eq!(fixture.core.hydrations.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_weight_keeps_its_decimal_spelling_through_the_engine() {
    let fixture = fixture();
    fixture
        .service
        .add_edge(&AddGraphEdgeInput {
            weight: Some(Some(0.3)),
            ..AddGraphEdgeInput::new(fixture.key(0, 1, "cites"))
        })
        .await
        .expect("links");
    let neighbors = fixture
        .service
        .neighbors(&GraphNeighborsInput::new(
            fixture.store,
            fixture.documents[0],
        ))
        .await
        .expect("answers");
    assert_eq!(
        serde_json::to_value(neighbors[0].edge.weight).expect("serialises"),
        json!(0.3)
    );
}
