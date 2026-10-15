use rusqlite::{Connection, OptionalExtension, params};
use zvs_contract::Timestamp;

use crate::data::schema::VectorDocumentRow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorDocumentDraft {
    pub id: String,
    pub store_id: String,
    pub source_path: String,
    pub content_hash: String,
    pub chunk_count: i64,
    pub bytes: i64,
    pub indexed_at: Timestamp,
}

pub struct VectorDocumentRepository<'c> {
    connection: &'c Connection,
}

impl<'c> VectorDocumentRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list_by_store(&self, store_id: &str) -> rusqlite::Result<Vec<VectorDocumentRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM vector_document WHERE store_id = ?1 ORDER BY source_path",
                VectorDocumentRow::COLUMNS
            ))?
            .query_map([store_id], VectorDocumentRow::from_row)?
            .collect()
    }

    pub fn find_by_id(&self, id: &str) -> rusqlite::Result<Option<VectorDocumentRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM vector_document WHERE id = ?1",
                VectorDocumentRow::COLUMNS
            ))?
            .query_row([id], VectorDocumentRow::from_row)
            .optional()
    }

    pub fn find_by_source(
        &self,
        store_id: &str,
        source_path: &str,
    ) -> rusqlite::Result<Option<VectorDocumentRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM vector_document WHERE store_id = ?1 AND source_path = ?2",
                VectorDocumentRow::COLUMNS
            ))?
            .query_row([store_id, source_path], VectorDocumentRow::from_row)
            .optional()
    }

    pub fn record_indexed(
        &self,
        draft: &VectorDocumentDraft,
    ) -> rusqlite::Result<VectorDocumentRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO vector_document (id, store_id, source_path, content_hash, chunk_count, bytes, indexed_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
                 ON CONFLICT (store_id, source_path) DO UPDATE SET content_hash = excluded.content_hash, chunk_count = excluded.chunk_count, bytes = excluded.bytes, indexed_at = excluded.indexed_at \
                 RETURNING {}",
                VectorDocumentRow::COLUMNS
            ))?
            .query_row(
                params![
                    draft.id,
                    draft.store_id,
                    draft.source_path,
                    draft.content_hash,
                    draft.chunk_count,
                    draft.bytes,
                    draft.indexed_at,
                ],
                VectorDocumentRow::from_row,
            )
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached("DELETE FROM vector_document WHERE id = ?1")?
            .execute([id])?;
        Ok(())
    }

    pub fn remove_by_source(&self, store_id: &str, source_path: &str) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached("DELETE FROM vector_document WHERE store_id = ?1 AND source_path = ?2")?
            .execute([store_id, source_path])?;
        Ok(())
    }

    pub fn remove_by_store(&self, store_id: &str) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached("DELETE FROM vector_document WHERE store_id = ?1")?
            .execute([store_id])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::repositories::{Repositories, VectorStoreDraft};
    use crate::testing::{seed_embedding_provider, temporary_database};

    fn draft(id: &str, path: &str, hash: &str) -> VectorDocumentDraft {
        VectorDocumentDraft {
            id: id.to_owned(),
            store_id: "s1".to_owned(),
            source_path: path.to_owned(),
            content_hash: hash.to_owned(),
            chunk_count: 2,
            bytes: 12,
            indexed_at: 2,
        }
    }

    #[test]
    fn document_upsert_retains_identity_and_supports_unchanged_file_lookup() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let provider = seed_embedding_provider(connection);
                let repositories = Repositories::new(connection);
                repositories
                    .vector_stores
                    .create(&VectorStoreDraft::new("s1", "K", &provider, "m", 3, 1))?;
                let documents = repositories.vector_documents;
                documents.record_indexed(&draft("d1", "a.md", "abc"))?;
                let changed = documents.record_indexed(&VectorDocumentDraft {
                    chunk_count: 3,
                    ..draft("d2", "a.md", "def")
                })?;
                assert_eq!(changed.id, "d1");
                assert_eq!(changed.chunk_count, 3);
                assert_eq!(
                    documents
                        .find_by_source("s1", "a.md")?
                        .map(|row| row.content_hash),
                    Some("def".to_owned())
                );
                assert_eq!(documents.list_by_store("s1")?.len(), 1);
                documents.remove_by_source("s1", "a.md")?;
                assert_eq!(documents.list_by_store("s1")?, Vec::new());

                documents.record_indexed(&draft("d3", "c.md", "c"))?;
                documents.record_indexed(&draft("d4", "b.md", "b"))?;
                assert_eq!(
                    documents
                        .list_by_store("s1")?
                        .into_iter()
                        .map(|row| row.source_path)
                        .collect::<Vec<_>>(),
                    ["b.md", "c.md"]
                );
                assert_eq!(documents.remove_by_store("s1")?, 2);
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn documents_go_with_their_store_and_the_store_restricts_its_provider() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let provider = seed_embedding_provider(connection);
                let repositories = Repositories::new(connection);
                repositories
                    .vector_stores
                    .create(&VectorStoreDraft::new("s1", "K", &provider, "m", 3, 1))?;
                repositories
                    .vector_documents
                    .record_indexed(&draft("d1", "a.md", "abc"))?;
                assert!(repositories.providers.remove(&provider).is_err());
                repositories.vector_stores.remove("s1")?;
                assert_eq!(repositories.vector_documents.find_by_id("d1")?, None);
                assert!(repositories.providers.remove(&provider).is_ok());
                Ok(())
            })
            .expect("the repository works");
    }
}
