use rusqlite::Row;
use zvs_contract::Timestamp;

use super::columns::json_column;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorEvalCaseRow {
    pub id: String,
    pub store_id: String,
    pub question: String,
    pub expected_document_ids: Vec<String>,
    pub created_at: Timestamp,
}

impl VectorEvalCaseRow {
    pub const COLUMNS: &str = "id, store_id, question, expected_document_ids, created_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            store_id: row.get("store_id")?,
            question: row.get("question")?,
            expected_document_ids: json_column(row, "expected_document_ids")?,
            created_at: row.get("created_at")?,
        })
    }
}
