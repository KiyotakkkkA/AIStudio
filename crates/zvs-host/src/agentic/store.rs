use std::sync::Arc;

use zvs_contract::{
    AgenticSessionDto, AgenticSessionId, AppError, AppErrorCode, ProviderId, RunId, StreamId,
};

use crate::data::schema::AgenticSessionRow;
use crate::data::{Database, Repositories};
use crate::platform::clock::SharedClock;

pub const SESSION_NOT_FOUND: &str = "Сессия не найдена";

fn malformed(field: &str) -> AppError {
    AppError::new(
        AppErrorCode::Unknown,
        format!("Сохранённая сессия повреждена: {field}"),
    )
}

#[derive(Clone)]
pub struct SessionStore {
    database: Arc<Database>,
    clock: SharedClock,
}

impl SessionStore {
    pub fn new(database: Arc<Database>, clock: SharedClock) -> Self {
        Self { database, clock }
    }

    pub fn now(&self) -> i64 {
        self.clock.now()
    }

    pub fn database(&self) -> &Arc<Database> {
        &self.database
    }

    pub fn load(&self, id: &str) -> Result<AgenticSessionRow, AppError> {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).agentic_sessions.get(id)?)
            })?
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, SESSION_NOT_FOUND))
    }

    pub fn list(&self) -> Result<Vec<AgenticSessionRow>, AppError> {
        self.database.with_connection(|connection| {
            Ok(Repositories::new(connection).agentic_sessions.list()?)
        })
    }

    pub fn update<T>(
        &self,
        id: &str,
        change: impl FnOnce(&mut AgenticSessionRow) -> Result<T, AppError>,
    ) -> Result<(AgenticSessionRow, T), AppError> {
        let now = self.clock.now();
        let mut outcome: Option<Result<T, AppError>> = None;
        let row = self.database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            let Some(mut row) = repositories.agentic_sessions.get(id)? else {
                return Ok(None);
            };
            match change(&mut row) {
                Ok(value) => {
                    row.updated_at = now;
                    repositories.agentic_sessions.save(&row)?;
                    outcome = Some(Ok(value));
                    Ok(Some(row))
                }
                Err(error) => {
                    outcome = Some(Err(error));
                    Ok(None)
                }
            }
        })?;
        match (row, outcome) {
            (Some(row), Some(Ok(value))) => Ok((row, value)),
            (_, Some(Err(error))) => Err(error),
            _ => Err(AppError::new(AppErrorCode::NotFound, SESSION_NOT_FOUND)),
        }
    }

    pub fn dto(&self, row: &AgenticSessionRow) -> Result<AgenticSessionDto, AppError> {
        let run = match &row.run_id {
            Some(run_id) => self.database.with_connection(|connection| {
                Ok(Repositories::new(connection).runs.get(run_id)?)
            })?,
            None => None,
        };
        let (run_id, stream_id) = run
            .and_then(|run| Some((RunId::parse(&run.id)?, StreamId::parse(&run.stream_id)?)))
            .unzip();
        Ok(AgenticSessionDto {
            id: AgenticSessionId::parse(&row.id).ok_or_else(|| malformed("id"))?,
            goal: row.goal.clone(),
            status: row.status,
            stop_reason: row.stop_reason,
            error: row.error.clone(),
            mode: row.mode,
            pending_mode: row.pending_mode,
            workspace: row.workspace.clone(),
            provider_id: ProviderId::parse(&row.provider_id)
                .ok_or_else(|| malformed("providerId"))?,
            model_id: row.model_id.clone(),
            tools: row.tools.clone(),
            budget: row.budget,
            usage: row.usage,
            plan: row.plan.clone(),
            pending_plan: row.pending_plan.clone(),
            pending_instructions: row.pending_instructions.clone(),
            changed_files: row.changed_files.clone(),
            run_id,
            stream_id,
            summary: row.summary.clone(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}
