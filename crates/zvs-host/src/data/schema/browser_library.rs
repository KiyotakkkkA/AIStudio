use rusqlite::Row;
use zvs_contract::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserHistoryRow {
    pub id: String,
    pub url: String,
    pub title: String,
    pub visited_at: Timestamp,
    pub imported_from: Option<String>,
}

impl BrowserHistoryRow {
    pub const COLUMNS: &str = "id, url, title, visited_at, imported_from";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            url: row.get("url")?,
            title: row.get("title")?,
            visited_at: row.get("visited_at")?,
            imported_from: row.get("imported_from")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserBookmarkRow {
    pub id: String,
    pub url: String,
    pub title: String,
    pub folder: String,
    pub created_at: Timestamp,
}

impl BrowserBookmarkRow {
    pub const COLUMNS: &str = "id, url, title, folder, created_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            url: row.get("url")?,
            title: row.get("title")?,
            folder: row.get("folder")?,
            created_at: row.get("created_at")?,
        })
    }
}
