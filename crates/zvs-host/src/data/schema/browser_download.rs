use rusqlite::Row;
use zvs_contract::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserDownloadRow {
    pub id: String,
    pub url: String,
    pub file_name: String,
    pub path: Option<String>,
    pub bytes: i64,
    pub state: String,
    pub agent: bool,
    pub finished_at: Timestamp,
}

impl BrowserDownloadRow {
    pub const COLUMNS: &str = "id, url, file_name, path, bytes, state, agent, finished_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            url: row.get("url")?,
            file_name: row.get("file_name")?,
            path: row.get("path")?,
            bytes: row.get("bytes")?,
            state: row.get("state")?,
            agent: row.get("agent")?,
            finished_at: row.get("finished_at")?,
        })
    }
}
