use rusqlite::Row;
use zvs_contract::{Timestamp, VectorSourceKind};

use super::columns::{EnumColumn, json_column};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorSourceRow {
    pub id: String,
    pub store_id: String,
    pub kind: VectorSourceKind,
    pub path: String,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub recursive: bool,
    pub created_at: Timestamp,
}

impl VectorSourceRow {
    pub const COLUMNS: &str = "id, store_id, kind, path, include, exclude, recursive, created_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            store_id: row.get("store_id")?,
            kind: row.get::<_, EnumColumn<VectorSourceKind>>("kind")?.0,
            path: row.get("path")?,
            include: json_column(row, "include")?,
            exclude: json_column(row, "exclude")?,
            recursive: row.get("recursive")?,
            created_at: row.get("created_at")?,
        })
    }
}
