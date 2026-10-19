use std::collections::{HashMap, HashSet};

use crate::graph::{GraphDirection, GraphNeighborDto, GraphNodeKind};

pub const GRAPH_EXPANSION_DEFAULT_MAX: usize = 5;

#[derive(Debug, Clone, PartialEq)]
pub struct GraphNeighborhood {
    pub document_id: String,
    pub neighbors: Vec<GraphNeighborDto>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphExpansion {
    pub document_id: String,
    pub relation: String,
    pub direction: GraphDirection,
    pub via_document_id: String,
    pub path: String,
    pub chunk_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphExpansionDocument {
    pub id: String,
    pub source_path: String,
    pub chunk_count: u64,
}

pub fn merge_graph_neighbors<S: AsRef<str>>(
    hits: &[S],
    neighborhoods: &[GraphNeighborhood],
    documents: &[GraphExpansionDocument],
    cap: usize,
) -> Vec<GraphExpansion> {
    if cap == 0 {
        return Vec::new();
    }
    let ranked: HashSet<&str> = hits.iter().map(AsRef::as_ref).collect();
    let by_id: HashMap<&str, &GraphExpansionDocument> = documents
        .iter()
        .map(|document| (document.id.as_str(), document))
        .collect();
    let mut taken: HashSet<&str> = HashSet::new();
    let mut expansions = Vec::new();
    for neighborhood in neighborhoods {
        for neighbor in &neighborhood.neighbors {
            if expansions.len() >= cap {
                return expansions;
            }
            let document_id = neighbor.node.id.as_str();
            if neighbor.node.kind != GraphNodeKind::Document
                || ranked.contains(document_id)
                || !taken.insert(document_id)
            {
                continue;
            }
            let document = by_id.get(document_id);
            expansions.push(GraphExpansion {
                document_id: document_id.to_owned(),
                relation: neighbor.edge.relation.clone(),
                direction: neighbor.direction,
                via_document_id: neighborhood.document_id.clone(),
                path: document.map_or_else(
                    || document_id.to_owned(),
                    |document| document.source_path.clone(),
                ),
                chunk_count: document.map(|document| document.chunk_count),
            });
        }
    }
    expansions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{GraphLinkDto, GraphNodeDto};
    use serde_json::Value;

    fn neighbor(id: &str, kind: GraphNodeKind, relation: &str) -> GraphNeighborDto {
        GraphNeighborDto {
            node: GraphNodeDto {
                id: id.to_owned(),
                kind,
            },
            edge: GraphLinkDto {
                from: "a".to_owned(),
                to: id.to_owned(),
                relation: relation.to_owned(),
                weight: None,
                metadata: Value::Null,
            },
            direction: GraphDirection::Outgoing,
        }
    }

    #[test]
    fn neighbours_skip_ranked_repeated_and_non_document_nodes_up_to_the_cap() {
        let neighborhoods = [
            GraphNeighborhood {
                document_id: "a".to_owned(),
                neighbors: vec![
                    neighbor("a", GraphNodeKind::Document, "self"),
                    neighbor("b", GraphNodeKind::Document, "references"),
                    neighbor("e", GraphNodeKind::Chunk, "mentions"),
                    neighbor("b", GraphNodeKind::Document, "duplicate"),
                    neighbor("c", GraphNodeKind::Document, "cites"),
                ],
            },
            GraphNeighborhood {
                document_id: "z".to_owned(),
                neighbors: vec![neighbor("d", GraphNodeKind::Document, "cites")],
            },
        ];
        let documents = [GraphExpansionDocument {
            id: "b".to_owned(),
            source_path: "docs/b.md".to_owned(),
            chunk_count: 4,
        }];
        let merged = merge_graph_neighbors(&["a"], &neighborhoods, &documents, 2);
        assert_eq!(
            merged,
            [
                GraphExpansion {
                    document_id: "b".to_owned(),
                    relation: "references".to_owned(),
                    direction: GraphDirection::Outgoing,
                    via_document_id: "a".to_owned(),
                    path: "docs/b.md".to_owned(),
                    chunk_count: Some(4),
                },
                GraphExpansion {
                    document_id: "c".to_owned(),
                    relation: "cites".to_owned(),
                    direction: GraphDirection::Outgoing,
                    via_document_id: "a".to_owned(),
                    path: "c".to_owned(),
                    chunk_count: None,
                },
            ]
        );
        assert!(merge_graph_neighbors(&["a"], &neighborhoods, &documents, 0).is_empty());
    }
}
