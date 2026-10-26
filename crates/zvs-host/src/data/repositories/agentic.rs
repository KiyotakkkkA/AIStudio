use rusqlite::{Connection, OptionalExtension, params};

use crate::data::schema::columns::{EnumColumn, json_column, json_text};
use crate::data::schema::{AgenticSessionRow, TranscriptEntry};

pub struct AgenticSessionRepository<'c> {
    connection: &'c Connection,
}

impl<'c> AgenticSessionRepository<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self { connection }
    }

    pub fn list(&self) -> rusqlite::Result<Vec<AgenticSessionRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM agentic_session ORDER BY updated_at DESC, id",
                AgenticSessionRow::COLUMNS
            ))?
            .query_map([], AgenticSessionRow::from_row)?
            .collect()
    }

    pub fn get(&self, id: &str) -> rusqlite::Result<Option<AgenticSessionRow>> {
        self.connection
            .prepare_cached(&format!(
                "SELECT {} FROM agentic_session WHERE id = ?1",
                AgenticSessionRow::COLUMNS
            ))?
            .query_row([id], AgenticSessionRow::from_row)
            .optional()
    }

    pub fn workspace(&self, id: &str) -> rusqlite::Result<Option<String>> {
        self.connection
            .prepare_cached("SELECT workspace FROM agentic_session WHERE id = ?1")?
            .query_row([id], |row| row.get(0))
            .optional()
    }

    pub fn create(&self, row: &AgenticSessionRow) -> rusqlite::Result<()> {
        self.connection
            .prepare_cached(&format!(
                "INSERT INTO agentic_session ({}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, \
                 ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
                AgenticSessionRow::COLUMNS
            ))?
            .execute(params![
                row.id,
                row.goal,
                EnumColumn(row.status),
                row.stop_reason.map(EnumColumn),
                row.error,
                row.summary,
                EnumColumn(row.mode),
                row.pending_mode.map(EnumColumn),
                row.workspace,
                row.provider_id,
                row.model_id,
                json_text(&row.tools)?,
                json_text(&row.budget)?,
                json_text(&row.usage)?,
                json_text(&row.plan)?,
                row.pending_plan.as_ref().map(json_text).transpose()?,
                json_text(&row.pending_instructions)?,
                json_text(&row.changed_files)?,
                row.run_id,
                row.created_at,
                row.updated_at,
            ])?;
        Ok(())
    }

    pub fn save(&self, row: &AgenticSessionRow) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached(
                "UPDATE agentic_session SET status = ?2, stop_reason = ?3, error = ?4, \
                 summary = ?5, mode = ?6, pending_mode = ?7, tools = ?8, budget = ?9, \
                 usage = ?10, plan = ?11, pending_plan = ?12, pending_instructions = ?13, \
                 changed_files = ?14, run_id = ?15, updated_at = ?16 \
                 WHERE id = ?1",
            )?
            .execute(params![
                row.id,
                EnumColumn(row.status),
                row.stop_reason.map(EnumColumn),
                row.error,
                row.summary,
                EnumColumn(row.mode),
                row.pending_mode.map(EnumColumn),
                json_text(&row.tools)?,
                json_text(&row.budget)?,
                json_text(&row.usage)?,
                json_text(&row.plan)?,
                row.pending_plan.as_ref().map(json_text).transpose()?,
                json_text(&row.pending_instructions)?,
                json_text(&row.changed_files)?,
                row.run_id,
                row.updated_at,
            ])?
            > 0)
    }

    pub fn transcript(&self, id: &str) -> rusqlite::Result<Vec<TranscriptEntry>> {
        self.connection
            .prepare_cached(
                "SELECT entry FROM agentic_transcript WHERE session_id = ?1 ORDER BY seq",
            )?
            .query_map([id], |row| json_column(row, "entry"))?
            .collect()
    }

    pub fn append_transcript(
        &self,
        id: &str,
        seq: usize,
        entries: &[TranscriptEntry],
    ) -> rusqlite::Result<()> {
        let mut statement = self.connection.prepare_cached(
            "INSERT INTO agentic_transcript (session_id, seq, entry) VALUES (?1, ?2, ?3)",
        )?;
        for (offset, entry) in entries.iter().enumerate() {
            statement.execute(params![
                id,
                i64::try_from(seq + offset).unwrap_or(i64::MAX),
                json_text(entry)?
            ])?;
        }
        Ok(())
    }

    pub fn remove(&self, id: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .prepare_cached("DELETE FROM agentic_session WHERE id = ?1")?
            .execute([id])?
            > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Repositories;
    use crate::data::schema::TranscriptEntry;
    use crate::testing::temporary_database;
    use zvs_contract::{
        AgenticBudgetDto, AgenticPermissionMode, AgenticPlanEditDto, AgenticPlanStepDto,
        AgenticPlanStepStatus, AgenticSessionStatus, AgenticStopReason, AgenticUsageDto,
    };

    fn session(id: &str, updated_at: i64) -> AgenticSessionRow {
        AgenticSessionRow {
            id: id.to_owned(),
            goal: "Написать сводку".to_owned(),
            status: AgenticSessionStatus::Running,
            stop_reason: None,
            error: None,
            summary: None,
            mode: AgenticPermissionMode::Ask,
            pending_mode: None,
            workspace: "C:\\work".to_owned(),
            provider_id: "provider".to_owned(),
            model_id: "model".to_owned(),
            tools: vec!["fs.read".to_owned()],
            budget: AgenticBudgetDto::default(),
            usage: AgenticUsageDto::default(),
            plan: Vec::new(),
            pending_plan: None,
            pending_instructions: Vec::new(),
            changed_files: Vec::new(),
            run_id: None,
            created_at: 1,
            updated_at,
        }
    }

    #[test]
    fn a_session_round_trips_and_lists_newest_first() {
        let database = temporary_database();
        database
            .with_connection(|connection| {
                let repository = Repositories::new(connection).agentic_sessions;
                repository.create(&session("a", 1))?;
                repository.create(&session("b", 2))?;
                let mut changed = session("a", 3);
                changed.status = AgenticSessionStatus::Paused;
                changed.stop_reason = Some(AgenticStopReason::Steps);
                changed.pending_mode = Some(AgenticPermissionMode::Auto);
                changed.plan = vec![AgenticPlanStepDto {
                    id: "p1".to_owned(),
                    title: "Прочитать".to_owned(),
                    status: AgenticPlanStepStatus::Done,
                    step_ids: Vec::new(),
                    summary: Some("Готово".to_owned()),
                }];
                changed.pending_plan = Some(vec![AgenticPlanEditDto {
                    id: None,
                    title: "Проверить".to_owned(),
                }]);
                let entry = |content: &str| TranscriptEntry {
                    role: "user".to_owned(),
                    content: content.to_owned(),
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                };
                assert!(repository.save(&changed)?);
                assert_eq!(repository.get("a")?, Some(changed));
                repository.append_transcript("a", 0, &[entry("Цель")])?;
                repository.append_transcript("a", 1, &[entry("Шаг"), entry("Итог")])?;
                assert_eq!(
                    repository.transcript("a")?,
                    [entry("Цель"), entry("Шаг"), entry("Итог")]
                );
                assert!(
                    repository
                        .append_transcript("a", 1, &[entry("Повтор")])
                        .is_err()
                );
                repository.append_transcript("b", 0, &[entry("Другая")])?;
                let order: Vec<String> = repository.list()?.into_iter().map(|row| row.id).collect();
                assert_eq!(order, ["a", "b"]);
                assert_eq!(repository.workspace("b")?.as_deref(), Some("C:\\work"));
                assert!(repository.remove("b")?);
                assert_eq!(repository.workspace("b")?, None);
                assert!(repository.transcript("b")?.is_empty());
                Ok(())
            })
            .expect("the repository works");
    }
}
