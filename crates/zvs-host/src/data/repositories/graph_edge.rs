use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use zvs_contract::Timestamp;

use crate::data::schema::GraphEdgeRow;
use crate::data::schema::columns::json_text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphEdgeLookup<'a> {
    pub store_id: &'a str,
    pub from_document_id: &'a str,
    pub to_document_id: &'a str,
    pub relation_type_id: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdgeDraft {
    pub id: String,
    pub store_id: String,
    pub from_document_id: String,
    pub to_document_id: String,
    pub relation_type_id: String,
    pub weight: Option<f64>,
    pub metadata: Value,
    pub created_at: Timestamp,
}

pub struct GraphEdgeRepository<'c> {
    connection: &'c Connection,
}

impl<'c> GraphEdgeRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list_by_store(&self, store_id: &str) -> rusqlite::Result<Vec<GraphEdgeRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM graph_edge WHERE store_id = ?1 ORDER BY created_at, id",
                GraphEdgeRow::COLUMNS
            ))?
            .query_map([store_id], GraphEdgeRow::from_row)?
            .collect()
    }

    pub fn list_by_document(
        &self,
        store_id: &str,
        document_id: &str,
    ) -> rusqlite::Result<Vec<GraphEdgeRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM graph_edge WHERE store_id = ?1 AND (from_document_id = ?2 OR to_document_id = ?2) ORDER BY created_at, id",
                GraphEdgeRow::COLUMNS
            ))?
            .query_map([store_id, document_id], GraphEdgeRow::from_row)?
            .collect()
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<GraphEdgeRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM graph_edge WHERE id = ?1",
                GraphEdgeRow::COLUMNS
            ))?
            .query_row([id], GraphEdgeRow::from_row)
            .optional()
    }

    pub fn find_by_key(&self, key: GraphEdgeLookup<'_>) -> rusqlite::Result<Option<GraphEdgeRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM graph_edge WHERE store_id = ?1 AND from_document_id = ?2 AND to_document_id = ?3 AND relation_type_id = ?4",
                GraphEdgeRow::COLUMNS
            ))?
            .query_row(
                [
                    key.store_id,
                    key.from_document_id,
                    key.to_document_id,
                    key.relation_type_id,
                ],
                GraphEdgeRow::from_row,
            )
            .optional()
    }

    pub fn count_by_store(&self, store_id: &str) -> rusqlite::Result<u64> {
        self.connection
            .prepare_cached("SELECT count(*) FROM graph_edge WHERE store_id = ?1")?
            .query_row([store_id], |row| row.get::<_, i64>(0))
            .map(|count| u64::try_from(count).unwrap_or(0))
    }

    pub fn link(&self, draft: &GraphEdgeDraft) -> rusqlite::Result<GraphEdgeRow> {
        let metadata = if draft.metadata.is_null() {
            None
        } else {
            Some(json_text(&draft.metadata)?)
        };
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO graph_edge (id, store_id, from_document_id, to_document_id, relation_type_id, weight, metadata, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) RETURNING {}",
                GraphEdgeRow::COLUMNS
            ))?
            .query_row(
                params![
                    draft.id,
                    draft.store_id,
                    draft.from_document_id,
                    draft.to_document_id,
                    draft.relation_type_id,
                    draft.weight,
                    metadata,
                    draft.created_at,
                ],
                GraphEdgeRow::from_row,
            )
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached("DELETE FROM graph_edge WHERE id = ?1")?
            .execute([id])
    }

    pub fn remove_by_store(&self, store_id: &str) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached("DELETE FROM graph_edge WHERE store_id = ?1")?
            .execute([store_id])
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::data::repositories::{Repositories, VectorDocumentDraft, VectorStoreDraft};
    use crate::testing::{seed_embedding_provider, temporary_database};

    const CITES: &str = "01999a00-0001-7000-8000-000000000001";

    fn document(id: &str) -> VectorDocumentDraft {
        VectorDocumentDraft {
            id: id.to_owned(),
            store_id: "s1".to_owned(),
            source_path: format!("{id}.md"),
            content_hash: id.to_owned(),
            chunk_count: 1,
            bytes: 1,
            indexed_at: 1,
        }
    }

    fn edge(id: &str, from: &str, to: &str, at: Timestamp) -> GraphEdgeDraft {
        GraphEdgeDraft {
            id: id.to_owned(),
            store_id: "s1".to_owned(),
            from_document_id: from.to_owned(),
            to_document_id: to.to_owned(),
            relation_type_id: CITES.to_owned(),
            weight: None,
            metadata: Value::Null,
            created_at: at,
        }
    }

    #[test]
    fn edges_are_keyed_unique_listed_in_order_and_cascade_with_documents() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let provider = seed_embedding_provider(connection);
                let repositories = Repositories::new(connection);
                repositories
                    .vector_stores
                    .create(&VectorStoreDraft::new("s1", "K", &provider, "m", 3, 1))?;
                for id in ["a", "b", "c"] {
                    repositories
                        .vector_documents
                        .record_indexed(&document(id))?;
                }
                let edges = repositories.graph_edges;
                let first = edges.link(&GraphEdgeDraft {
                    weight: Some(0.25),
                    metadata: json!({ "note": "вручную" }),
                    ..edge("e2", "a", "b", 2)
                })?;
                assert_eq!(first.weight, Some(0.25));
                assert_eq!(first.metadata(), json!({ "note": "вручную" }));
                let plain = edges.link(&edge("e1", "b", "c", 1))?;
                assert_eq!(plain.metadata, None);
                assert!(edges.link(&edge("e3", "a", "b", 3)).is_err());
                assert_eq!(
                    edges
                        .list_by_store("s1")?
                        .into_iter()
                        .map(|row| row.id)
                        .collect::<Vec<_>>(),
                    ["e1", "e2"]
                );
                assert_eq!(edges.list_by_document("s1", "a")?.len(), 1);
                assert_eq!(edges.count_by_store("s1")?, 2);
                assert_eq!(
                    edges
                        .find_by_key(GraphEdgeLookup {
                            store_id: "s1",
                            from_document_id: "a",
                            to_document_id: "b",
                            relation_type_id: CITES,
                        })?
                        .map(|row| row.id),
                    Some("e2".to_owned())
                );
                repositories.vector_documents.remove("c")?;
                assert_eq!(edges.find_by_id("e1")?, None);
                assert_eq!(edges.remove("e2")?, 1);
                assert_eq!(edges.remove_by_store("s1")?, 0);
                Ok(())
            })
            .expect("the repository works");
    }
}
