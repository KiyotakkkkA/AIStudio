use std::collections::{HashMap, HashSet, VecDeque};

use petgraph::{
    Direction as Flow,
    stable_graph::{EdgeIndex, NodeIndex, StableDiGraph},
    visit::EdgeRef,
};
use serde::{Deserialize, Serialize};

use crate::{CancellationToken, Error, Result, error::check_cancelled};

pub const MAX_HOPS: u32 = 8;
pub const DEFAULT_NODE_BUDGET: usize = 4_096;
pub const MAX_NODE_BUDGET: usize = 65_536;

const CANCEL_STRIDE: usize = 256;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Document,
    Chunk,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub relation: String,
    #[serde(default)]
    pub weight: Option<f32>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Outgoing,
    Incoming,
    #[default]
    Both,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Neighbor {
    pub node: Node,
    pub edge: Edge,
    pub direction: Direction,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphPath {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubgraphNode {
    pub node: Node,
    pub hops: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Subgraph {
    pub root: String,
    pub nodes: Vec<SubgraphNode>,
    pub edges: Vec<Edge>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub document_count: usize,
    pub chunk_count: usize,
}

#[derive(Clone, Debug)]
struct EdgeData {
    relation: String,
    weight: Option<f32>,
    metadata: serde_json::Value,
}

#[derive(Debug, Default)]
pub struct GraphStore {
    graph: StableDiGraph<Node, EdgeData>,
    ids: HashMap<String, NodeIndex>,
}

fn flows(direction: Direction) -> &'static [(Direction, Flow)] {
    match direction {
        Direction::Outgoing => &[(Direction::Outgoing, Flow::Outgoing)],
        Direction::Incoming => &[(Direction::Incoming, Flow::Incoming)],
        Direction::Both => &[
            (Direction::Outgoing, Flow::Outgoing),
            (Direction::Incoming, Flow::Incoming),
        ],
    }
}

fn selected(relation: Option<&str>, candidate: &str) -> bool {
    relation.is_none_or(|wanted| wanted == candidate)
}

impl GraphStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hydrate(nodes: &[Node], edges: &[Edge], token: &CancellationToken) -> Result<Self> {
        let mut store = Self::new();
        store.add_nodes(nodes, token)?;
        store.add_edges(edges, token)?;
        Ok(store)
    }

    pub fn contains_node(&self, id: &str) -> bool {
        self.ids.contains_key(id)
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.ids
            .get(id)
            .and_then(|index| self.graph.node_weight(*index))
    }

    pub fn stats(&self) -> GraphStats {
        let document_count = self
            .graph
            .node_weights()
            .filter(|node| node.kind == NodeKind::Document)
            .count();
        GraphStats {
            node_count: self.graph.node_count(),
            edge_count: self.graph.edge_count(),
            document_count,
            chunk_count: self.graph.node_count() - document_count,
        }
    }

    pub fn nodes(&self) -> Vec<Node> {
        let mut nodes = self.graph.node_weights().cloned().collect::<Vec<_>>();
        nodes.sort_by(|left, right| left.id.cmp(&right.id));
        nodes
    }

    pub fn edges(&self) -> Vec<Edge> {
        let mut edges = self
            .graph
            .edge_indices()
            .filter_map(|index| self.edge_at(index))
            .collect::<Vec<_>>();
        sort_edges(&mut edges);
        edges
    }

    pub fn add_node(&mut self, node: Node) -> Result<bool> {
        let id = validated(&node.id, "Graph node id")?;
        if let Some(index) = self.ids.get(&id) {
            if let Some(existing) = self.graph.node_weight_mut(*index) {
                existing.kind = node.kind;
            }
            return Ok(false);
        }
        let index = self.graph.add_node(Node {
            id: id.clone(),
            kind: node.kind,
        });
        self.ids.insert(id, index);
        Ok(true)
    }

    pub fn add_nodes(&mut self, nodes: &[Node], token: &CancellationToken) -> Result<usize> {
        check_cancelled(token)?;
        for node in nodes {
            validated(&node.id, "Graph node id")?;
        }
        let mut added = 0;
        for (position, node) in nodes.iter().enumerate() {
            if position.is_multiple_of(CANCEL_STRIDE) {
                check_cancelled(token)?;
            }
            if self.add_node(node.clone())? {
                added += 1;
            }
        }
        check_cancelled(token)?;
        Ok(added)
    }

    pub fn add_edge(&mut self, edge: Edge) -> Result<bool> {
        let (from, to, relation) = self.validated_edge(&edge)?;
        let data = EdgeData {
            relation,
            weight: edge.weight,
            metadata: edge.metadata,
        };
        if let Some(index) = self.edge_between(from, to, &data.relation) {
            if let Some(existing) = self.graph.edge_weight_mut(index) {
                *existing = data;
            }
            return Ok(false);
        }
        self.graph.add_edge(from, to, data);
        Ok(true)
    }

    pub fn add_edges(&mut self, edges: &[Edge], token: &CancellationToken) -> Result<usize> {
        check_cancelled(token)?;
        for edge in edges {
            self.validated_edge(edge)?;
        }
        let mut added = 0;
        for (position, edge) in edges.iter().enumerate() {
            if position.is_multiple_of(CANCEL_STRIDE) {
                check_cancelled(token)?;
            }
            if self.add_edge(edge.clone())? {
                added += 1;
            }
        }
        check_cancelled(token)?;
        Ok(added)
    }

    pub fn remove_edge(&mut self, from: &str, to: &str, relation: &str) -> bool {
        let (Some(from), Some(to)) = (self.ids.get(from).copied(), self.ids.get(to).copied())
        else {
            return false;
        };
        match self.edge_between(from, to, relation) {
            Some(index) => self.graph.remove_edge(index).is_some(),
            None => false,
        }
    }

    pub fn remove_edges_touching(&mut self, id: &str) -> Option<usize> {
        let index = self.ids.get(id).copied()?;
        Some(self.detach(index))
    }

    pub fn remove_node(&mut self, id: &str) -> Option<usize> {
        let index = self.ids.remove(id)?;
        let removed = self.detach(index);
        self.graph.remove_node(index);
        Some(removed)
    }

    pub fn remove_nodes(&mut self, ids: &[String], token: &CancellationToken) -> Result<usize> {
        check_cancelled(token)?;
        let mut removed = 0;
        for (position, id) in ids.iter().enumerate() {
            if position.is_multiple_of(CANCEL_STRIDE) {
                check_cancelled(token)?;
            }
            if self.remove_node(id).is_some() {
                removed += 1;
            }
        }
        check_cancelled(token)?;
        Ok(removed)
    }

    pub fn neighbors(
        &self,
        id: &str,
        relation: Option<&str>,
        direction: Direction,
    ) -> Result<Vec<Neighbor>> {
        let index = self.require(id)?;
        let mut neighbors = Vec::new();
        for (side, flow) in flows(direction) {
            for reference in self.graph.edges_directed(index, *flow) {
                if !selected(relation, &reference.weight().relation) {
                    continue;
                }
                let other = match flow {
                    Flow::Outgoing => reference.target(),
                    Flow::Incoming => reference.source(),
                };
                let (Some(node), Some(edge)) =
                    (self.graph.node_weight(other), self.edge_at(reference.id()))
                else {
                    continue;
                };
                neighbors.push(Neighbor {
                    node: node.clone(),
                    edge,
                    direction: *side,
                });
            }
        }
        neighbors.sort_by(|left, right| {
            left.node
                .id
                .cmp(&right.node.id)
                .then_with(|| left.edge.relation.cmp(&right.edge.relation))
                .then_with(|| left.direction.cmp(&right.direction))
        });
        Ok(neighbors)
    }

    pub fn shortest_path(
        &self,
        from: &str,
        to: &str,
        relation: Option<&str>,
        direction: Direction,
        token: &CancellationToken,
    ) -> Result<Option<GraphPath>> {
        check_cancelled(token)?;
        let source = self.require(from)?;
        let target = self.require(to)?;
        if source == target {
            return Ok(self.graph.node_weight(source).map(|node| GraphPath {
                nodes: vec![node.clone()],
                edges: Vec::new(),
            }));
        }
        let mut came_from: HashMap<NodeIndex, (NodeIndex, EdgeIndex)> = HashMap::new();
        let mut seen = HashSet::from([source]);
        let mut queue = VecDeque::from([source]);
        let mut visited = 0usize;
        while let Some(current) = queue.pop_front() {
            visited += 1;
            if visited.is_multiple_of(CANCEL_STRIDE) {
                check_cancelled(token)?;
            }
            for (next, edge) in self.step(current, relation, direction) {
                if !seen.insert(next) {
                    continue;
                }
                came_from.insert(next, (current, edge));
                if next == target {
                    check_cancelled(token)?;
                    return Ok(self.rebuild(source, target, &came_from));
                }
                queue.push_back(next);
            }
        }
        check_cancelled(token)?;
        Ok(None)
    }

    pub fn k_hop_subgraph(
        &self,
        id: &str,
        hops: u32,
        relation: Option<&str>,
        direction: Direction,
        budget: Option<usize>,
        token: &CancellationToken,
    ) -> Result<Subgraph> {
        check_cancelled(token)?;
        if hops > MAX_HOPS {
            return Err(Error::InvalidInput(format!(
                "A k-hop query accepts at most {MAX_HOPS} hops; {hops} were requested"
            )));
        }
        let budget = budget.unwrap_or(DEFAULT_NODE_BUDGET);
        if budget == 0 || budget > MAX_NODE_BUDGET {
            return Err(Error::InvalidInput(format!(
                "A k-hop node budget must be between 1 and {MAX_NODE_BUDGET}; {budget} was requested"
            )));
        }
        let root = self.require(id)?;
        let mut depths = HashMap::from([(root, 0u32)]);
        let mut frontier = vec![root];
        for depth in 1..=hops {
            check_cancelled(token)?;
            let mut next = Vec::new();
            for current in frontier {
                for (neighbor, _) in self.step(current, relation, direction) {
                    if depths.contains_key(&neighbor) {
                        continue;
                    }
                    if depths.len() >= budget {
                        return Err(Error::LimitExceeded(format!(
                            "A {hops}-hop subgraph around {id} exceeds the {budget} node budget"
                        )));
                    }
                    depths.insert(neighbor, depth);
                    next.push(neighbor);
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        let mut nodes = Vec::with_capacity(depths.len());
        let mut edges = Vec::new();
        for (index, depth) in &depths {
            check_cancelled(token)?;
            if let Some(node) = self.graph.node_weight(*index) {
                nodes.push(SubgraphNode {
                    node: node.clone(),
                    hops: *depth,
                });
            }
            for reference in self.graph.edges_directed(*index, Flow::Outgoing) {
                if !selected(relation, &reference.weight().relation)
                    || !depths.contains_key(&reference.target())
                {
                    continue;
                }
                if let Some(edge) = self.edge_at(reference.id()) {
                    edges.push(edge);
                }
            }
        }
        nodes.sort_by(|left, right| {
            left.hops
                .cmp(&right.hops)
                .then_with(|| left.node.id.cmp(&right.node.id))
        });
        sort_edges(&mut edges);
        check_cancelled(token)?;
        Ok(Subgraph {
            root: id.to_owned(),
            nodes,
            edges,
        })
    }

    fn detach(&mut self, index: NodeIndex) -> usize {
        let doomed = self
            .graph
            .edges_directed(index, Flow::Outgoing)
            .chain(self.graph.edges_directed(index, Flow::Incoming))
            .map(|reference| reference.id())
            .collect::<HashSet<_>>();
        doomed
            .into_iter()
            .filter(|edge| self.graph.remove_edge(*edge).is_some())
            .count()
    }

    fn step(
        &self,
        index: NodeIndex,
        relation: Option<&str>,
        direction: Direction,
    ) -> Vec<(NodeIndex, EdgeIndex)> {
        let mut steps = Vec::new();
        for (_, flow) in flows(direction) {
            for reference in self.graph.edges_directed(index, *flow) {
                if !selected(relation, &reference.weight().relation) {
                    continue;
                }
                let other = match flow {
                    Flow::Outgoing => reference.target(),
                    Flow::Incoming => reference.source(),
                };
                steps.push((other, reference.id()));
            }
        }
        steps.sort_by(|left, right| {
            let ordering = match (
                self.graph.node_weight(left.0),
                self.graph.node_weight(right.0),
            ) {
                (Some(left), Some(right)) => left.id.cmp(&right.id),
                _ => std::cmp::Ordering::Equal,
            };
            ordering.then_with(|| {
                self.graph[left.1]
                    .relation
                    .cmp(&self.graph[right.1].relation)
            })
        });
        steps
    }

    fn rebuild(
        &self,
        source: NodeIndex,
        target: NodeIndex,
        came_from: &HashMap<NodeIndex, (NodeIndex, EdgeIndex)>,
    ) -> Option<GraphPath> {
        let mut nodes = vec![self.graph.node_weight(target)?.clone()];
        let mut edges = Vec::new();
        let mut cursor = target;
        while cursor != source {
            let (previous, edge) = came_from.get(&cursor)?;
            edges.push(self.edge_at(*edge)?);
            nodes.push(self.graph.node_weight(*previous)?.clone());
            cursor = *previous;
        }
        nodes.reverse();
        edges.reverse();
        Some(GraphPath { nodes, edges })
    }

    fn require(&self, id: &str) -> Result<NodeIndex> {
        self.ids
            .get(id)
            .copied()
            .ok_or_else(|| Error::InvalidInput(format!("Unknown graph node: {id}")))
    }

    fn edge_between(&self, from: NodeIndex, to: NodeIndex, relation: &str) -> Option<EdgeIndex> {
        self.graph
            .edges_directed(from, Flow::Outgoing)
            .find(|reference| reference.target() == to && reference.weight().relation == relation)
            .map(|reference| reference.id())
    }

    fn edge_at(&self, index: EdgeIndex) -> Option<Edge> {
        let (from, to) = self.graph.edge_endpoints(index)?;
        let data = self.graph.edge_weight(index)?;
        Some(Edge {
            from: self.graph.node_weight(from)?.id.clone(),
            to: self.graph.node_weight(to)?.id.clone(),
            relation: data.relation.clone(),
            weight: data.weight,
            metadata: data.metadata.clone(),
        })
    }

    fn validated_edge(&self, edge: &Edge) -> Result<(NodeIndex, NodeIndex, String)> {
        let from = validated(&edge.from, "Graph edge source")?;
        let to = validated(&edge.to, "Graph edge target")?;
        let relation = validated(&edge.relation, "Graph edge relation")?;
        if from == to {
            return Err(Error::InvalidInput(format!(
                "A graph edge cannot connect {from} to itself"
            )));
        }
        if edge.weight.is_some_and(|weight| !weight.is_finite()) {
            return Err(Error::InvalidInput(
                "A graph edge weight must be finite".into(),
            ));
        }
        Ok((self.require(&from)?, self.require(&to)?, relation))
    }
}

fn sort_edges(edges: &mut [Edge]) {
    edges.sort_by(|left, right| {
        left.from
            .cmp(&right.from)
            .then_with(|| left.to.cmp(&right.to))
            .then_with(|| left.relation.cmp(&right.relation))
    });
}

fn validated(value: &str, label: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidInput(format!("{label} must not be empty")));
    }
    Ok(trimmed.to_owned())
}
