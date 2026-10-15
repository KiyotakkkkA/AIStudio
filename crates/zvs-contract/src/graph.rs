use serde::{Deserialize, Serialize};

use crate::enums::wire_enum;
use crate::errors::AppError;
use crate::ids::{DocumentId, GraphEdgeId, GraphRelationTypeId, VectorStoreId};
use crate::primitives::{Json, Timestamp};
use crate::validate::{Validate, check, invalid, js_trim, nested, no_refinements};

wire_enum!(GraphNodeKind {
    Document => "document",
    Chunk => "chunk",
});

wire_enum!(GraphDirection {
    Outgoing => "outgoing",
    Incoming => "incoming",
    Both => "both",
});

pub const GRAPH_MAX_HOPS: u64 = 8;
pub const GRAPH_DEFAULT_HOPS: u64 = 2;
pub const GRAPH_MAX_BUDGET: u64 = 65_536;
pub const RELATION_NAME_MAX: usize = 64;
pub const RELATION_DESCRIPTION_MAX: usize = 512;

pub fn is_relation_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}

fn check_weight(weight: Option<f64>) -> Result<(), AppError> {
    weight.map_or(Ok(()), |weight| check::finite("weight", weight))
}

fn check_relation_filter(relation: Option<&str>) -> Result<(), AppError> {
    relation.map_or(Ok(()), |relation| {
        check::trimmed_name("relation", relation, RELATION_NAME_MAX)
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRelationTypeDto {
    pub id: GraphRelationTypeId,
    pub name: String,
    pub description: String,
    pub directed: bool,
}

impl Validate for GraphRelationTypeDto {
    fn validate(&self) -> Result<(), AppError> {
        check::trimmed_name("name", &self.name, RELATION_NAME_MAX)?;
        if !is_relation_name(js_trim(&self.name)) {
            return Err(invalid("name", "regex"));
        }
        check::max_len("description", &self.description, RELATION_DESCRIPTION_MAX)
    }
}

pub type GraphRelationsListOutput = Vec<GraphRelationTypeDto>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdgeDto {
    pub id: GraphEdgeId,
    pub store_id: VectorStoreId,
    pub from_document_id: DocumentId,
    pub to_document_id: DocumentId,
    pub relation_type_id: GraphRelationTypeId,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub weight: Option<f64>,
    pub metadata: Json,
    pub created_at: Timestamp,
}

impl Validate for GraphEdgeDto {
    fn validate(&self) -> Result<(), AppError> {
        check_weight(self.weight)?;
        check::timestamp("createdAt", self.created_at)
    }
}

pub type GraphEdgesListOutput = Vec<GraphEdgeDto>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdgeKey {
    pub store_id: VectorStoreId,
    pub from_document_id: DocumentId,
    pub to_document_id: DocumentId,
    pub relation_type_id: GraphRelationTypeId,
}

pub type RemoveGraphEdgeInput = GraphEdgeKey;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddGraphEdgeInput {
    #[serde(flatten)]
    pub key: GraphEdgeKey,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::optional_nullable"
    )]
    pub weight: Option<Option<f64>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub metadata: Option<Json>,
}

impl AddGraphEdgeInput {
    pub const fn new(key: GraphEdgeKey) -> Self {
        Self {
            key,
            weight: None,
            metadata: None,
        }
    }

    pub fn weight(&self) -> Option<f64> {
        self.weight.flatten()
    }

    pub fn metadata(&self) -> Json {
        self.metadata.clone().unwrap_or(Json::Null)
    }
}

impl Validate for AddGraphEdgeInput {
    fn validate(&self) -> Result<(), AppError> {
        check_weight(self.weight())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRemoveEdgeOutput {
    pub id: GraphEdgeId,
    pub removed: bool,
}

impl Validate for GraphRemoveEdgeOutput {
    fn validate(&self) -> Result<(), AppError> {
        check::literal_true("removed", self.removed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNodeDto {
    pub id: String,
    pub kind: GraphNodeKind,
}

impl Validate for GraphNodeDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("id", &self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphLinkDto {
    pub from: String,
    pub to: String,
    pub relation: String,
    #[serde(deserialize_with = "crate::wire::nullable")]
    pub weight: Option<f64>,
    pub metadata: Json,
}

impl Validate for GraphLinkDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("from", &self.from)?;
        check::non_empty("to", &self.to)?;
        check::non_empty("relation", &self.relation)?;
        check_weight(self.weight)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNeighborDto {
    pub node: GraphNodeDto,
    pub edge: GraphLinkDto,
    pub direction: GraphDirection,
}

impl Validate for GraphNeighborDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("node", self.node.validate())?;
        nested("edge", self.edge.validate())
    }
}

pub type GraphNeighborsOutput = Vec<GraphNeighborDto>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphPathDto {
    pub nodes: Vec<GraphNodeDto>,
    pub edges: Vec<GraphLinkDto>,
}

impl Validate for GraphPathDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("nodes", self.nodes.validate())?;
        nested("edges", self.edges.validate())
    }
}

pub type GraphShortestPathOutput = Option<GraphPathDto>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphSubgraphNodeDto {
    pub node: GraphNodeDto,
    pub hops: u64,
}

impl Validate for GraphSubgraphNodeDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("node", self.node.validate())?;
        check::safe_integer("hops", self.hops)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphSubgraphDto {
    pub root: String,
    pub nodes: Vec<GraphSubgraphNodeDto>,
    pub edges: Vec<GraphLinkDto>,
}

impl Validate for GraphSubgraphDto {
    fn validate(&self) -> Result<(), AppError> {
        nested("nodes", self.nodes.validate())?;
        nested("edges", self.edges.validate())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNeighborsInput {
    pub store_id: VectorStoreId,
    pub document_id: DocumentId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub relation: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub direction: Option<GraphDirection>,
}

impl GraphNeighborsInput {
    pub const fn new(store_id: VectorStoreId, document_id: DocumentId) -> Self {
        Self {
            store_id,
            document_id,
            relation: None,
            direction: None,
        }
    }

    pub fn direction(&self) -> GraphDirection {
        self.direction.unwrap_or(GraphDirection::Both)
    }
}

impl Validate for GraphNeighborsInput {
    fn validate(&self) -> Result<(), AppError> {
        check_relation_filter(self.relation.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphShortestPathInput {
    pub store_id: VectorStoreId,
    pub from_document_id: DocumentId,
    pub to_document_id: DocumentId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub relation: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub direction: Option<GraphDirection>,
}

impl GraphShortestPathInput {
    pub const fn new(store_id: VectorStoreId, from: DocumentId, to: DocumentId) -> Self {
        Self {
            store_id,
            from_document_id: from,
            to_document_id: to,
            relation: None,
            direction: None,
        }
    }

    pub fn direction(&self) -> GraphDirection {
        self.direction.unwrap_or(GraphDirection::Both)
    }
}

impl Validate for GraphShortestPathInput {
    fn validate(&self) -> Result<(), AppError> {
        check_relation_filter(self.relation.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphSubgraphInput {
    pub store_id: VectorStoreId,
    pub document_id: DocumentId,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub hops: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub relation: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub direction: Option<GraphDirection>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::wire::present"
    )]
    pub budget: Option<u64>,
}

impl GraphSubgraphInput {
    pub const fn new(store_id: VectorStoreId, document_id: DocumentId) -> Self {
        Self {
            store_id,
            document_id,
            hops: None,
            relation: None,
            direction: None,
            budget: None,
        }
    }

    pub fn hops(&self) -> u64 {
        self.hops.unwrap_or(GRAPH_DEFAULT_HOPS)
    }

    pub fn direction(&self) -> GraphDirection {
        self.direction.unwrap_or(GraphDirection::Both)
    }
}

impl Validate for GraphSubgraphInput {
    fn validate(&self) -> Result<(), AppError> {
        check::int_range("hops", self.hops(), 1, GRAPH_MAX_HOPS)?;
        check_relation_filter(self.relation.as_deref())?;
        if let Some(budget) = self.budget {
            check::int_range("budget", budget, 1, GRAPH_MAX_BUDGET)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphReconcileDto {
    pub store_id: VectorStoreId,
    pub recorded_edges: u64,
    pub engine_edges: u64,
    pub graph_stale: bool,
}

impl Validate for GraphReconcileDto {
    fn validate(&self) -> Result<(), AppError> {
        check::safe_integer("recordedEdges", self.recorded_edges)?;
        check::safe_integer("engineEdges", self.engine_edges)
    }
}

no_refinements!(GraphEdgeKey);

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::validate::parse;

    const STORE: &str = "0199aa11-1111-7111-8111-111111111111";
    const FROM: &str = "0199aa11-2222-7111-8111-111111111111";
    const TO: &str = "0199aa11-3333-7111-8111-111111111111";
    const RELATION: &str = "01999a00-0001-7000-8000-000000000001";

    #[test]
    fn relation_names_follow_the_catalogue_pattern() {
        assert!(is_relation_name("cites"));
        assert!(is_relation_name("derived_from2"));
        assert!(!is_relation_name("Cites"));
        assert!(!is_relation_name("2cites"));
        assert!(!is_relation_name(""));
        assert!(!is_relation_name("co-occurs"));
    }

    #[test]
    fn an_edge_input_defaults_weight_and_metadata_to_null() {
        let input = parse::<AddGraphEdgeInput>(json!({
            "storeId": STORE,
            "fromDocumentId": FROM,
            "toDocumentId": TO,
            "relationTypeId": RELATION,
        }))
        .expect("parses");
        assert_eq!(input.weight(), None);
        assert_eq!(input.metadata(), Json::Null);
        let weighted = parse::<AddGraphEdgeInput>(json!({
            "storeId": STORE,
            "fromDocumentId": FROM,
            "toDocumentId": TO,
            "relationTypeId": RELATION,
            "weight": 0.5,
            "metadata": { "note": "x" },
        }))
        .expect("parses");
        assert_eq!(weighted.weight(), Some(0.5));
        assert_eq!(weighted.metadata(), json!({ "note": "x" }));
    }

    #[test]
    fn a_subgraph_query_is_bounded() {
        let base = json!({ "storeId": STORE, "documentId": FROM });
        let input = parse::<GraphSubgraphInput>(base).expect("parses");
        assert_eq!(input.hops(), 2);
        assert_eq!(input.direction(), GraphDirection::Both);
        for (key, value) in [("hops", json!(9)), ("hops", json!(0)), ("budget", json!(0))] {
            let mut payload = json!({ "storeId": STORE, "documentId": FROM });
            payload[key] = value;
            assert!(parse::<GraphSubgraphInput>(payload).is_err(), "{key}");
        }
        assert!(
            parse::<GraphNeighborsInput>(json!({
                "storeId": STORE,
                "documentId": FROM,
                "relation": "  ",
            }))
            .is_err()
        );
    }
}
