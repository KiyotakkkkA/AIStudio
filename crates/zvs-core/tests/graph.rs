use zvs_core::{
    CancellationToken, Error,
    graph::{Direction, Edge, GraphStore, MAX_HOPS, MAX_NODE_BUDGET, Node, NodeKind},
};

fn document(id: &str) -> Node {
    Node {
        id: id.into(),
        kind: NodeKind::Document,
    }
}

fn chunk(id: &str) -> Node {
    Node {
        id: id.into(),
        kind: NodeKind::Chunk,
    }
}

fn edge(from: &str, to: &str, relation: &str) -> Edge {
    Edge {
        from: from.into(),
        to: to.into(),
        relation: relation.into(),
        weight: Some(0.5),
        metadata: serde_json::json!({ "source": relation }),
    }
}

fn ids(nodes: &[Node]) -> Vec<&str> {
    nodes.iter().map(|node| node.id.as_str()).collect()
}

fn line(length: usize) -> GraphStore {
    let token = CancellationToken::new();
    let nodes = (0..length)
        .map(|n| chunk(&format!("c{n}")))
        .collect::<Vec<_>>();
    let edges = (1..length)
        .map(|n| edge(&format!("c{}", n - 1), &format!("c{n}"), "cites"))
        .collect::<Vec<_>>();
    GraphStore::hydrate(&nodes, &edges, &token).unwrap()
}

fn sample() -> GraphStore {
    let token = CancellationToken::new();
    let nodes = [
        document("d1"),
        document("d2"),
        document("d3"),
        chunk("c1"),
        chunk("c2"),
        chunk("c3"),
        chunk("far"),
    ];
    let edges = [
        edge("d1", "c1", "derived_from"),
        edge("d1", "c2", "derived_from"),
        edge("d2", "c3", "derived_from"),
        edge("d1", "d2", "cites"),
        edge("d2", "d3", "cites"),
        edge("c1", "c3", "co_occurs"),
        edge("d3", "far", "derived_from"),
    ];
    GraphStore::hydrate(&nodes, &edges, &token).unwrap()
}

#[test]
fn empty_and_single_node_graphs() {
    let token = CancellationToken::new();
    let mut store = GraphStore::new();
    let stats = store.stats();
    assert_eq!(stats.node_count, 0);
    assert_eq!(stats.edge_count, 0);
    assert!(store.nodes().is_empty());
    assert!(store.edges().is_empty());
    assert!(!store.contains_node("d1"));
    assert!(store.remove_node("d1").is_none());
    assert!(store.remove_edges_touching("d1").is_none());
    assert!(!store.remove_edge("d1", "d2", "cites"));
    assert!(matches!(
        store.neighbors("d1", None, Direction::Both),
        Err(Error::InvalidInput(_))
    ));

    assert!(store.add_node(document("d1")).unwrap());
    assert!(!store.add_node(document("d1")).unwrap());
    assert_eq!(store.stats().node_count, 1);
    assert_eq!(store.stats().document_count, 1);
    assert!(store.node("d1").is_some());
    assert!(
        store
            .neighbors("d1", None, Direction::Both)
            .unwrap()
            .is_empty()
    );

    let alone = store
        .k_hop_subgraph("d1", 3, None, Direction::Both, None, &token)
        .unwrap();
    assert_eq!(alone.root, "d1");
    assert_eq!(alone.nodes.len(), 1);
    assert_eq!(alone.nodes[0].hops, 0);
    assert!(alone.edges.is_empty());

    let self_path = store
        .shortest_path("d1", "d1", None, Direction::Both, &token)
        .unwrap()
        .unwrap();
    assert_eq!(ids(&self_path.nodes), ["d1"]);
    assert!(self_path.edges.is_empty());

    assert!(store.remove_node("d1").is_some());
    assert_eq!(store.stats().node_count, 0);
}

#[test]
fn nodes_and_edges_round_trip_and_upsert() {
    let token = CancellationToken::new();
    let mut store = GraphStore::new();
    assert_eq!(
        store
            .add_nodes(&[document("d1"), chunk("c1"), chunk("c1")], &token)
            .unwrap(),
        2
    );
    assert_eq!(
        store
            .add_edges(&[edge("d1", "c1", "derived_from")], &token)
            .unwrap(),
        1
    );

    assert!(!store.add_node(chunk("d1")).unwrap());
    assert_eq!(store.node("d1").unwrap().kind, NodeKind::Chunk);
    assert!(!store.add_node(document("d1")).unwrap());
    assert_eq!(store.node("d1").unwrap().kind, NodeKind::Document);

    let mut revised = edge("d1", "c1", "derived_from");
    revised.weight = Some(0.9);
    revised.metadata = serde_json::json!({ "revised": true });
    assert!(!store.add_edge(revised).unwrap());
    assert_eq!(store.stats().edge_count, 1);
    let stored = store.edges();
    assert_eq!(stored[0].weight, Some(0.9));
    assert_eq!(stored[0].metadata, serde_json::json!({ "revised": true }));

    assert!(store.add_edge(edge("d1", "c1", "cites")).unwrap());
    assert_eq!(store.stats().edge_count, 2);
    assert!(store.remove_edge("d1", "c1", "cites"));
    assert!(!store.remove_edge("d1", "c1", "cites"));
    assert_eq!(store.stats().edge_count, 1);

    assert_eq!(ids(&store.nodes()), ["c1", "d1"]);
}

#[test]
fn invalid_nodes_and_edges_are_rejected() {
    let token = CancellationToken::new();
    let mut store = GraphStore::new();
    assert!(matches!(
        store.add_node(document("  ")),
        Err(Error::InvalidInput(_))
    ));
    store
        .add_nodes(&[document("d1"), chunk("c1")], &token)
        .unwrap();

    assert!(matches!(
        store.add_edge(edge("d1", "missing", "cites")),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        store.add_edge(edge("d1", "d1", "cites")),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        store.add_edge(edge("d1", "c1", " ")),
        Err(Error::InvalidInput(_))
    ));
    let mut infinite = edge("d1", "c1", "cites");
    infinite.weight = Some(f32::NAN);
    assert!(matches!(
        store.add_edge(infinite),
        Err(Error::InvalidInput(_))
    ));

    assert!(matches!(
        store.add_edges(
            &[edge("d1", "c1", "cites"), edge("d1", "gone", "cites")],
            &token
        ),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(store.stats().edge_count, 0);

    let error = store.add_node(document("")).unwrap_err();
    assert_eq!(error.code(), "VALIDATION_FAILED");
}

#[test]
fn neighbors_filter_by_relation_and_direction() {
    let store = sample();

    let outgoing = store.neighbors("d1", None, Direction::Outgoing).unwrap();
    assert_eq!(
        outgoing
            .iter()
            .map(|n| n.node.id.as_str())
            .collect::<Vec<_>>(),
        ["c1", "c2", "d2"]
    );
    assert!(outgoing.iter().all(|n| n.direction == Direction::Outgoing));
    assert_eq!(outgoing[0].edge.relation, "derived_from");
    assert_eq!(outgoing[0].edge.weight, Some(0.5));
    assert_eq!(outgoing[0].edge.from, "d1");

    let incoming = store.neighbors("d2", None, Direction::Incoming).unwrap();
    assert_eq!(
        incoming
            .iter()
            .map(|n| n.node.id.as_str())
            .collect::<Vec<_>>(),
        ["d1"]
    );
    assert_eq!(incoming[0].direction, Direction::Incoming);

    let both = store.neighbors("d2", None, Direction::Both).unwrap();
    assert_eq!(
        both.iter().map(|n| n.node.id.as_str()).collect::<Vec<_>>(),
        ["c3", "d1", "d3"]
    );

    let cites = store
        .neighbors("d2", Some("cites"), Direction::Both)
        .unwrap();
    assert_eq!(
        cites.iter().map(|n| n.node.id.as_str()).collect::<Vec<_>>(),
        ["d1", "d3"]
    );
    assert!(
        store
            .neighbors("d2", Some("user_linked"), Direction::Both)
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        store.neighbors("nope", None, Direction::Both),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn shortest_path_respects_direction_and_relation() {
    let token = CancellationToken::new();
    let store = sample();

    let path = store
        .shortest_path("d1", "far", None, Direction::Outgoing, &token)
        .unwrap()
        .unwrap();
    assert_eq!(ids(&path.nodes), ["d1", "d2", "d3", "far"]);
    assert_eq!(
        path.edges
            .iter()
            .map(|e| e.relation.as_str())
            .collect::<Vec<_>>(),
        ["cites", "cites", "derived_from"]
    );

    let through_chunks = store
        .shortest_path("c1", "d2", None, Direction::Both, &token)
        .unwrap()
        .unwrap();
    assert_eq!(ids(&through_chunks.nodes), ["c1", "c3", "d2"]);

    assert!(
        store
            .shortest_path("far", "d1", None, Direction::Outgoing, &token)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .shortest_path("d1", "far", Some("co_occurs"), Direction::Both, &token)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        store.shortest_path("d1", "nope", None, Direction::Both, &token),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn k_hop_subgraph_matches_a_hand_drawn_expansion() {
    let token = CancellationToken::new();
    let store = sample();

    let two = store
        .k_hop_subgraph("d1", 2, None, Direction::Both, None, &token)
        .unwrap();
    assert_eq!(
        two.nodes
            .iter()
            .map(|n| (n.node.id.as_str(), n.hops))
            .collect::<Vec<_>>(),
        [
            ("d1", 0),
            ("c1", 1),
            ("c2", 1),
            ("d2", 1),
            ("c3", 2),
            ("d3", 2)
        ]
    );
    assert_eq!(
        two.edges
            .iter()
            .map(|e| (e.from.as_str(), e.to.as_str(), e.relation.as_str()))
            .collect::<Vec<_>>(),
        [
            ("c1", "c3", "co_occurs"),
            ("d1", "c1", "derived_from"),
            ("d1", "c2", "derived_from"),
            ("d1", "d2", "cites"),
            ("d2", "c3", "derived_from"),
            ("d2", "d3", "cites"),
        ]
    );

    let one = store
        .k_hop_subgraph("d1", 1, None, Direction::Outgoing, None, &token)
        .unwrap();
    assert_eq!(
        one.nodes
            .iter()
            .map(|n| n.node.id.as_str())
            .collect::<Vec<_>>(),
        ["d1", "c1", "c2", "d2"]
    );

    let cites_only = store
        .k_hop_subgraph("d1", 8, Some("cites"), Direction::Outgoing, None, &token)
        .unwrap();
    assert_eq!(
        cites_only
            .nodes
            .iter()
            .map(|n| n.node.id.as_str())
            .collect::<Vec<_>>(),
        ["d1", "d2", "d3"]
    );
    assert_eq!(cites_only.edges.len(), 2);

    let zero = store
        .k_hop_subgraph("d1", 0, None, Direction::Both, None, &token)
        .unwrap();
    assert_eq!(zero.nodes.len(), 1);
    assert!(zero.edges.is_empty());
}

#[test]
fn k_hop_queries_are_bounded() {
    let token = CancellationToken::new();
    let store = line(64);

    assert!(matches!(
        store.k_hop_subgraph("c0", MAX_HOPS + 1, None, Direction::Both, None, &token),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        store.k_hop_subgraph("c0", 2, None, Direction::Both, Some(0), &token),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        store.k_hop_subgraph(
            "c0",
            2,
            None,
            Direction::Both,
            Some(MAX_NODE_BUDGET + 1),
            &token
        ),
        Err(Error::InvalidInput(_))
    ));

    let capped = store
        .k_hop_subgraph("c0", 8, None, Direction::Both, Some(4), &token)
        .unwrap_err();
    assert!(matches!(capped, Error::LimitExceeded(_)));
    assert_eq!(capped.code(), "VALIDATION_FAILED");

    assert_eq!(
        store
            .k_hop_subgraph("c0", 4, None, Direction::Both, Some(5), &token)
            .unwrap()
            .nodes
            .len(),
        5
    );
    assert!(
        store
            .k_hop_subgraph("c0", MAX_HOPS, None, Direction::Both, None, &token)
            .is_ok()
    );
}

#[test]
fn cascade_removal_drops_every_touching_edge() {
    let token = CancellationToken::new();
    let mut store = sample();
    assert_eq!(store.stats().edge_count, 7);

    assert_eq!(store.remove_edges_touching("c1"), Some(2));
    assert!(store.contains_node("c1"));
    assert_eq!(store.stats().edge_count, 5);
    assert!(
        store
            .neighbors("c1", None, Direction::Both)
            .unwrap()
            .is_empty()
    );

    assert_eq!(store.remove_node("d2"), Some(3));
    assert!(!store.contains_node("d2"));
    assert_eq!(store.stats().edge_count, 2);
    assert!(
        store
            .edges()
            .iter()
            .all(|edge| edge.from != "d2" && edge.to != "d2")
    );
    assert!(matches!(
        store.add_edge(edge("d1", "d2", "cites")),
        Err(Error::InvalidInput(_))
    ));

    assert_eq!(
        store
            .remove_nodes(&["d1".into(), "d3".into(), "absent".into()], &token)
            .unwrap(),
        2
    );
    assert_eq!(store.stats().edge_count, 0);
    assert_eq!(ids(&store.nodes()), ["c1", "c2", "c3", "far"]);

    assert!(store.add_node(document("d2")).unwrap());
    assert!(store.add_edge(edge("d2", "c3", "derived_from")).unwrap());
    assert_eq!(store.stats().edge_count, 1);
}

#[test]
fn traversals_honour_cancellation() {
    let token = CancellationToken::new();
    token.cancel();
    let store = sample();
    let mut fresh = GraphStore::new();

    assert!(matches!(
        fresh.add_nodes(&[document("d1")], &token),
        Err(Error::Cancelled)
    ));
    assert!(matches!(
        store.shortest_path("d1", "far", None, Direction::Both, &token),
        Err(Error::Cancelled)
    ));
    assert!(matches!(
        store.k_hop_subgraph("d1", 2, None, Direction::Both, None, &token),
        Err(Error::Cancelled)
    ));
    assert_eq!(
        store
            .k_hop_subgraph("d1", 2, None, Direction::Both, None, &token)
            .unwrap_err()
            .code(),
        "RUN_CANCELLED"
    );
}

#[test]
fn hydration_round_trips_host_rows() {
    let token = CancellationToken::new();
    let store = sample();
    let nodes = store.nodes();
    let edges = store.edges();

    let rehydrated = GraphStore::hydrate(&nodes, &edges, &token).unwrap();
    assert_eq!(rehydrated.nodes(), nodes);
    assert_eq!(rehydrated.edges(), edges);
    assert_eq!(rehydrated.stats(), store.stats());
    assert_eq!(rehydrated.stats().document_count, 3);
    assert_eq!(rehydrated.stats().chunk_count, 4);

    let json = serde_json::to_string(&edges[0]).unwrap();
    assert_eq!(serde_json::from_str::<Edge>(&json).unwrap(), edges[0]);
    assert_eq!(
        serde_json::from_str::<Node>(r#"{"id":"d1","kind":"document"}"#).unwrap(),
        document("d1")
    );
    assert_eq!(
        serde_json::from_str::<Edge>(r#"{"from":"a","to":"b","relation":"cites"}"#)
            .unwrap()
            .metadata,
        serde_json::Value::Null
    );
}
