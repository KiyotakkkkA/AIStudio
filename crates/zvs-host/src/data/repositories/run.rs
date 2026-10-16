use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde_json::Value;
use zvs_contract::{
    EventSeq, HostEvent, HostEventDraft, LogLineDto, RunGraph, RunKind, RunOutcomeDto, RunStatus,
    StepStatus, Timestamp,
};

use crate::data::schema::columns::{EnumColumn, json_text};
use crate::data::schema::{RunRow, StepRow};

#[derive(Debug, Clone, PartialEq)]
pub struct RunDraft {
    pub id: String,
    pub kind: RunKind,
    pub subject_id: Option<String>,
    pub title: Option<String>,
    pub retry_of_id: Option<String>,
    pub status: RunStatus,
    pub graph: RunGraph,
    pub input: Value,
    pub stream_id: String,
    pub concurrency: i64,
    pub created_at: Timestamp,
    pub started_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunPatch {
    pub status: Option<RunStatus>,
    pub outcome: Option<RunOutcomeDto>,
    pub error: Option<Option<String>>,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunPageQuery {
    pub statuses: Vec<RunStatus>,
    pub kinds: Vec<RunKind>,
    pub query: Option<String>,
    pub from: Option<Timestamp>,
    pub to: Option<Timestamp>,
    pub live: bool,
    pub attention_since: Option<Timestamp>,
    pub limit: i64,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunCursor<'a> {
    pub created_at: Timestamp,
    pub id: &'a str,
}

pub fn encode_run_cursor(cursor: RunCursor<'_>) -> String {
    format!("{}:{}", cursor.created_at, cursor.id)
}

pub fn decode_run_cursor(value: &str) -> Option<RunCursor<'_>> {
    let separator = value.find(':')?;
    if separator == 0 {
        return None;
    }
    let (created, rest) = value.split_at(separator);
    let id = &rest[1..];
    let created_at = parse_js_integer(created)?;
    if id.is_empty() {
        return None;
    }
    Some(RunCursor { created_at, id })
}

fn parse_js_integer(text: &str) -> Option<Timestamp> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Some(0);
    }
    let number: f64 = trimmed.parse().ok()?;
    if number.is_finite() && number.fract() == 0.0 && number.abs() <= 9_007_199_254_740_991.0 {
        #[allow(clippy::cast_possible_truncation)]
        Some(number as Timestamp)
    } else {
        None
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunCounts {
    pub by_status: Vec<(RunStatus, i64)>,
    pub by_kind: Vec<(RunKind, i64)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningNode {
    pub run_id: String,
    pub node_id: String,
    pub started_at: Timestamp,
}

fn nullable_json(value: &Value) -> rusqlite::Result<Option<String>> {
    if value.is_null() {
        Ok(None)
    } else {
        json_text(value).map(Some)
    }
}

fn placeholders(count: usize) -> String {
    vec!["?"; count].join(", ")
}

fn status_values(statuses: &[RunStatus]) -> impl Iterator<Item = SqlValue> + '_ {
    statuses
        .iter()
        .map(|status| SqlValue::Text(status.as_str().to_owned()))
}

pub struct RunRepository<'c> {
    connection: &'c Connection,
}

impl<'c> RunRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn create(&self, draft: &RunDraft) -> rusqlite::Result<RunRow> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO run (id, kind, subject_id, title, retry_of_id, status, graph, input, \
                 stream_id, concurrency, created_at, started_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) RETURNING {}",
                RunRow::COLUMNS
            ))?
            .query_row(
                params![
                    draft.id,
                    EnumColumn(draft.kind),
                    draft.subject_id,
                    draft.title,
                    draft.retry_of_id,
                    EnumColumn(draft.status),
                    json_text(&draft.graph)?,
                    nullable_json(&draft.input)?,
                    draft.stream_id,
                    draft.concurrency,
                    draft.created_at,
                    draft.started_at,
                ],
                RunRow::from_row,
            )
    }

    pub fn get(&self, id: &str) -> rusqlite::Result<Option<RunRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM run WHERE id = ?1",
                RunRow::COLUMNS
            ))?
            .query_row([id], RunRow::from_row)
            .optional()
    }

    pub fn list(&self) -> rusqlite::Result<Vec<RunRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM run ORDER BY created_at DESC, id DESC",
                RunRow::COLUMNS
            ))?
            .query_map([], RunRow::from_row)?
            .collect()
    }

    pub fn page(&self, query: &RunPageQuery) -> rusqlite::Result<Vec<RunRow>> {
        let mut conditions: Vec<String> = Vec::new();
        let mut values: Vec<SqlValue> = Vec::new();
        if !query.statuses.is_empty() {
            conditions.push(format!(
                "status IN ({})",
                placeholders(query.statuses.len())
            ));
            values.extend(status_values(&query.statuses));
        }
        if !query.kinds.is_empty() {
            conditions.push(format!("kind IN ({})", placeholders(query.kinds.len())));
            values.extend(
                query
                    .kinds
                    .iter()
                    .map(|kind| SqlValue::Text(kind.as_str().to_owned())),
            );
        }
        if query.live {
            conditions.push(format!(
                "(status IN ({}) OR (status IN ({}) AND finished_at >= ?))",
                placeholders(RunStatus::UNFINISHED.len()),
                placeholders(RunStatus::ATTENTION.len())
            ));
            values.extend(status_values(RunStatus::UNFINISHED));
            values.extend(status_values(RunStatus::ATTENTION));
            values.push(SqlValue::Integer(query.attention_since.unwrap_or(0)));
        }
        if let Some(from) = query.from {
            conditions.push("created_at >= ?".to_owned());
            values.push(SqlValue::Integer(from));
        }
        if let Some(to) = query.to {
            conditions.push("created_at <= ?".to_owned());
            values.push(SqlValue::Integer(to));
        }
        if let Some(text) = query.query.as_deref().filter(|text| !text.is_empty()) {
            let search = format!("%{text}%");
            conditions.push(
                "(title LIKE ? OR id LIKE ? OR subject_id LIKE ? OR EXISTS (SELECT 1 FROM step \
                 WHERE step.run_id = run.id AND (step.node_id LIKE ? OR step.type LIKE ?)))"
                    .to_owned(),
            );
            values.extend((0..5).map(|_| SqlValue::Text(search.clone())));
        }
        if let Some(cursor) = query.cursor.as_deref().and_then(decode_run_cursor) {
            conditions.push("(created_at < ? OR (created_at = ? AND id < ?))".to_owned());
            values.push(SqlValue::Integer(cursor.created_at));
            values.push(SqlValue::Integer(cursor.created_at));
            values.push(SqlValue::Text(cursor.id.to_owned()));
        }
        let filter = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };
        values.push(SqlValue::Integer(query.limit));
        self.connection
            .prepare(&format!(
                "SELECT {} FROM run{filter} ORDER BY created_at DESC, id DESC LIMIT ?",
                RunRow::COLUMNS
            ))?
            .query_map(params_from_iter(values), RunRow::from_row)?
            .collect()
    }

    pub fn counts(&self) -> rusqlite::Result<RunCounts> {
        let by_status = self
            .connection
            .prepare_cached("SELECT status, count(*) FROM run GROUP BY status")?
            .query_map([], |row| {
                Ok((
                    row.get::<_, EnumColumn<RunStatus>>(0)?.0,
                    row.get::<_, i64>(1)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let by_kind = self
            .connection
            .prepare_cached("SELECT kind, count(*) FROM run GROUP BY kind")?
            .query_map([], |row| {
                Ok((
                    row.get::<_, EnumColumn<RunKind>>(0)?.0,
                    row.get::<_, i64>(1)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(RunCounts { by_status, by_kind })
    }

    pub fn unfinished(&self) -> rusqlite::Result<Vec<RunRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM run WHERE status IN ({})",
                RunRow::COLUMNS,
                placeholders(RunStatus::UNFINISHED.len())
            ))?
            .query_map(
                params_from_iter(status_values(RunStatus::UNFINISHED)),
                RunRow::from_row,
            )?
            .collect()
    }

    pub fn update(&self, id: &str, patch: &RunPatch) -> rusqlite::Result<usize> {
        let mut assignments: Vec<&str> = Vec::new();
        let mut values: Vec<SqlValue> = Vec::new();
        if let Some(status) = patch.status {
            assignments.push("status = ?");
            values.push(SqlValue::Text(status.as_str().to_owned()));
        }
        if let Some(outcome) = &patch.outcome {
            assignments.push("outcome = ?");
            values.push(SqlValue::Text(json_text(outcome)?));
        }
        if let Some(error) = &patch.error {
            assignments.push("error = ?");
            values.push(error.clone().map_or(SqlValue::Null, SqlValue::Text));
        }
        if let Some(started_at) = patch.started_at {
            assignments.push("started_at = ?");
            values.push(SqlValue::Integer(started_at));
        }
        if let Some(finished_at) = patch.finished_at {
            assignments.push("finished_at = ?");
            values.push(SqlValue::Integer(finished_at));
        }
        if assignments.is_empty() {
            return Ok(0);
        }
        values.push(SqlValue::Text(id.to_owned()));
        self.connection
            .prepare_cached(&format!(
                "UPDATE run SET {} WHERE id = ?",
                assignments.join(", ")
            ))?
            .execute(params_from_iter(values))
    }

    pub fn steps(&self, run_id: &str) -> rusqlite::Result<Vec<StepRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM step WHERE run_id = ?1 ORDER BY id",
                StepRow::COLUMNS
            ))?
            .query_map([run_id], StepRow::from_row)?
            .collect()
    }

    pub fn succeeded_counts(&self, ids: &[String]) -> rusqlite::Result<Vec<(String, i64)>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.connection
            .prepare(&format!(
                "SELECT run_id, count(*) FROM step WHERE run_id IN ({}) AND status = 'succeeded' \
                 GROUP BY run_id",
                placeholders(ids.len())
            ))?
            .query_map(params_from_iter(ids), |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect()
    }

    pub fn running_nodes(&self, ids: &[String]) -> rusqlite::Result<Vec<RunningNode>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.connection
            .prepare(&format!(
                "SELECT run_id, node_id, started_at FROM step WHERE run_id IN ({}) \
                 AND status = 'running' ORDER BY started_at",
                placeholders(ids.len())
            ))?
            .query_map(params_from_iter(ids), |row| {
                Ok(RunningNode {
                    run_id: row.get(0)?,
                    node_id: row.get(1)?,
                    started_at: row.get(2)?,
                })
            })?
            .collect()
    }

    pub fn add_step(&self, step: &StepRow) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached(
                "INSERT INTO step (id, run_id, node_id, type, status, input, output, error, \
                 started_at, finished_at, attempt) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?
            .execute(params![
                step.id,
                step.run_id,
                step.node_id,
                step.step_type,
                EnumColumn(step.status),
                nullable_json(&step.input)?,
                nullable_json(&step.output)?,
                step.error,
                step.started_at,
                step.finished_at,
                step.attempt,
            ])?;
        Ok(())
    }

    pub fn update_step(&self, step: &StepRow) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached(
                "UPDATE step SET status = ?2, input = ?3, output = ?4, error = ?5, \
                 finished_at = ?6 WHERE id = ?1",
            )?
            .execute(params![
                step.id,
                EnumColumn(step.status),
                nullable_json(&step.input)?,
                nullable_json(&step.output)?,
                step.error,
                step.finished_at,
            ])
    }

    pub fn finish_step(
        &self,
        id: &str,
        status: StepStatus,
        error: &str,
        finished_at: Timestamp,
    ) -> rusqlite::Result<usize> {
        self.connection
            .prepare_cached(
                "UPDATE step SET status = ?2, error = ?3, finished_at = ?4 WHERE id = ?1",
            )?
            .execute(params![id, EnumColumn(status), error, finished_at])
    }

    pub fn append_event(&self, run_id: &str, event: &HostEvent) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached("INSERT INTO run_event (run_id, seq, event) VALUES (?1, ?2, ?3)")?
            .execute(params![
                run_id,
                i64::try_from(event.seq).unwrap_or(i64::MAX),
                json_text(event)?
            ])?;
        Ok(())
    }

    pub fn events(&self, run_id: &str) -> rusqlite::Result<Vec<HostEvent>> {
        self.connection
            .prepare_cached("SELECT event FROM run_event WHERE run_id = ?1 ORDER BY seq")?
            .query_map([run_id], |row| {
                crate::data::schema::columns::json_column(row, "event")
            })?
            .collect()
    }

    pub fn logs(&self, run_id: &str) -> rusqlite::Result<Vec<LogLineDto>> {
        Ok(self
            .events(run_id)?
            .into_iter()
            .filter_map(|event| match event.body {
                HostEventDraft::Log { line } => Some(line),
                _ => None,
            })
            .collect())
    }

    pub fn next_sequence(&self, run_id: &str) -> rusqlite::Result<EventSeq> {
        let last: Option<i64> = self
            .connection
            .prepare_cached(
                "SELECT seq FROM run_event WHERE run_id = ?1 ORDER BY seq DESC LIMIT 1",
            )?
            .query_row([run_id], |row| row.get(0))
            .optional()?;
        Ok(last.map_or(0, |seq| EventSeq::try_from(seq + 1).unwrap_or(0)))
    }

    pub fn prunable(&self, before: Timestamp, limit: i64) -> rusqlite::Result<Vec<String>> {
        self.connection
            .prepare_cached(
                "SELECT id FROM run WHERE finished_at < ?1 AND pruned_at IS NULL \
                 ORDER BY finished_at LIMIT ?2",
            )?
            .query_map(params![before, limit], |row| row.get(0))?
            .collect()
    }

    pub fn drop_payloads(&self, ids: &[String], pruned_at: Timestamp) -> rusqlite::Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let scope = placeholders(ids.len());
        self.connection.execute(
            &format!("UPDATE step SET input = NULL, output = NULL WHERE run_id IN ({scope})"),
            params_from_iter(ids),
        )?;
        self.connection.execute(
            &format!("DELETE FROM run_event WHERE run_id IN ({scope})"),
            params_from_iter(ids),
        )?;
        let mut values: Vec<SqlValue> = vec![SqlValue::Integer(pruned_at)];
        values.extend(ids.iter().map(|id| SqlValue::Text(id.clone())));
        self.connection.execute(
            &format!("UPDATE run SET pruned_at = ? WHERE id IN ({scope})"),
            params_from_iter(values),
        )?;
        Ok(())
    }

    pub fn referenced(&self) -> rusqlite::Result<Vec<String>> {
        self.connection
            .prepare_cached("SELECT DISTINCT run_id FROM message WHERE run_id IS NOT NULL")?
            .query_map([], |row| row.get(0))?
            .collect()
    }

    pub fn remove_finished(&self, except: &[String]) -> rusqlite::Result<usize> {
        let finished: Vec<String> = self
            .connection
            .prepare_cached(&format!(
                "SELECT id FROM run WHERE status IN ({})",
                placeholders(RunStatus::FINISHED.len())
            ))?
            .query_map(
                params_from_iter(status_values(RunStatus::FINISHED)),
                |row| row.get(0),
            )?
            .collect::<rusqlite::Result<Vec<String>>>()?
            .into_iter()
            .filter(|id| !except.contains(id))
            .collect();
        if finished.is_empty() {
            return Ok(0);
        }
        self.connection.execute(
            &format!(
                "DELETE FROM run WHERE id IN ({})",
                placeholders(finished.len())
            ),
            params_from_iter(&finished),
        )?;
        Ok(finished.len())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zvs_contract::{GraphNode, StreamId};

    use super::*;
    use crate::data::repositories::Repositories;
    use crate::testing::temporary_database;

    const STREAM: &str = "0199aa11-1111-7111-8111-111111111111";

    fn draft(id: &str, status: RunStatus, created_at: Timestamp) -> RunDraft {
        RunDraft {
            id: id.to_owned(),
            kind: RunKind::Job,
            subject_id: None,
            title: Some(format!("run {id}")),
            retry_of_id: None,
            status,
            graph: RunGraph::new(vec![GraphNode::new("a", "work").normalized()]),
            input: Value::Null,
            stream_id: format!("stream-{id}"),
            concurrency: 4,
            created_at,
            started_at: None,
        }
    }

    fn step(id: &str, run_id: &str, status: StepStatus) -> StepRow {
        StepRow {
            id: id.to_owned(),
            run_id: run_id.to_owned(),
            node_id: "a".to_owned(),
            step_type: "work".to_owned(),
            status,
            input: json!({ "q": 1 }),
            output: Value::Null,
            error: None,
            started_at: 5,
            finished_at: None,
            attempt: 1,
        }
    }

    #[test]
    fn cursors_round_trip_and_reject_malformed_text() {
        let encoded = encode_run_cursor(RunCursor {
            created_at: 12,
            id: "a:b",
        });
        assert_eq!(encoded, "12:a:b");
        assert_eq!(
            decode_run_cursor(&encoded),
            Some(RunCursor {
                created_at: 12,
                id: "a:b"
            })
        );
        for bad in [":x", "12:", "x:id", "1.5:id", "nope"] {
            assert_eq!(decode_run_cursor(bad), None, "{bad}");
        }
    }

    #[test]
    fn runs_steps_and_events_are_stored_and_read_back() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let runs = Repositories::new(connection).runs;
                let created = runs.create(&draft("r1", RunStatus::Queued, 10))?;
                assert_eq!(created.input, Value::Null);
                assert_eq!(created.graph.nodes[0].retry().max_attempts, 1);
                runs.update(
                    "r1",
                    &RunPatch {
                        status: Some(RunStatus::Failed),
                        error: Some(Some("boom".to_owned())),
                        finished_at: Some(20),
                        ..RunPatch::default()
                    },
                )?;
                let stored = runs.get("r1")?.expect("a run");
                assert_eq!(stored.status, RunStatus::Failed);
                assert_eq!(stored.error.as_deref(), Some("boom"));

                let mut first = step("s1", "r1", StepStatus::Running);
                runs.add_step(&first)?;
                first.status = StepStatus::Succeeded;
                first.output = json!("done");
                first.finished_at = Some(7);
                runs.update_step(&first)?;
                assert_eq!(runs.steps("r1")?, [first]);
                assert_eq!(
                    runs.succeeded_counts(&["r1".to_owned()])?,
                    [("r1".to_owned(), 1)]
                );

                let stream = StreamId::parse(STREAM).expect("a stream id");
                assert_eq!(runs.next_sequence("r1")?, 0);
                for seq in [0, 1] {
                    runs.append_event(
                        "r1",
                        &HostEvent::new(
                            stream,
                            seq,
                            1,
                            HostEventDraft::Log {
                                line: zvs_contract::log_line(&[("seq", json!(seq))]),
                            },
                        ),
                    )?;
                }
                assert_eq!(runs.next_sequence("r1")?, 2);
                assert_eq!(runs.logs("r1")?.len(), 2);
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn paging_filters_by_status_kind_text_window_and_cursor() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let runs = Repositories::new(connection).runs;
                runs.create(&draft("r1", RunStatus::Succeeded, 10))?;
                runs.create(&draft("r2", RunStatus::Failed, 20))?;
                runs.create(&RunDraft {
                    kind: RunKind::Chat,
                    ..draft("r3", RunStatus::Running, 30)
                })?;
                let ids =
                    |rows: Vec<RunRow>| rows.into_iter().map(|row| row.id).collect::<Vec<_>>();
                let query = RunPageQuery {
                    limit: 10,
                    ..RunPageQuery::default()
                };
                assert_eq!(ids(runs.page(&query)?), ["r3", "r2", "r1"]);
                assert_eq!(
                    ids(runs.page(&RunPageQuery {
                        statuses: vec![RunStatus::Failed],
                        ..query.clone()
                    })?),
                    ["r2"]
                );
                assert_eq!(
                    ids(runs.page(&RunPageQuery {
                        kinds: vec![RunKind::Chat],
                        ..query.clone()
                    })?),
                    ["r3"]
                );
                assert_eq!(
                    ids(runs.page(&RunPageQuery {
                        query: Some("RUN R1".to_owned()),
                        ..query.clone()
                    })?),
                    ["r1"]
                );
                assert_eq!(
                    ids(runs.page(&RunPageQuery {
                        from: Some(15),
                        to: Some(25),
                        ..query.clone()
                    })?),
                    ["r2"]
                );
                assert_eq!(
                    ids(runs.page(&RunPageQuery {
                        cursor: Some(encode_run_cursor(RunCursor {
                            created_at: 20,
                            id: "r2"
                        })),
                        ..query.clone()
                    })?),
                    ["r1"]
                );
                runs.update(
                    "r2",
                    &RunPatch {
                        finished_at: Some(100),
                        ..RunPatch::default()
                    },
                )?;
                assert_eq!(
                    ids(runs.page(&RunPageQuery {
                        live: true,
                        attention_since: Some(50),
                        ..query.clone()
                    })?),
                    ["r3", "r2"]
                );
                assert_eq!(
                    ids(runs.page(&RunPageQuery {
                        live: true,
                        attention_since: Some(150),
                        ..query
                    })?),
                    ["r3"]
                );
                assert_eq!(runs.unfinished()?.len(), 1);
                let counts = runs.counts()?;
                assert_eq!(counts.by_kind.len(), 2);
                assert_eq!(counts.by_status.len(), 3);
                Ok(())
            })
            .expect("the repository works");
    }

    #[test]
    fn pruning_drops_payloads_and_events_but_keeps_the_rows() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let runs = Repositories::new(connection).runs;
                runs.create(&draft("old", RunStatus::Succeeded, 1))?;
                runs.create(&draft("new", RunStatus::Succeeded, 2))?;
                runs.update(
                    "old",
                    &RunPatch {
                        finished_at: Some(10),
                        ..RunPatch::default()
                    },
                )?;
                runs.update(
                    "new",
                    &RunPatch {
                        finished_at: Some(90),
                        ..RunPatch::default()
                    },
                )?;
                runs.add_step(&StepRow {
                    output: json!("kept?"),
                    ..step("s1", "old", StepStatus::Succeeded)
                })?;
                let prunable = runs.prunable(50, 10)?;
                assert_eq!(prunable, ["old"]);
                runs.drop_payloads(&prunable, 77)?;
                assert_eq!(runs.get("old")?.and_then(|row| row.pruned_at), Some(77));
                let steps = runs.steps("old")?;
                assert_eq!(steps[0].input, Value::Null);
                assert_eq!(steps[0].output, Value::Null);
                assert!(runs.prunable(50, 10)?.is_empty());
                assert_eq!(runs.remove_finished(&["new".to_owned()])?, 1);
                assert_eq!(runs.list()?.len(), 1);
                Ok(())
            })
            .expect("the repository works");
    }
}
