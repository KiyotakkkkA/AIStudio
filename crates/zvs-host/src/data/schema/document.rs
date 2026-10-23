use rusqlite::Row;
use zvs_contract::{DocumentKind, DocumentOfficeFormat, Timestamp};

use super::columns::EnumColumn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRow {
    pub id: String,
    pub content_hash: String,
    pub source_path: String,
    pub kind: DocumentKind,
    pub mime_type: String,
    pub office_format: Option<DocumentOfficeFormat>,
    pub page_count: Option<i64>,
    pub part_count: i64,
    pub size_bytes: i64,
    pub cached_file: String,
    pub opened_at: Timestamp,
}

impl DocumentRow {
    pub const COLUMNS: &str = "id, content_hash, source_path, kind, mime_type, office_format, \
        page_count, part_count, size_bytes, cached_file, opened_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            content_hash: row.get("content_hash")?,
            source_path: row.get("source_path")?,
            kind: row.get::<_, EnumColumn<DocumentKind>>("kind")?.0,
            mime_type: row.get("mime_type")?,
            office_format: row
                .get::<_, Option<EnumColumn<DocumentOfficeFormat>>>("office_format")?
                .map(|column| column.0),
            page_count: row.get("page_count")?,
            part_count: row.get("part_count")?,
            size_bytes: row.get("size_bytes")?,
            cached_file: row.get("cached_file")?,
            opened_at: row.get("opened_at")?,
        })
    }
}
