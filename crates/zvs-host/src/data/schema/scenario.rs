use rusqlite::Row;
use zvs_contract::{ScenarioDiffDto, ScenarioOverlap, ScenarioTriggerDto, Timestamp};

use super::columns::{EnumColumn, json_column, optional_json_column};

#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioRow {
    pub id: String,
    pub name: String,
    pub tags: Vec<String>,
    pub active: bool,
    pub overlap: ScenarioOverlap,
    pub current_version_id: Option<String>,
    pub trigger: Option<ScenarioTriggerDto>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl ScenarioRow {
    pub const COLUMNS: &str =
        "id, name, tags, active, overlap, current_version_id, trigger, created_at, updated_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            tags: json_column(row, "tags")?,
            active: row.get("active")?,
            overlap: row.get::<_, EnumColumn<ScenarioOverlap>>("overlap")?.0,
            current_version_id: row.get("current_version_id")?,
            trigger: optional_json_column(row, "trigger")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioVersionRow {
    pub id: String,
    pub scenario_id: String,
    pub version: i64,
    pub note: Option<String>,
    pub summary: ScenarioDiffDto,
    pub created_at: Timestamp,
}

impl ScenarioVersionRow {
    pub const COLUMNS: &str = "id, scenario_id, version, note, summary, created_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            scenario_id: row.get("scenario_id")?,
            version: row.get("version")?,
            note: row.get("note")?,
            summary: json_column(row, "summary")?,
            created_at: row.get("created_at")?,
        })
    }
}
