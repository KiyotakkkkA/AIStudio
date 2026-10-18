use rusqlite::Row;
use rusqlite::types::Type;
use serde_json::Value;
use zvs_contract::{Timestamp, ToolReferenceKind, ToolSource, ToolTier};

use super::columns::EnumColumn;

#[derive(Debug, Clone, PartialEq)]
pub struct ToolRow {
    pub id: String,
    pub key: String,
    pub source: ToolSource,
    pub source_ref: Option<String>,
    pub display_name: String,
    pub description: String,
    pub input_schema: Value,
    pub read_only: bool,
    pub destructive: bool,
    pub tier: ToolTier,
    pub enabled: bool,
    pub available: bool,
    pub last_used_at: Option<Timestamp>,
    pub call_count: u64,
    pub registered_at: Timestamp,
    pub updated_at: Timestamp,
}

pub fn nullable_json(row: &Row<'_>, name: &str) -> rusqlite::Result<Value> {
    let Some(text) = row.get::<_, Option<String>>(name)? else {
        return Ok(Value::Null);
    };
    let index = row.as_ref().column_index(name)?;
    serde_json::from_str(&text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

pub fn nullable_json_text(value: &Value) -> rusqlite::Result<Option<String>> {
    if value.is_null() {
        return Ok(None);
    }
    serde_json::to_string(value)
        .map(Some)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

impl ToolRow {
    pub const COLUMNS: &str = "id, key, source, source_ref, display_name, description, \
        input_schema, read_only, destructive, tier, enabled, available, last_used_at, \
        call_count, registered_at, updated_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            key: row.get("key")?,
            source: row.get::<_, EnumColumn<ToolSource>>("source")?.0,
            source_ref: row.get("source_ref")?,
            display_name: row.get("display_name")?,
            description: row.get("description")?,
            input_schema: nullable_json(row, "input_schema")?,
            read_only: row.get("read_only")?,
            destructive: row.get("destructive")?,
            tier: row.get::<_, EnumColumn<ToolTier>>("tier")?.0,
            enabled: row.get("enabled")?,
            available: row.get("available")?,
            last_used_at: row.get("last_used_at")?,
            call_count: u64::try_from(row.get::<_, i64>("call_count")?).unwrap_or(0),
            registered_at: row.get("registered_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolReferenceRow {
    pub id: String,
    pub tool_id: String,
    pub kind: ToolReferenceKind,
    pub ref_id: String,
    pub name: String,
}

impl ToolReferenceRow {
    pub const COLUMNS: &str = "id, tool_id, kind, ref_id, name";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            tool_id: row.get("tool_id")?,
            kind: row.get::<_, EnumColumn<ToolReferenceKind>>("kind")?.0,
            ref_id: row.get("ref_id")?,
            name: row.get("name")?,
        })
    }
}
