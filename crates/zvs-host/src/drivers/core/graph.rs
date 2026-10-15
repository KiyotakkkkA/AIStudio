use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use zvs_contract::{AppError, AppErrorCode};
use zvs_core::graph::GraphStore;

use super::{
    GRAPH_REQUEST_MESSAGE, GraphContent, GraphDirection, GraphEdge, GraphNode, MAX_HOPS,
    MAX_NODE_BUDGET,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeighborQuery {
    pub id: String,
    pub relation: Option<String>,
    pub direction: GraphDirection,
}

impl NeighborQuery {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            relation: None,
            direction: GraphDirection::Both,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathQuery {
    pub from: String,
    pub to: String,
    pub relation: Option<String>,
    pub direction: GraphDirection,
}

impl PathQuery {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            relation: None,
            direction: GraphDirection::Both,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubgraphQuery {
    pub id: String,
    pub hops: u32,
    pub relation: Option<String>,
    pub direction: GraphDirection,
    pub budget: Option<usize>,
}

impl SubgraphQuery {
    pub fn new(id: impl Into<String>, hops: u32) -> Self {
        Self {
            id: id.into(),
            hops,
            relation: None,
            direction: GraphDirection::Both,
            budget: None,
        }
    }
}

fn invalid() -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, GRAPH_REQUEST_MESSAGE)
}

pub(super) fn required(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(invalid())
    } else {
        Ok(trimmed.to_owned())
    }
}

fn optional(value: Option<String>) -> Result<Option<String>, AppError> {
    value.as_deref().map(required).transpose()
}

pub(super) fn node(node: GraphNode) -> Result<GraphNode, AppError> {
    Ok(GraphNode {
        id: required(&node.id)?,
        kind: node.kind,
    })
}

pub(super) fn edge(edge: GraphEdge) -> Result<GraphEdge, AppError> {
    if edge.weight.is_some_and(|weight| !weight.is_finite()) {
        return Err(invalid());
    }
    Ok(GraphEdge {
        from: required(&edge.from)?,
        to: required(&edge.to)?,
        relation: required(&edge.relation)?,
        weight: edge.weight,
        metadata: edge.metadata,
    })
}

pub(super) fn content(content: GraphContent) -> Result<GraphContent, AppError> {
    Ok(GraphContent {
        nodes: content
            .nodes
            .into_iter()
            .map(node)
            .collect::<Result<_, _>>()?,
        edges: content
            .edges
            .into_iter()
            .map(edge)
            .collect::<Result<_, _>>()?,
    })
}

pub(super) fn neighbor_query(query: NeighborQuery) -> Result<NeighborQuery, AppError> {
    Ok(NeighborQuery {
        id: required(&query.id)?,
        relation: optional(query.relation)?,
        direction: query.direction,
    })
}

pub(super) fn path_query(query: PathQuery) -> Result<PathQuery, AppError> {
    Ok(PathQuery {
        from: required(&query.from)?,
        to: required(&query.to)?,
        relation: optional(query.relation)?,
        direction: query.direction,
    })
}

pub(super) fn subgraph_query(query: SubgraphQuery) -> Result<SubgraphQuery, AppError> {
    if query.hops > MAX_HOPS
        || query
            .budget
            .is_some_and(|budget| budget == 0 || budget > MAX_NODE_BUDGET)
    {
        return Err(invalid());
    }
    Ok(SubgraphQuery {
        id: required(&query.id)?,
        hops: query.hops,
        relation: optional(query.relation)?,
        direction: query.direction,
        budget: query.budget,
    })
}

#[derive(Debug, Default)]
pub struct GraphRegistry(Mutex<HashMap<String, GraphStore>>);

impl GraphRegistry {
    pub fn replace(&self, graph_id: String, store: GraphStore) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(graph_id, store);
    }

    pub fn write<T>(
        &self,
        graph_id: &str,
        work: impl FnOnce(&mut GraphStore) -> zvs_core::Result<T>,
    ) -> zvs_core::Result<T> {
        let mut graphs = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        work(graphs.entry(graph_id.to_owned()).or_default())
    }

    pub fn read<T>(
        &self,
        graph_id: &str,
        work: impl FnOnce(&GraphStore) -> zvs_core::Result<T>,
    ) -> zvs_core::Result<T> {
        let graphs = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match graphs.get(graph_id) {
            Some(store) => work(store),
            None => work(&GraphStore::new()),
        }
    }

    pub fn len(&self) -> usize {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
