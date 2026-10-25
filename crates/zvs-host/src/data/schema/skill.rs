use rusqlite::Row;
use zvs_contract::{SkillIssueDto, SkillModelDto, SkillSource, SkillStatus, Timestamp};

use super::columns::{EnumColumn, json_column, optional_json_column};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRow {
    pub id: String,
    pub key: String,
    pub name: String,
    pub description: String,
    pub allowed_tools: Vec<String>,
    pub model_override: Option<SkillModelDto>,
    pub enabled: bool,
    pub source: SkillSource,
    pub status: SkillStatus,
    pub issues: Vec<SkillIssueDto>,
    pub file_count: i64,
    pub content_hash: String,
    pub path: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl SkillRow {
    pub const COLUMNS: &str = "id, key, name, description, allowed_tools, model_override, enabled, \
        source, status, issues, file_count, content_hash, path, version, author, created_at, \
        updated_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            key: row.get("key")?,
            name: row.get("name")?,
            description: row.get("description")?,
            allowed_tools: json_column(row, "allowed_tools")?,
            model_override: optional_json_column(row, "model_override")?,
            enabled: row.get("enabled")?,
            source: row.get::<_, EnumColumn<SkillSource>>("source")?.0,
            status: row.get::<_, EnumColumn<SkillStatus>>("status")?.0,
            issues: json_column(row, "issues")?,
            file_count: row.get("file_count")?,
            content_hash: row.get("content_hash")?,
            path: row.get("path")?,
            version: row.get("version")?,
            author: row.get("author")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}
