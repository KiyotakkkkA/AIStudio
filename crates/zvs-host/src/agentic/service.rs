use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use serde_json::json;
use zvs_contract::validate::js_slice;
use zvs_contract::{
    AgenticBudgetDto, AgenticInstructInput, AgenticSessionDto, AgenticSessionStatus,
    AgenticStopReason, AgenticUsageDto, AppError, AppErrorCode, CreateAgenticSessionInput,
    ExtendAgenticSessionInput, GraphNode, RunGraph, RunId, RunKind, RunStatus, SetAgenticModeInput,
    StartRunInput, ToolListFilter, ToolTier, UpdateAgenticPlanInput, Validate,
};

use super::node::{AGENTIC_SESSION, SESSION_NODE_ID};
use super::plan::fail;
use super::store::SessionStore;
use crate::data::schema::AgenticSessionRow;
use crate::data::{Database, Repositories};
use crate::platform::clock::{SharedClock, SystemClock};
use crate::platform::ids::new_id;
use crate::services::RunService;
use crate::tools::ToolRegistry;
use crate::workspace::FolderAccessService;

pub const STEPS_KEY: &str = "agentic.budget.maxSteps";
pub const TOKENS_KEY: &str = "agentic.budget.maxTokens";
pub const MINUTES_KEY: &str = "agentic.budget.maxMinutes";
pub const RUN_TITLE_MAX: usize = 80;

pub const SESSION_FINISHED: &str = "Сессия завершена, изменить её нельзя";
pub const SESSION_RUNNING: &str = "Сессия выполняется: удалить её можно после остановки";
pub const SESSION_NOT_PAUSED: &str = "Продлить можно только приостановленную сессию";
pub const BUDGET_NOT_ABOVE_USAGE: &str = "Каждый новый лимит должен превышать уже израсходованное";
pub const PROVIDER_MISSING: &str = "Провайдер не найден";
pub const TOOL_UNKNOWN: &str = "Инструмент не зарегистрирован";
pub const RUN_INTERRUPTED: &str = "Выполнение прервано";

pub struct AgenticServiceOptions {
    pub database: Arc<Database>,
    pub runs: RunService,
    pub folders: FolderAccessService,
    pub tools: ToolRegistry,
    pub clock: SharedClock,
}

impl AgenticServiceOptions {
    pub fn new(
        database: Arc<Database>,
        runs: RunService,
        folders: FolderAccessService,
        tools: ToolRegistry,
    ) -> Self {
        Self {
            database,
            runs,
            folders,
            tools,
            clock: Arc::new(SystemClock),
        }
    }
}

struct Inner {
    store: SessionStore,
    runs: RunService,
    folders: FolderAccessService,
    tools: ToolRegistry,
}

#[derive(Clone)]
pub struct AgenticService {
    inner: Arc<Inner>,
}

fn run_title(goal: &str) -> String {
    let line = goal
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(goal);
    js_slice(line, RUN_TITLE_MAX).to_owned()
}

fn editable(row: &AgenticSessionRow) -> Result<(), AppError> {
    if row.status.is_finished() {
        Err(AppError::new(AppErrorCode::Conflict, SESSION_FINISHED))
    } else {
        Ok(())
    }
}

fn above_usage(budget: &AgenticBudgetDto, usage: &AgenticUsageDto) -> bool {
    budget.max_steps > usage.steps
        && budget.max_tokens > usage.tokens
        && budget.max_minutes.saturating_mul(60_000) > usage.elapsed_ms
}

fn graph(session_id: &str, tools: &[String]) -> RunGraph {
    RunGraph {
        allowed_tools: Some(tools.to_vec()),
        ..RunGraph::new(vec![GraphNode {
            input: Some(json!({ "sessionId": session_id })),
            ..GraphNode::new(SESSION_NODE_ID, AGENTIC_SESSION)
        }])
    }
}

impl AgenticService {
    pub fn new(options: AgenticServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                store: SessionStore::new(options.database, options.clock),
                runs: options.runs,
                folders: options.folders,
                tools: options.tools,
            }),
        }
    }

    pub fn store(&self) -> SessionStore {
        self.inner.store.clone()
    }

    pub fn default_budget(&self) -> Result<AgenticBudgetDto, AppError> {
        let fallback = AgenticBudgetDto::default();
        let read = |repositories: &Repositories<'_>, key: &str, fallback: u64| {
            Ok::<u64, crate::data::DataError>(
                repositories
                    .settings
                    .get(key)?
                    .and_then(|row| serde_json::from_str::<u64>(&row.value).ok())
                    .unwrap_or(fallback),
            )
        };
        let budget = self.inner.store.database().with_connection(|connection| {
            let repositories = Repositories::new(connection);
            Ok(AgenticBudgetDto {
                max_steps: read(&repositories, STEPS_KEY, fallback.max_steps)?,
                max_tokens: read(&repositories, TOKENS_KEY, fallback.max_tokens)?,
                max_minutes: read(&repositories, MINUTES_KEY, fallback.max_minutes)?,
            })
        })?;
        Ok(if budget.validate().is_ok() {
            budget
        } else {
            fallback
        })
    }

    fn tools(&self, requested: Option<&[String]>) -> Result<Vec<String>, AppError> {
        let registered = self.inner.tools.list(&ToolListFilter::default())?;
        let Some(requested) = requested else {
            return Ok(registered
                .into_iter()
                .filter(|tool| tool.enabled && tool.available && tool.tier != ToolTier::Off)
                .map(|tool| tool.key)
                .collect());
        };
        let known: HashSet<&str> = registered.iter().map(|tool| tool.key.as_str()).collect();
        let mut kept: Vec<String> = Vec::new();
        for key in requested {
            if !known.contains(key.as_str()) {
                return Err(AppError::new(AppErrorCode::ValidationFailed, TOOL_UNKNOWN)
                    .with_detail("key", key.clone()));
            }
            if !kept.contains(key) {
                kept.push(key.clone());
            }
        }
        Ok(kept)
    }

    fn launch(&self, row: &AgenticSessionRow, create: bool) -> Result<RunId, AppError> {
        let input = StartRunInput {
            subject_id: Some(row.id.clone()),
            title: Some(run_title(&row.goal)),
            ..StartRunInput::new(RunKind::Agentic, graph(&row.id, &row.tools))
        };
        let pending = row.clone();
        let now = self.inner.store.now();
        let (handle, _) = self.inner.runs.start_with(
            input,
            move |repositories, run_id| {
                let mut row = pending;
                row.run_id = Some(run_id.to_string());
                row.updated_at = now;
                if create {
                    repositories.agentic_sessions.create(&row)?;
                } else {
                    repositories.agentic_sessions.save(&row)?;
                }
                Ok(())
            },
            None,
        )?;
        Ok(handle.id)
    }

    pub fn create(&self, input: &CreateAgenticSessionInput) -> Result<AgenticSessionDto, AppError> {
        input.validate()?;
        let workspace = self
            .inner
            .folders
            .working_folder(Path::new(&input.workspace))?;
        let provider = input.provider_id.to_string();
        let exists = self.inner.store.database().with_connection(|connection| {
            Ok(Repositories::new(connection)
                .providers
                .find_by_id(&provider)?
                .is_some())
        })?;
        if !exists {
            return Err(AppError::new(AppErrorCode::NotFound, PROVIDER_MISSING));
        }
        let budget = match input.budget {
            Some(budget) => budget,
            None => self.default_budget()?,
        };
        let now = self.inner.store.now();
        let row = AgenticSessionRow {
            id: new_id().hyphenated().to_string(),
            goal: input.goal.trim().to_owned(),
            status: AgenticSessionStatus::Running,
            stop_reason: None,
            error: None,
            summary: None,
            mode: input.mode,
            pending_mode: None,
            workspace: workspace.to_string_lossy().into_owned(),
            provider_id: provider,
            model_id: input.model_id.clone(),
            tools: self.tools(input.tools.as_deref())?,
            budget,
            usage: AgenticUsageDto::default(),
            plan: Vec::new(),
            pending_plan: None,
            pending_instructions: Vec::new(),
            changed_files: Vec::new(),
            transcript: Vec::new(),
            run_id: None,
            created_at: now,
            updated_at: now,
        };
        self.launch(&row, true)?;
        self.get(&row.id)
    }

    fn settle(&self, row: AgenticSessionRow) -> Result<AgenticSessionRow, AppError> {
        if row.status != AgenticSessionStatus::Running {
            return Ok(row);
        }
        let run = match &row.run_id {
            Some(run_id) => RunId::parse(run_id).and_then(|id| self.inner.runs.get(&id).ok()),
            None => None,
        };
        if run
            .as_ref()
            .is_some_and(|run| RunStatus::UNFINISHED.contains(&run.status))
        {
            return Ok(row);
        }
        let (status, reason, error) = match run {
            Some(run) if run.status == RunStatus::Cancelled => (
                AgenticSessionStatus::Cancelled,
                AgenticStopReason::Cancelled,
                None,
            ),
            Some(run) if run.status == RunStatus::Failed => (
                AgenticSessionStatus::Failed,
                AgenticStopReason::Error,
                run.error.clone(),
            ),
            _ => (
                AgenticSessionStatus::Failed,
                AgenticStopReason::Error,
                Some(RUN_INTERRUPTED.to_owned()),
            ),
        };
        let (row, ()) = self.inner.store.update(&row.id, |row| {
            if row.status == AgenticSessionStatus::Running {
                row.status = status;
                row.stop_reason = Some(reason);
                row.error = error;
                fail(&mut row.plan);
            }
            Ok(())
        })?;
        Ok(row)
    }

    fn load(&self, id: &str) -> Result<AgenticSessionRow, AppError> {
        self.settle(self.inner.store.load(id)?)
    }

    pub fn get(&self, id: &str) -> Result<AgenticSessionDto, AppError> {
        self.inner.store.dto(&self.load(id)?)
    }

    pub fn list(&self) -> Result<Vec<AgenticSessionDto>, AppError> {
        self.inner
            .store
            .list()?
            .into_iter()
            .map(|row| self.inner.store.dto(&self.settle(row)?))
            .collect()
    }

    fn change(
        &self,
        id: &str,
        change: impl FnOnce(&mut AgenticSessionRow),
    ) -> Result<AgenticSessionDto, AppError> {
        self.load(id)?;
        let (row, ()) = self.inner.store.update(id, |row| {
            editable(row)?;
            change(row);
            Ok(())
        })?;
        self.inner.store.dto(&row)
    }

    pub fn update_plan(
        &self,
        input: &UpdateAgenticPlanInput,
    ) -> Result<AgenticSessionDto, AppError> {
        input.validate()?;
        let steps = input.steps.clone();
        self.change(&input.id.to_string(), move |row| {
            row.pending_plan = Some(steps);
        })
    }

    pub fn set_mode(&self, input: &SetAgenticModeInput) -> Result<AgenticSessionDto, AppError> {
        let mode = input.mode;
        self.change(&input.id.to_string(), move |row| {
            row.pending_mode = (mode != row.mode).then_some(mode);
        })
    }

    pub fn instruct(&self, input: &AgenticInstructInput) -> Result<AgenticSessionDto, AppError> {
        input.validate()?;
        let text = input.text.trim().to_owned();
        self.change(&input.id.to_string(), move |row| {
            row.pending_instructions.push(text);
        })
    }

    pub fn extend(&self, input: &ExtendAgenticSessionInput) -> Result<AgenticSessionDto, AppError> {
        input.validate()?;
        let id = input.id.to_string();
        let mut row = self.load(&id)?;
        if row.status != AgenticSessionStatus::Paused {
            return Err(AppError::new(AppErrorCode::Conflict, SESSION_NOT_PAUSED));
        }
        if !above_usage(&input.budget, &row.usage) {
            return Err(AppError::new(
                AppErrorCode::ValidationFailed,
                BUDGET_NOT_ABOVE_USAGE,
            ));
        }
        row.budget = input.budget;
        row.status = AgenticSessionStatus::Running;
        row.stop_reason = None;
        row.error = None;
        self.launch(&row, false)?;
        self.get(&id)
    }

    pub async fn cancel(&self, id: &str) -> Result<(), AppError> {
        let row = self.load(id)?;
        match row.status {
            AgenticSessionStatus::Running => {
                if let Some(run_id) = row.run_id.as_deref().and_then(RunId::parse) {
                    self.inner.runs.cancel(&run_id).await?;
                }
                self.load(id)?;
                Ok(())
            }
            AgenticSessionStatus::Paused => {
                self.inner.store.update(id, |row| {
                    row.status = AgenticSessionStatus::Cancelled;
                    row.stop_reason = Some(AgenticStopReason::Cancelled);
                    Ok(())
                })?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub fn remove(&self, id: &str) -> Result<(), AppError> {
        let row = self.load(id)?;
        if row.status == AgenticSessionStatus::Running {
            return Err(AppError::new(AppErrorCode::Conflict, SESSION_RUNNING));
        }
        self.inner.store.database().with_connection(|connection| {
            Repositories::new(connection)
                .agentic_sessions
                .remove(&row.id)?;
            Ok(())
        })
    }
}
