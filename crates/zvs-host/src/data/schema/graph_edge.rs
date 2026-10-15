use rusqlite::Row;
use serde_json::Value;
use zvs_contract::Timestamp;

use super::columns::optional_json_column;

#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdgeRow {
    pub id: String,
    pub store_id: String,
    pub from_document_id: String,
    pub to_document_id: String,
    pub relation_type_id: String,
    pub weight: Option<f64>,
    pub metadata: Option<Value>,
    pub created_at: Timestamp,
}

impl GraphEdgeRow {
    pub const COLUMNS: &str = "id, store_id, from_document_id, to_document_id, relation_type_id, weight, metadata, created_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            store_id: row.get("store_id")?,
            from_document_id: row.get("from_document_id")?,
            to_document_id: row.get("to_document_id")?,
            relation_type_id: row.get("relation_type_id")?,
            weight: row.get("weight")?,
            metadata: optional_json_column(row, "metadata")?,
            created_at: row.get("created_at")?,
        })
    }

    pub fn metadata(&self) -> Value {
        self.metadata.clone().unwrap_or(Value::Null)
    }
}
