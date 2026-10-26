use rusqlite::Row;
use serde::{Deserialize, Serialize};
use zvs_contract::{
    AgenticBudgetDto, AgenticChangedFileDto, AgenticPermissionMode, AgenticPlanEditDto,
    AgenticPlanStepDto, AgenticSessionStatus, AgenticStopReason, AgenticUsageDto, Timestamp,
};

use super::columns::{EnumColumn, json_column, optional_json_column};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEntry {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<TranscriptCall>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgenticSessionRow {
    pub id: String,
    pub goal: String,
    pub status: AgenticSessionStatus,
    pub stop_reason: Option<AgenticStopReason>,
    pub error: Option<String>,
    pub summary: Option<String>,
    pub mode: AgenticPermissionMode,
    pub pending_mode: Option<AgenticPermissionMode>,
    pub workspace: String,
    pub provider_id: String,
    pub model_id: String,
    pub tools: Vec<String>,
    pub budget: AgenticBudgetDto,
    pub usage: AgenticUsageDto,
    pub plan: Vec<AgenticPlanStepDto>,
    pub pending_plan: Option<Vec<AgenticPlanEditDto>>,
    pub pending_instructions: Vec<String>,
    pub changed_files: Vec<AgenticChangedFileDto>,
    pub run_id: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl AgenticSessionRow {
    pub const COLUMNS: &str = "id, goal, status, stop_reason, error, summary, mode, pending_mode, \
                               workspace, provider_id, model_id, tools, budget, usage, plan, \
                               pending_plan, pending_instructions, changed_files, \
                               run_id, created_at, updated_at";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            goal: row.get("goal")?,
            status: row.get::<_, EnumColumn<AgenticSessionStatus>>("status")?.0,
            stop_reason: row
                .get::<_, Option<EnumColumn<AgenticStopReason>>>("stop_reason")?
                .map(|column| column.0),
            error: row.get("error")?,
            summary: row.get("summary")?,
            mode: row.get::<_, EnumColumn<AgenticPermissionMode>>("mode")?.0,
            pending_mode: row
                .get::<_, Option<EnumColumn<AgenticPermissionMode>>>("pending_mode")?
                .map(|column| column.0),
            workspace: row.get("workspace")?,
            provider_id: row.get("provider_id")?,
            model_id: row.get("model_id")?,
            tools: json_column(row, "tools")?,
            budget: json_column(row, "budget")?,
            usage: json_column(row, "usage")?,
            plan: json_column(row, "plan")?,
            pending_plan: optional_json_column(row, "pending_plan")?,
            pending_instructions: json_column(row, "pending_instructions")?,
            changed_files: json_column(row, "changed_files")?,
            run_id: row.get("run_id")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}
