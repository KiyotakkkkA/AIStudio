use rusqlite::Row;
use serde_json::Value;
use zvs_contract::{RunGraph, RunKind, RunOutcomeDto, RunStatus, StepStatus, Timestamp};

use super::columns::{EnumColumn, json_column, optional_json_column};

#[derive(Debug, Clone, PartialEq)]
pub struct RunRow {
    pub id: String,
    pub kind: RunKind,
    pub subject_id: Option<String>,
    pub title: Option<String>,
    pub retry_of_id: Option<String>,
    pub status: RunStatus,
    pub graph: RunGraph,
    pub input: Value,
    pub outcome: Option<RunOutcomeDto>,
    pub stream_id: String,
    pub concurrency: i64,
    pub created_at: Timestamp,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
    pub pruned_at: Option<Timestamp>,
    pub error: Option<String>,
}

impl RunRow {
    pub const COLUMNS: &str = "id, kind, subject_id, title, retry_of_id, status, graph, input, \
         outcome, stream_id, concurrency, created_at, started_at, finished_at, pruned_at, error";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            kind: row.get::<_, EnumColumn<RunKind>>("kind")?.0,
            subject_id: row.get("subject_id")?,
            title: row.get("title")?,
            retry_of_id: row.get("retry_of_id")?,
            status: row.get::<_, EnumColumn<RunStatus>>("status")?.0,
            graph: json_column(row, "graph")?,
            input: optional_json_column(row, "input")?.unwrap_or(Value::Null),
            outcome: optional_json_column(row, "outcome")?,
            stream_id: row.get("stream_id")?,
            concurrency: row.get("concurrency")?,
            created_at: row.get("created_at")?,
            started_at: row.get("started_at")?,
            finished_at: row.get("finished_at")?,
            pruned_at: row.get("pruned_at")?,
            error: row.get("error")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StepRow {
    pub id: String,
    pub run_id: String,
    pub node_id: String,
    pub step_type: String,
    pub status: StepStatus,
    pub input: Value,
    pub output: Value,
    pub error: Option<String>,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
    pub attempt: i64,
}

impl StepRow {
    pub const COLUMNS: &str =
        "id, run_id, node_id, type, status, input, output, error, started_at, finished_at, attempt";

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            run_id: row.get("run_id")?,
            node_id: row.get("node_id")?,
            step_type: row.get("type")?,
            status: row.get::<_, EnumColumn<StepStatus>>("status")?.0,
            input: optional_json_column(row, "input")?.unwrap_or(Value::Null),
            output: optional_json_column(row, "output")?.unwrap_or(Value::Null),
            error: row.get("error")?,
            started_at: row.get("started_at")?,
            finished_at: row.get("finished_at")?,
            attempt: row.get("attempt")?,
        })
    }
}
