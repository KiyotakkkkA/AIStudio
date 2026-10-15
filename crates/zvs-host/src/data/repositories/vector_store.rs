use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use zvs_contract::{
    Timestamp, VectorBackend, VectorEvalRunDto, VectorHealth, VectorMetric, VectorOcrConfig,
    VectorRerankConfig, VectorSearchMode,
};

use crate::data::schema::VectorStoreRow;
use crate::data::schema::columns::json_text;

#[derive(Debug, Clone, PartialEq)]
pub struct VectorStoreDraft {
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
    pub index_type: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl VectorStoreDraft {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        embedding_provider_id: impl Into<String>,
        embedding_model_id: impl Into<String>,
        dimension: i64,
        now: Timestamp,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            backend: VectorBackend::Lancedb,
            embedding_provider_id: embedding_provider_id.into(),
            embedding_model_id: embedding_model_id.into(),
            dimension,
            metric: VectorMetric::Cosine,
            chunk_size: 256,
            chunk_overlap: 32,
            rerank: VectorRerankConfig::default(),
            ocr: VectorOcrConfig::default(),
            query_prefix: String::new(),
            document_prefix: String::new(),
            search_mode: VectorSearchMode::Hybrid,
            chat_min_score: 0.3,
            index_type: "FLAT".to_owned(),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VectorStorePatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub chunk_size: Option<i64>,
    pub chunk_overlap: Option<i64>,
    pub rerank: Option<VectorRerankConfig>,
    pub ocr: Option<VectorOcrConfig>,
    pub query_prefix: Option<String>,
    pub document_prefix: Option<String>,
    pub search_mode: Option<VectorSearchMode>,
    pub chat_min_score: Option<f64>,
    pub schema_version: Option<i64>,
    pub index_type: Option<String>,
    pub status: Option<VectorHealth>,
    pub last_eval: Option<Option<VectorEvalRunDto>>,
    pub graph_stale: Option<bool>,
    pub table_created_at: Option<Option<Timestamp>>,
    pub last_indexed_at: Option<Option<Timestamp>>,
    pub documents: Option<i64>,
    pub vectors: Option<i64>,
    pub bytes: Option<i64>,
    pub updated_at: Option<Timestamp>,
}

impl VectorStorePatch {
    pub fn status(status: VectorHealth, now: Timestamp) -> Self {
        Self {
            status: Some(status),
            updated_at: Some(now),
            ..Self::default()
        }
    }

    fn assignments(&self) -> rusqlite::Result<Vec<(&'static str, SqlValue)>> {
        let text = |value: &str| SqlValue::Text(value.to_owned());
        let nullable = |value: Option<Timestamp>| value.map_or(SqlValue::Null, SqlValue::Integer);
        let mut assignments = Vec::new();
        let mut push = |column: &'static str, value: Option<SqlValue>| {
            if let Some(value) = value {
                assignments.push((column, value));
            }
        };
        push("name", self.name.as_deref().map(text));
        push("description", self.description.as_deref().map(text));
        push("chunk_size", self.chunk_size.map(SqlValue::Integer));
        push("chunk_overlap", self.chunk_overlap.map(SqlValue::Integer));
        push(
            "rerank",
            self.rerank
                .as_ref()
                .map(json_text)
                .transpose()?
                .map(SqlValue::Text),
        );
        push(
            "ocr",
            self.ocr
                .as_ref()
                .map(json_text)
                .transpose()?
                .map(SqlValue::Text),
        );
        push("query_prefix", self.query_prefix.as_deref().map(text));
        push("document_prefix", self.document_prefix.as_deref().map(text));
        push(
            "search_mode",
            self.search_mode.map(|mode| text(mode.as_str())),
        );
        push("chat_min_score", self.chat_min_score.map(SqlValue::Real));
        push("schema_version", self.schema_version.map(SqlValue::Integer));
        push("index_type", self.index_type.as_deref().map(text));
        push("status", self.status.map(|status| text(status.as_str())));
        push(
            "last_eval",
            match &self.last_eval {
                None => None,
                Some(None) => Some(SqlValue::Null),
                Some(Some(run)) => Some(SqlValue::Text(json_text(run)?)),
            },
        );
        push(
            "graph_stale",
            self.graph_stale
                .map(|stale| SqlValue::Integer(i64::from(stale))),
        );
        push("table_created_at", self.table_created_at.map(nullable));
        push("last_indexed_at", self.last_indexed_at.map(nullable));
        push("documents", self.documents.map(SqlValue::Integer));
        push("vectors", self.vectors.map(SqlValue::Integer));
        push("bytes", self.bytes.map(SqlValue::Integer));
        push("updated_at", self.updated_at.map(SqlValue::Integer));
        Ok(assignments)
    }
}

pub struct VectorStoreRepository<'c> {
    connection: &'c Connection,
}

impl<'c> VectorStoreRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self) -> rusqlite::Result<Vec<VectorStoreRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM vector_store ORDER BY name, id",
                VectorStoreRow::COLUMNS
            ))?
            .query_map([], VectorStoreRow::from_row)?
            .collect()
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<VectorStoreRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM vector_store WHERE id = ?1",
                VectorStoreRow::COLUMNS
            ))?
            .query_row([id], VectorStoreRow::from_row)
            .optional()
    }

    pub fn find_by_name(&self, name: &str) -> rusqlite::Result<Option<VectorStoreRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM vector_store WHERE name = ?1",
                VectorStoreRow::COLUMNS
            ))?
            .query_row([name], VectorStoreRow::from_row)
            .optional()
    }

    pub fn create(&self, draft: &VectorStoreDraft) -> rusqlite::Result<VectorStoreRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO vector_store (id, name, description, backend, embedding_provider_id, embedding_model_id, dimension, metric, chunk_size, chunk_overlap, rerank, ocr, query_prefix, document_prefix, search_mode, chat_min_score, index_type, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19) RETURNING {}",
                VectorStoreRow::COLUMNS
            ))?
            .query_row(
                params![
                    draft.id,
                    draft.name,
                    draft.description,
                    draft.backend.as_str(),
                    draft.embedding_provider_id,
                    draft.embedding_model_id,
                    draft.dimension,
                    draft.metric.as_str(),
                    draft.chunk_size,
                    draft.chunk_overlap,
                    json_text(&draft.rerank)?,
                    json_text(&draft.ocr)?,
                    draft.query_prefix,
                    draft.document_prefix,
                    draft.search_mode.as_str(),
                    draft.chat_min_score,
                    draft.index_type,
                    draft.created_at,
                    draft.updated_at,
                ],
                VectorStoreRow::from_row,
            )
    }

    pub fn update(
        &self,
        id: &str,
        patch: &VectorStorePatch,
    ) -> rusqlite::Result<Option<VectorStoreRow>> {
        let assignments = patch.assignments()?;
        if assignments.is_empty() {
            return self.find_by_id(id);
        }
        let set = assignments
            .iter()
            .map(|(column, _)| format!("{column} = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        let values = assignments
            .into_iter()
            .map(|(_, value)| value)
            .chain([SqlValue::Text(id.to_owned())]);
        self.connection
            .prepare_cached(&format!(
                "UPDATE vector_store SET {set} WHERE id = ? RETURNING {}",
                VectorStoreRow::COLUMNS
            ))?
            .query_row(params_from_iter(values), VectorStoreRow::from_row)
            .optional()
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached("DELETE FROM vector_store WHERE id = ?1")?
            .execute([id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::repositories::Repositories;
    use crate::testing::{seed_embedding_provider as provider, temporary_database};

    #[test]
    fn a_store_is_created_with_the_schema_defaults_and_patched_column_by_column() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let provider = provider(connection);
                let stores = Repositories::new(connection).vector_stores;
                let created = stores.create(&VectorStoreDraft::new(
                    "s1",
                    "Knowledge",
                    &provider,
                    "embed",
                    3,
                    10,
                ))?;
                assert_eq!(created.status, VectorHealth::Pending);
                assert_eq!(created.schema_version, 1);
                assert_eq!(created.table_created_at, None);
                assert_eq!(created.last_eval, None);
                assert!(!created.graph_stale);
                assert_eq!(created.rerank, VectorRerankConfig::default());

                let patched = stores
                    .update(
                        "s1",
                        &VectorStorePatch {
                            name: Some("Renamed".to_owned()),
                            graph_stale: Some(true),
                            table_created_at: Some(Some(20)),
                            chat_min_score: Some(0.5),
                            ..VectorStorePatch::default()
                        },
                    )?
                    .expect("the row");
                assert_eq!(patched.name, "Renamed");
                assert!(patched.graph_stale);
                assert_eq!(patched.table_created_at, Some(20));
                assert_eq!(patched.updated_at, 10);
                assert!((patched.chat_min_score - 0.5).abs() < f64::EPSILON);
                assert_eq!(stores.find_by_name("Renamed")?, Some(patched.clone()));

                let cleared = stores
                    .update(
                        "s1",
                        &VectorStorePatch {
                            table_created_at: Some(None),
                            ..VectorStorePatch::status(VectorHealth::Broken, 30)
                        },
                    )?
                    .expect("the row");
                assert_eq!(cleared.table_created_at, None);
                assert_eq!(cleared.status, VectorHealth::Broken);
                assert_eq!(cleared.updated_at, 30);
                assert_eq!(
                    stores.update("missing", &VectorStorePatch::default())?,
                    None
                );
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn stores_list_by_name_and_names_are_unique() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let provider = provider(connection);
                let stores = Repositories::new(connection).vector_stores;
                stores.create(&VectorStoreDraft::new("b", "Beta", &provider, "m", 3, 1))?;
                stores.create(&VectorStoreDraft::new("a", "Alpha", &provider, "m", 3, 1))?;
                assert_eq!(
                    stores
                        .list()?
                        .into_iter()
                        .map(|row| row.name)
                        .collect::<Vec<_>>(),
                    ["Alpha", "Beta"]
                );
                assert!(
                    stores
                        .create(&VectorStoreDraft::new("c", "Alpha", &provider, "m", 3, 1))
                        .is_err()
                );
                stores.remove("a")?;
                assert_eq!(stores.find_by_id("a")?, None);
                Ok(())
            })
            .expect("the repository works");
    }
}
