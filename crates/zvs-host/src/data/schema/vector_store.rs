use rusqlite::Row;
use zvs_contract::{
    Timestamp, VectorBackend, VectorEvalRunDto, VectorHealth, VectorMetric, VectorOcrConfig,
    VectorRerankConfig, VectorSearchMode,
};

use super::columns::{EnumColumn, json_column, optional_json_column};

#[derive(Debug, Clone, PartialEq)]
pub struct VectorStoreRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub backend: VectorBackend,
    pub embedding_provider_id: String,
    pub embedding_model_id: String,
    pub dimension: i64,
    pub metric: VectorMetric,
    pub chunk_size: i64,
    pub chunk_overlap: i64,
    pub rerank: VectorRerankConfig,
    pub ocr: VectorOcrConfig,
    pub query_prefix: String,
    pub document_prefix: String,
    pub search_mode: VectorSearchMode,
    pub chat_min_score: f64,
    pub schema_version: i64,
    pub index_type: String,
    pub status: VectorHealth,
    pub last_eval: Option<VectorEvalRunDto>,
    pub graph_stale: bool,
    pub table_created_at: Option<Timestamp>,
    pub last_indexed_at: Option<Timestamp>,
    pub documents: i64,
    pub vectors: i64,
    pub bytes: i64,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl VectorStoreRow {
    pub const COLUMNS: &str = "id, name, description, backend, embedding_provider_id, embedding_model_id, dimension, metric, chunk_size, chunk_overlap, rerank, ocr, query_prefix, document_prefix, search_mode, chat_min_score, schema_version, index_type, status, last_eval, graph_stale, table_created_at, last_indexed_at, documents, vectors, bytes, created_at, updated_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            description: row.get("description")?,
            backend: row.get::<_, EnumColumn<VectorBackend>>("backend")?.0,
            embedding_provider_id: row.get("embedding_provider_id")?,
            embedding_model_id: row.get("embedding_model_id")?,
            dimension: row.get("dimension")?,
            metric: row.get::<_, EnumColumn<VectorMetric>>("metric")?.0,
            chunk_size: row.get("chunk_size")?,
            chunk_overlap: row.get("chunk_overlap")?,
            rerank: json_column(row, "rerank")?,
            ocr: json_column(row, "ocr")?,
            query_prefix: row.get("query_prefix")?,
            document_prefix: row.get("document_prefix")?,
            search_mode: row.get::<_, EnumColumn<VectorSearchMode>>("search_mode")?.0,
            chat_min_score: row.get("chat_min_score")?,
            schema_version: row.get("schema_version")?,
            index_type: row.get("index_type")?,
            status: row.get::<_, EnumColumn<VectorHealth>>("status")?.0,
            last_eval: optional_json_column(row, "last_eval")?,
            graph_stale: row.get("graph_stale")?,
            table_created_at: row.get("table_created_at")?,
            last_indexed_at: row.get("last_indexed_at")?,
            documents: row.get("documents")?,
            vectors: row.get("vectors")?,
            bytes: row.get("bytes")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}
