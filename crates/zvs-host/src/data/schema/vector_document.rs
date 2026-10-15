use rusqlite::Row;
use zvs_contract::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorDocumentRow {
    pub id: String,
    pub store_id: String,
    pub source_path: String,
    pub content_hash: String,
    pub chunk_count: i64,
    pub bytes: i64,
    pub indexed_at: Timestamp,
}

impl VectorDocumentRow {
    pub const COLUMNS: &str =
        "id, store_id, source_path, content_hash, chunk_count, bytes, indexed_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            store_id: row.get("store_id")?,
            source_path: row.get("source_path")?,
            content_hash: row.get("content_hash")?,
            chunk_count: row.get("chunk_count")?,
            bytes: row.get("bytes")?,
            indexed_at: row.get("indexed_at")?,
        })
    }
}
