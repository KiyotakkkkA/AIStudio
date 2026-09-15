use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use napi_derive::napi;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use zvs_core::{
    CancellationToken, Error,
    graph::{Direction, Edge, GraphStore, Node},
};

use crate::{guarded, number};

static GRAPHS: OnceLock<Mutex<HashMap<String, GraphStore>>> = OnceLock::new();

fn graphs() -> &'static Mutex<HashMap<String, GraphStore>> {
    GRAPHS.get_or_init(Mutex::default)
}

fn key(graph_id: &str) -> zvs_core::Result<String> {
    let trimmed = graph_id.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidInput("Graph id must not be empty".into()));
    }
    Ok(trimmed.to_owned())
}

fn parse<T: DeserializeOwned>(payload: &str, label: &str) -> zvs_core::Result<T> {
    serde_json::from_str(payload)
        .map_err(|error| Error::InvalidInput(format!("Invalid {label} payload: {error}")))
}

fn encode<T: Serialize>(value: &T) -> zvs_core::Result<String> {
    serde_json::to_string(value).map_err(|error| Error::Backend(error.to_string()))
}

fn write<T>(
    graph_id: &str,
    work: impl FnOnce(&mut GraphStore) -> zvs_core::Result<T>,
) -> zvs_core::Result<T> {
    let graph_id = key(graph_id)?;
    let mut graphs = graphs().lock().unwrap_or_else(|error| error.into_inner());
    work(graphs.entry(graph_id).or_default())
}

fn read<T>(
    graph_id: &str,
    work: impl FnOnce(&GraphStore) -> zvs_core::Result<T>,
) -> zvs_core::Result<T> {
    let graph_id = key(graph_id)?;
    let graphs = graphs().lock().unwrap_or_else(|error| error.into_inner());
    match graphs.get(&graph_id) {
        Some(store) => work(store),
        None => work(&GraphStore::new()),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NeighborQuery {
    id: String,
    #[serde(default)]
    relation: Option<String>,
    #[serde(default)]
    direction: Direction,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PathQuery {
    from: String,
    to: String,
    #[serde(default)]
    relation: Option<String>,
    #[serde(default)]
    direction: Direction,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubgraphQuery {
    id: String,
    hops: u32,
    #[serde(default)]
    relation: Option<String>,
    #[serde(default)]
    direction: Direction,
    #[serde(default)]
    budget: Option<u32>,
}

#[napi]
pub async fn graph_add_node(graph_id: String, node: String) -> napi::Result<bool> {
    guarded(|| {
        let node: Node = parse(&node, "graph node")?;
        write(&graph_id, |store| store.add_node(node))
    })
}

#[napi]
pub async fn graph_add_edge(graph_id: String, edge: String) -> napi::Result<bool> {
    guarded(|| {
        let edge: Edge = parse(&edge, "graph edge")?;
        write(&graph_id, |store| store.add_edge(edge))
    })
}

#[napi]
pub async fn graph_remove_edge(
    graph_id: String,
    from: String,
    to: String,
    relation: String,
) -> napi::Result<bool> {
    guarded(|| {
        write(&graph_id, |store| {
            Ok(store.remove_edge(&from, &to, &relation))
        })
    })
}

#[napi]
pub async fn graph_remove_node(graph_id: String, id: String) -> napi::Result<Option<u32>> {
    guarded(|| {
        write(&graph_id, |store| {
            store.remove_node(&id).map(number).transpose()
        })
    })
}

#[napi]
pub async fn graph_neighbors(graph_id: String, query: String) -> napi::Result<String> {
    guarded(|| {
        let query: NeighborQuery = parse(&query, "neighbor query")?;
        read(&graph_id, |store| {
            encode(&store.neighbors(&query.id, query.relation.as_deref(), query.direction)?)
        })
    })
}

#[napi]
pub async fn graph_shortest_path(graph_id: String, query: String) -> napi::Result<String> {
    guarded(|| {
        let query: PathQuery = parse(&query, "path query")?;
        read(&graph_id, |store| {
            encode(&store.shortest_path(
                &query.from,
                &query.to,
                query.relation.as_deref(),
                query.direction,
                &CancellationToken::new(),
            )?)
        })
    })
}

#[napi]
pub async fn graph_k_hop_subgraph(graph_id: String, query: String) -> napi::Result<String> {
    guarded(|| {
        let query: SubgraphQuery = parse(&query, "subgraph query")?;
        read(&graph_id, |store| {
            encode(&store.k_hop_subgraph(
                &query.id,
                query.hops,
                query.relation.as_deref(),
                query.direction,
                query.budget.map(|budget| budget as usize),
                &CancellationToken::new(),
            )?)
        })
    })
}

#[cfg(feature = "debug-panic")]
#[napi]
pub async fn graph_debug_panic(graph_id: String) -> napi::Result<()> {
    guarded(|| write::<()>(&graph_id, |_| panic!("deliberate native graph panic")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str) -> String {
        format!(r#"{{"id":"{id}","kind":"document"}}"#)
    }

    #[test]
    fn stores_are_isolated_by_id_and_survive_a_panic_under_the_lock() {
        let store = "tests-isolation";
        assert!(write(store, |graph| graph.add_node(parse(&node("a"), "node")?)).unwrap());
        assert!(write(store, |graph| graph.add_node(parse(&node("b"), "node")?)).unwrap());
        assert!(
            write(store, |graph| graph.add_edge(parse(
                r#"{"from":"a","to":"b","relation":"cites","metadata":{"note":"x"}}"#,
                "edge"
            )?))
            .unwrap()
        );
        assert!(!read("tests-other", |graph| Ok(graph.contains_node("a"))).unwrap());

        let error = guarded::<()>(|| write(store, |_| panic!("under the lock"))).unwrap_err();
        let payload: serde_json::Value = serde_json::from_str(&error.reason).unwrap();
        assert_eq!(payload["code"], "NATIVE_ERROR");

        let neighbors = read(store, |graph| {
            encode(&graph.neighbors("a", None, Direction::Both)?)
        })
        .unwrap();
        let neighbors: serde_json::Value = serde_json::from_str(&neighbors).unwrap();
        assert_eq!(neighbors[0]["node"]["id"], "b");
        assert_eq!(neighbors[0]["edge"]["metadata"]["note"], "x");
        assert_eq!(
            write(store, |graph| Ok(graph.remove_node("a"))).unwrap(),
            Some(1)
        );
    }

    #[test]
    fn rejects_empty_ids_and_malformed_payloads() {
        assert_eq!(key("  ").unwrap_err().code(), "VALIDATION_FAILED");
        assert_eq!(
            parse::<Node>("{", "graph node").unwrap_err().code(),
            "VALIDATION_FAILED"
        );
        assert_eq!(
            parse::<SubgraphQuery>(r#"{"id":"a","hops":1,"extra":true}"#, "subgraph query")
                .unwrap_err()
                .code(),
            "VALIDATION_FAILED"
        );
    }
}
