use rusqlite::Row;
use zvs_contract::{ChecksumAlgorithm, DownloadItemKind, DownloadStatus, Timestamp};

use super::columns::EnumColumn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRow {
    pub id: String,
    pub item_kind: DownloadItemKind,
    pub item_ref: String,
    pub display_name: String,
    pub version: Option<String>,
    pub size_bytes: u64,
    pub bytes_done: u64,
    pub status: DownloadStatus,
    pub priority: u64,
    pub target_path: String,
    pub url: String,
    pub checksum_algorithm: Option<ChecksumAlgorithm>,
    pub checksum_value: Option<String>,
    pub error: Option<String>,
    pub run_id: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
}

fn unsigned(row: &Row<'_>, name: &str) -> rusqlite::Result<u64> {
    Ok(u64::try_from(row.get::<_, i64>(name)?).unwrap_or(0))
}

impl DownloadRow {
    pub const COLUMNS: &str = "id, item_kind, item_ref, display_name, version, size_bytes, \
        bytes_done, status, priority, target_path, url, checksum_algorithm, checksum_value, \
        error, run_id, created_at, updated_at, started_at, finished_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            item_kind: row.get::<_, EnumColumn<DownloadItemKind>>("item_kind")?.0,
            item_ref: row.get("item_ref")?,
            display_name: row.get("display_name")?,
            version: row.get("version")?,
            size_bytes: unsigned(row, "size_bytes")?,
            bytes_done: unsigned(row, "bytes_done")?,
            status: row.get::<_, EnumColumn<DownloadStatus>>("status")?.0,
            priority: unsigned(row, "priority")?,
            target_path: row.get("target_path")?,
            url: row.get("url")?,
            checksum_algorithm: row
                .get::<_, Option<EnumColumn<ChecksumAlgorithm>>>("checksum_algorithm")?
                .map(|column| column.0),
            checksum_value: row.get("checksum_value")?,
            error: row.get("error")?,
            run_id: row.get("run_id")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            started_at: row.get("started_at")?,
            finished_at: row.get("finished_at")?,
        })
    }
}
