use rusqlite::{Connection, OptionalExtension, params};
use zvs_contract::Timestamp;

use crate::data::schema::VectorEvalCaseRow;
use crate::data::schema::columns::json_text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorEvalCaseDraft {
    pub id: String,
    pub store_id: String,
    pub question: String,
    pub expected_document_ids: Vec<String>,
    pub created_at: Timestamp,
}

pub struct VectorEvalCaseRepository<'c> {
    connection: &'c Connection,
}

impl<'c> VectorEvalCaseRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list_by_store(&self, store_id: &str) -> rusqlite::Result<Vec<VectorEvalCaseRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM vector_eval_case WHERE store_id = ?1 ORDER BY created_at, id",
                VectorEvalCaseRow::COLUMNS
            ))?
            .query_map([store_id], VectorEvalCaseRow::from_row)?
            .collect()
    }

    pub fn find_in_store(
        &self,
        store_id: &str,
        id: &str,
    ) -> rusqlite::Result<Option<VectorEvalCaseRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM vector_eval_case WHERE store_id = ?1 AND id = ?2",
                VectorEvalCaseRow::COLUMNS
            ))?
            .query_row([store_id, id], VectorEvalCaseRow::from_row)
            .optional()
    }

    pub fn add(&self, draft: &VectorEvalCaseDraft) -> rusqlite::Result<VectorEvalCaseRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO vector_eval_case (id, store_id, question, expected_document_ids, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5) RETURNING {}",
                VectorEvalCaseRow::COLUMNS
            ))?
            .query_row(
                params![
                    draft.id,
                    draft.store_id,
                    draft.question,
                    json_text(&draft.expected_document_ids)?,
                    draft.created_at,
                ],
                VectorEvalCaseRow::from_row,
            )
    }

    pub fn remove(&self, store_id: &str, id: &str) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached("DELETE FROM vector_eval_case WHERE store_id = ?1 AND id = ?2")?
            .execute([store_id, id])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::repositories::{Repositories, VectorStoreDraft};
    use crate::testing::{seed_embedding_provider, temporary_database};

    #[test]
    fn cases_list_in_creation_order_and_are_removed_per_store() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let provider = seed_embedding_provider(connection);
                let repositories = Repositories::new(connection);
                repositories
                    .vector_stores
                    .create(&VectorStoreDraft::new("s1", "K", &provider, "m", 3, 1))?;
                let cases = repositories.vector_eval_cases;
                for (id, at) in [("c2", 2), ("c1", 1), ("c3", 2)] {
                    cases.add(&VectorEvalCaseDraft {
                        id: id.to_owned(),
                        store_id: "s1".to_owned(),
                        question: format!("{id}?"),
                        expected_document_ids: vec!["d1".to_owned()],
                        created_at: at,
                    })?;
                }
                assert_eq!(
                    cases
                        .list_by_store("s1")?
                        .into_iter()
                        .map(|row| row.id)
                        .collect::<Vec<_>>(),
                    ["c1", "c2", "c3"]
                );
                assert_eq!(
                    cases
                        .find_in_store("s1", "c1")?
                        .map(|row| row.expected_document_ids),
                    Some(vec!["d1".to_owned()])
                );
                assert_eq!(cases.remove("other", "c1")?, 0);
                assert_eq!(cases.remove("s1", "c1")?, 1);
                repositories.vector_stores.remove("s1")?;
                assert_eq!(cases.list_by_store("s1")?, Vec::new());
                Ok(())
            })
            .expect("the repository works");
    }
}
