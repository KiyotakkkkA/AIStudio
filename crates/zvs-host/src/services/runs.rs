use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures::FutureExt;
use futures::future::{BoxFuture, Shared, join_all};
use serde_json::json;
use tokio::runtime::Handle;
use tokio::sync::oneshot;
use zvs_contract::{
    AppError, AppErrorCode, HostEvent, HostEventDraft, RunApprovalDto, RunCountsDto, RunDetailDto,
    RunDto, RunHandleDto, RunId, RunKind, RunListFilter, RunPageDto, RunProgressDto, RunStatus,
    RunSummaryDto, RunsClearFinishedOutput, StartRunInput, StepDto, StepId, StepStatus, StreamId,
    ToolTier, Validate, log_line,
};

use super::approvals::{ApprovalService, DEFAULT_APPROVAL_TIMEOUT};
use super::permissions::{Admission, PermissionContext, PermissionService};
use super::questions::QuestionService;
use crate::data::repositories::{RunCursor, RunDraft, RunPageQuery, encode_run_cursor};
use crate::data::schema::{RunRow, StepRow};
use crate::data::{DataError, Database, Repositories};
use crate::kernel::scheduler::step_event;
use crate::kernel::{
    AdmissionOrigin, AdmitHook, ApprovalDecision, ApprovalHook, DeferredApproval, Finish,
    KernelServices, NodeRegistry, Observers, PermissionRequirement, QuestionHook, RunObserver,
    RunStream, RunWatcher, Scheduler, SchedulerOptions, StepContext, Subscription,
};
use crate::platform::clock::{SharedClock, SystemClock};
use crate::platform::events::{EventBus, StreamOptions};
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};
use crate::tools::ToolRegistry;

pub const DEFAULT_GRACE: Duration = Duration::from_millis(500);
pub const MAX_GRACE: Duration = Duration::from_millis(30_000);
pub const ATTENTION_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);
pub const RUN_NOT_FOUND: &str = "Запуск не найден";
pub const SERVICE_STOPPED: &str = "Сервис выполнения остановлен";
pub const RUN_NOT_FINISHED: &str = "Запуск ещё не завершён";
pub const CHAT_RETRY_REFUSED: &str =
    "Ход чата повторяется со страницы чата: граф ссылается на сообщения исходного хода";
pub const RESUMING_MESSAGE: &str = "Возобновление с последней контрольной точки";
pub const ABANDONED_ON_EXIT: &str = "Выполнение прервано завершением приложения";
pub const APPROVAL_INTERRUPTED: &str = "Approval interrupted";
pub const OUTSIDE_ALLOWED_TOOLS: &str =
    "Инструмент не входит в набор, разрешённый для этого запуска";
pub const NOT_RESUMABLE: &str =
    "Автовозобновление недоступно: узел имеет побочные эффекты или отсутствует";

pub struct RunServiceOptions {
    pub database: Arc<Database>,
    pub events: EventBus,
    pub registry: Arc<NodeRegistry>,
    pub services: KernelServices,
    pub clock: SharedClock,
    pub logger: SharedLogger,
    pub grace: Duration,
    pub approval_timeout: Duration,
    pub attention_window: Duration,
    pub approval: Option<ApprovalHook>,
    pub admit: Option<AdmitHook>,
    pub runtime: Option<Handle>,
    pub tools: Option<ToolRegistry>,
}

impl RunServiceOptions {
    pub fn new(database: Arc<Database>, events: EventBus, registry: Arc<NodeRegistry>) -> Self {
        Self {
            database,
            events,
            registry,
            services: KernelServices::default(),
            clock: Arc::new(SystemClock),
            logger: silent_logger(),
            grace: DEFAULT_GRACE,
            approval_timeout: DEFAULT_APPROVAL_TIMEOUT,
            attention_window: ATTENTION_WINDOW,
            approval: None,
            admit: None,
            runtime: None,
            tools: None,
        }
    }
}

type Done = Shared<BoxFuture<'static, ()>>;

struct ActiveRun {
    scheduler: Arc<Scheduler>,
    done: Done,
}

struct Gate {
    database: Arc<Database>,
    permissions: Arc<PermissionService>,
    approvals: Arc<ApprovalService>,
    tools: Option<ToolRegistry>,
    clock: SharedClock,
}

impl Gate {
    async fn admit_tool(
        &self,
        tool: String,
        tier: Option<ToolTier>,
        context: StepContext,
        origin: AdmissionOrigin,
    ) -> Result<(), AppError> {
        let run = read_run(&self.database, &context.run_id.to_string())?;
        if origin == AdmissionOrigin::Model && !run.graph.allows_tool(&tool) {
            return Err(
                AppError::new(AppErrorCode::PermissionDenied, OUTSIDE_ALLOWED_TOOLS)
                    .with_detail("key", tool),
            );
        }
        if origin == AdmissionOrigin::Graph {
            let shortcut = matches!(
                (run.kind, tool.as_str()),
                (RunKind::Chat | RunKind::Skill, "llm.generate")
                    | (RunKind::Chat, "vector.search")
                    | (RunKind::Indexing, "vector.index")
                    | (RunKind::Download, "download.fetch")
            );
            if shortcut {
                return Ok(());
            }
        }
        if let Some(tools) = &self.tools
            && tools
                .ceiling(&tool)?
                .is_some_and(|ceiling| !ceiling.enabled || !ceiling.available)
        {
            return Err(AppError::new(
                AppErrorCode::PermissionDenied,
                "Tool disabled",
            ));
        }
        let mut scopes = run.graph.permission_scopes().to_vec();
        if run.kind == RunKind::Scenario
            && let Some(subject) = &run.subject_id
        {
            scopes.push(format!("scenario:{subject}"));
        }
        let permission = PermissionContext {
            run_id: run.id.clone(),
            scopes,
        };
        let minimum = tier.unwrap_or(ToolTier::Auto);
        match self
            .permissions
            .admit_subject(&tool, &permission, minimum)?
        {
            Admission::Allow => Ok(()),
            Admission::Deny => Err(AppError::new(
                AppErrorCode::PermissionDenied,
                "Permission denied",
            )),
            Admission::Ask => {
                let scope = permission.scopes.first().map_or("global", String::as_str);
                if context.defer_approval(DeferredApproval {
                    subject: tool.clone(),
                    scope: scope.to_owned(),
                }) {
                    return Ok(());
                }
                let request = self.approvals.create(&run.id, &tool, scope)?;
                let decision = context.request_approval(request).await?;
                if decision != ApprovalDecision::Approved {
                    return Err(AppError::new(
                        AppErrorCode::ApprovalDenied,
                        "Approval denied",
                    ));
                }
                if self
                    .permissions
                    .admit_subject(&tool, &permission, minimum)?
                    == Admission::Deny
                {
                    return Err(AppError::new(
                        AppErrorCode::PermissionDenied,
                        "Permission revoked",
                    ));
                }
                Ok(())
            }
        }
    }
}

fn read_run(database: &Database, id: &str) -> Result<RunRow, AppError> {
    database
        .with_connection(|connection| Ok(Repositories::new(connection).runs.get(id)?))?
        .ok_or_else(|| AppError::new(AppErrorCode::NotFound, RUN_NOT_FOUND))
}

fn malformed(field: &str) -> AppError {
    AppError::new(
        AppErrorCode::Unknown,
        format!("Сохранённый запуск повреждён: {field}"),
    )
}

fn run_dto(row: &RunRow) -> Result<RunDto, AppError> {
    Ok(RunDto {
        id: RunId::parse(&row.id).ok_or_else(|| malformed("id"))?,
        stream_id: StreamId::parse(&row.stream_id).ok_or_else(|| malformed("streamId"))?,
        kind: row.kind,
        subject_id: row.subject_id.clone(),
        title: row.title.clone(),
        retry_of_id: row.retry_of_id.as_deref().and_then(RunId::parse),
        status: row.status,
        graph: row.graph.clone(),
        input: row.input.clone(),
        outcome: row.outcome.clone(),
        concurrency: u64::try_from(row.concurrency).unwrap_or(1),
        created_at: row.created_at,
        started_at: row.started_at,
        finished_at: row.finished_at,
        pruned_at: row.pruned_at,
        error: row.error.clone(),
    })
}

fn step_dto(row: StepRow) -> Result<StepDto, AppError> {
    let output =
        (row.status == StepStatus::Succeeded && !row.output.is_null()).then_some(row.output);
    Ok(StepDto {
        id: StepId::parse(&row.id).ok_or_else(|| malformed("step.id"))?,
        run_id: RunId::parse(&row.run_id).ok_or_else(|| malformed("step.runId"))?,
        node_id: row.node_id,
        step_type: row.step_type,
        status: row.status,
        input: row.input,
        output,
        error: row.error,
        started_at: row.started_at,
        finished_at: row.finished_at,
        attempt: u64::try_from(row.attempt).unwrap_or(1),
    })
}

struct Inner {
    database: Arc<Database>,
    events: EventBus,
    registry: Arc<NodeRegistry>,
    clock: SharedClock,
    logger: SharedLogger,
    attention_window: Duration,
    runtime: Option<Handle>,
    options: Arc<SchedulerOptions>,
    permissions: Arc<PermissionService>,
    approvals: Arc<ApprovalService>,
    questions: Arc<QuestionService>,
    observers: Arc<Observers>,
    active: Mutex<HashMap<String, ActiveRun>>,
    closed: AtomicBool,
}

#[derive(Clone)]
pub struct RunService {
    inner: Arc<Inner>,
}

impl RunService {
    pub fn new(options: RunServiceOptions) -> Result<Self, AppError> {
        if options.grace > MAX_GRACE || !options.grace.subsec_nanos().is_multiple_of(1_000_000) {
            return Err(AppError::new(
                AppErrorCode::ValidationFailed,
                "Invalid cancellation grace period",
            ));
        }
        let permissions = Arc::new(PermissionService::new(
            Arc::clone(&options.database),
            Arc::clone(&options.registry),
            options.clock.clone(),
        ));
        let approvals = Arc::new(ApprovalService::new(
            Arc::clone(&options.database),
            Arc::clone(&permissions),
            options.approval_timeout,
            options.clock.clone(),
        )?);
        let approval = options.approval.unwrap_or_else(|| {
            let approvals = Arc::clone(&approvals);
            Arc::new(move |request, cancel| {
                let approvals = Arc::clone(&approvals);
                async move { approvals.wait(&request, &cancel).await }.boxed()
            })
        });
        let asked = Arc::new(QuestionService::new());
        let questions: QuestionHook = {
            let asked = Arc::clone(&asked);
            Arc::new(move |request, cancel| {
                let asked = Arc::clone(&asked);
                async move { asked.wait(request, &cancel).await }.boxed()
            })
        };
        let admit = options.admit.unwrap_or_else(|| {
            let gate = Arc::new(Gate {
                database: Arc::clone(&options.database),
                permissions: Arc::clone(&permissions),
                approvals: Arc::clone(&approvals),
                tools: options.tools.clone(),
                clock: options.clock.clone(),
            });
            Arc::new(move |requirement, context, origin| {
                let gate = Arc::clone(&gate);
                async move {
                    match requirement {
                        PermissionRequirement::None => Ok(()),
                        PermissionRequirement::Tool { tool, tier } => {
                            gate.admit_tool(tool.clone(), tier, context, origin).await?;
                            if let Some(tools) = &gate.tools {
                                tools.record_use(&tool, gate.clock.now())?;
                            }
                            Ok(())
                        }
                    }
                }
                .boxed()
            })
        });
        let scheduler = Arc::new(SchedulerOptions {
            database: Arc::clone(&options.database),
            registry: Arc::clone(&options.registry),
            services: options.services,
            clock: options.clock.clone(),
            grace: options.grace,
            logger: options.logger.clone(),
            approval: Some(approval),
            questions: Some(questions),
            admit: Some(admit),
        });
        Ok(Self {
            inner: Arc::new(Inner {
                database: options.database,
                events: options.events,
                registry: options.registry,
                clock: options.clock,
                logger: options.logger,
                attention_window: options.attention_window,
                runtime: options.runtime,
                options: scheduler,
                permissions,
                approvals,
                questions: asked,
                observers: Arc::new(Observers::default()),
                active: Mutex::new(HashMap::new()),
                closed: AtomicBool::new(false),
            }),
        })
    }

    pub fn permissions(&self) -> Arc<PermissionService> {
        Arc::clone(&self.inner.permissions)
    }

    pub fn approvals(&self) -> Arc<ApprovalService> {
        Arc::clone(&self.inner.approvals)
    }

    pub fn questions(&self) -> Arc<QuestionService> {
        Arc::clone(&self.inner.questions)
    }

    pub fn registry(&self) -> Arc<NodeRegistry> {
        Arc::clone(&self.inner.registry)
    }

    fn active(&self) -> MutexGuard<'_, HashMap<String, ActiveRun>> {
        self.inner
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub fn approve(&self, id: &RunId, approval_id: &str, always: bool) -> Result<(), AppError> {
        self.inner.approvals.decide(
            &id.to_string(),
            approval_id,
            ApprovalDecision::Approved,
            always,
        )
    }

    pub fn deny(&self, id: &RunId, approval_id: &str) -> Result<(), AppError> {
        self.inner.approvals.decide(
            &id.to_string(),
            approval_id,
            ApprovalDecision::Denied,
            false,
        )
    }

    pub fn start(&self, input: StartRunInput) -> Result<RunHandleDto, AppError> {
        self.start_with(input, |_, _| Ok(()), None)
            .map(|(handle, _)| handle)
    }

    pub fn start_with(
        &self,
        input: StartRunInput,
        prepare: impl FnOnce(&Repositories<'_>, &RunId) -> Result<(), DataError>,
        observer: Option<RunObserver>,
    ) -> Result<(RunHandleDto, Option<Subscription>), AppError> {
        if self.inner.closed.load(Ordering::SeqCst) {
            return Err(AppError::new(AppErrorCode::Conflict, SERVICE_STOPPED));
        }
        input.validate()?;
        let graph = input.graph.clone().normalized();
        self.inner.registry.validate(&graph)?;
        let id = RunId::new(new_id());
        let stream_id = StreamId::new(new_id());
        let draft = RunDraft {
            id: id.to_string(),
            kind: input.kind,
            subject_id: input.subject_id.clone(),
            title: input.title().map(str::to_owned),
            retry_of_id: input.retry_of_id.map(|run| run.to_string()),
            status: RunStatus::Queued,
            graph,
            input: input.input().clone(),
            stream_id: stream_id.to_string(),
            concurrency: i64::try_from(input.concurrency()).unwrap_or(1),
            created_at: self.inner.clock.now(),
            started_at: None,
        };
        let run = self.inner.database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            let run = repositories.runs.create(&draft)?;
            prepare(&repositories, &id)?;
            Ok(run)
        })?;
        let subscription = observer.map(|observer| {
            let token = self.inner.observers.add(&run.id, observer);
            Subscription::new(Arc::clone(&self.inner.observers), run.id.clone(), token)
        });
        let stream = self.open_stream(&run)?;
        stream.emit(HostEventDraft::Log {
            line: log_line(&[("runId", json!(run.id)), ("status", json!("queued"))]),
        })?;
        self.launch(run, stream)?;
        Ok((RunHandleDto { id, stream_id }, subscription))
    }

    fn open_stream(&self, run: &RunRow) -> Result<Arc<RunStream>, AppError> {
        let next_seq = self.inner.database.with_connection(|connection| {
            Ok(Repositories::new(connection).runs.next_sequence(&run.id)?)
        })?;
        let handle = self.inner.events.open_stream_with(StreamOptions {
            id: Some(StreamId::parse(&run.stream_id).ok_or_else(|| malformed("streamId"))?),
            next_seq: Some(next_seq),
            record: None,
        })?;
        Ok(Arc::new(RunStream::new(
            run.id.clone(),
            handle,
            Arc::clone(&self.inner.database),
            Arc::clone(&self.inner.observers),
        )))
    }

    pub fn watch(&self, watcher: RunWatcher) {
        self.inner.observers.watch(watcher);
    }

    pub fn subscribe(&self, id: &str, observer: RunObserver) -> Result<Subscription, AppError> {
        read_run(&self.inner.database, id)?;
        let token = self.inner.observers.add(id, observer);
        Ok(Subscription::new(
            Arc::clone(&self.inner.observers),
            id.to_owned(),
            token,
        ))
    }

    fn launch(&self, run: RunRow, stream: Arc<RunStream>) -> Result<(), AppError> {
        let id = run.id.clone();
        let scheduler = Arc::new(Scheduler::new(
            run,
            stream,
            Arc::clone(&self.inner.options),
        )?);
        let (sender, receiver) = oneshot::channel::<()>();
        let done: Done = receiver.map(|_| ()).boxed().shared();
        self.active().insert(
            id.clone(),
            ActiveRun {
                scheduler: Arc::clone(&scheduler),
                done,
            },
        );
        let inner = Arc::clone(&self.inner);
        let work = async move {
            tokio::task::yield_now().await;
            if let Err(error) = scheduler.execute().await {
                inner.logger.log(
                    LogLevel::Error,
                    "kernel",
                    "Could not persist run failure",
                    &[("runId", json!(id)), ("error", json!(error.message))],
                );
            }
            inner
                .active
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&id);
            let _ = sender.send(());
        };
        match &self.inner.runtime {
            Some(runtime) => drop(runtime.spawn(work)),
            None => drop(tokio::spawn(work)),
        }
        Ok(())
    }

    pub async fn cancel(&self, id: &RunId) -> Result<(), AppError> {
        let id = id.to_string();
        read_run(&self.inner.database, &id)?;
        let active = self
            .active()
            .get(&id)
            .map(|active| (Arc::clone(&active.scheduler), active.done.clone()));
        if let Some((scheduler, done)) = active {
            scheduler.cancel();
            done.await;
        }
        Ok(())
    }

    pub async fn wait(&self, id: &RunId) {
        let done = self
            .active()
            .get(&id.to_string())
            .map(|active| active.done.clone());
        if let Some(done) = done {
            done.await;
        }
    }

    pub fn get(&self, id: &RunId) -> Result<RunDto, AppError> {
        run_dto(&read_run(&self.inner.database, &id.to_string())?)
    }

    pub fn list(&self, filter: &RunListFilter) -> Result<RunPageDto, AppError> {
        filter.validate()?;
        let limit = filter.limit();
        let window = i64::try_from(self.inner.attention_window.as_millis()).unwrap_or(i64::MAX);
        let query = RunPageQuery {
            statuses: filter.statuses.clone().unwrap_or_default(),
            kinds: filter.kinds.clone().unwrap_or_default(),
            subject_id: filter.subject_id.clone(),
            query: filter.query().map(str::to_owned),
            from: filter.from,
            to: filter.to,
            live: filter.live(),
            attention_since: Some(self.inner.clock.now() - window),
            limit: i64::try_from(limit + 1).unwrap_or(i64::MAX),
            cursor: filter.cursor.clone(),
        };
        let mut rows = self
            .inner
            .database
            .with_connection(|connection| Ok(Repositories::new(connection).runs.page(&query)?))?;
        let more = rows.len() as u64 > limit;
        rows.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        let next_cursor = more.then(|| rows.last()).flatten().map(|last| {
            encode_run_cursor(RunCursor {
                created_at: last.created_at,
                id: &last.id,
            })
        });
        Ok(RunPageDto {
            items: self.summaries(&rows)?,
            next_cursor,
            counts: self.counts()?,
        })
    }

    pub fn detail(&self, id: &RunId) -> Result<RunDetailDto, AppError> {
        let row = read_run(&self.inner.database, &id.to_string())?;
        let run = run_dto(&row)?;
        let (logs, events) = self.inner.database.with_connection(|connection| {
            let runs = Repositories::new(connection).runs;
            Ok((runs.logs(&row.id)?, runs.events(&row.id)?))
        })?;
        let summary = self
            .summaries(std::slice::from_ref(&row))?
            .pop()
            .ok_or_else(|| malformed("summary"))?;
        Ok(RunDetailDto {
            run,
            summary,
            steps: self.steps(id)?,
            logs,
            events,
        })
    }

    pub fn retry(&self, id: &RunId) -> Result<RunHandleDto, AppError> {
        let original = self.get(id)?;
        if original.status.is_unfinished() {
            return Err(AppError::new(AppErrorCode::Conflict, RUN_NOT_FINISHED));
        }
        if original.kind == RunKind::Chat {
            return Err(AppError::new(AppErrorCode::Conflict, CHAT_RETRY_REFUSED));
        }
        self.start(StartRunInput {
            kind: original.kind,
            subject_id: original.subject_id,
            title: original.title,
            retry_of_id: Some(original.id),
            graph: original.graph,
            input: Some(original.input),
            concurrency: Some(original.concurrency),
        })
    }

    pub fn clear_finished(&self) -> Result<RunsClearFinishedOutput, AppError> {
        let removed = self.inner.database.transaction(|transaction| {
            let runs = Repositories::new(transaction).runs;
            let referenced = runs.referenced()?;
            Ok(runs.remove_finished(&referenced)?)
        })?;
        Ok(RunsClearFinishedOutput {
            removed: removed as u64,
        })
    }

    fn counts(&self) -> Result<RunCountsDto, AppError> {
        let raw = self
            .inner
            .database
            .with_connection(|connection| Ok(Repositories::new(connection).runs.counts()?))?;
        let mut by_status: BTreeMap<RunStatus, u64> =
            RunStatus::ALL.iter().map(|status| (*status, 0)).collect();
        let mut by_kind: BTreeMap<RunKind, u64> =
            RunKind::ALL.iter().map(|kind| (*kind, 0)).collect();
        let mut total = 0;
        for (status, count) in raw.by_status {
            let count = u64::try_from(count).unwrap_or(0);
            by_status.insert(status, count);
            total += count;
        }
        for (kind, count) in raw.by_kind {
            by_kind.insert(kind, u64::try_from(count).unwrap_or(0));
        }
        Ok(RunCountsDto {
            total,
            by_status,
            by_kind,
        })
    }

    fn summaries(&self, rows: &[RunRow]) -> Result<Vec<RunSummaryDto>, AppError> {
        let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        let (succeeded, running, approvals) =
            self.inner.database.with_connection(|connection| {
                let repositories = Repositories::new(connection);
                Ok((
                    repositories.runs.succeeded_counts(&ids)?,
                    repositories.runs.running_nodes(&ids)?,
                    repositories.permissions.pending_for_runs(&ids)?,
                ))
            })?;
        let succeeded: HashMap<String, i64> = succeeded.into_iter().collect();
        let mut active: HashMap<String, String> = HashMap::new();
        for node in running {
            active.entry(node.run_id).or_insert(node.node_id);
        }
        let mut pending: HashMap<String, RunApprovalDto> = HashMap::new();
        for approval in approvals {
            pending
                .entry(approval.run_id.clone())
                .or_insert(RunApprovalDto {
                    id: approval.id,
                    subject: approval.subject,
                    scope: approval.scope,
                    expires_at: approval.expires_at,
                });
        }
        rows.iter()
            .map(|row| {
                let total = row.graph.nodes.len() as u64;
                let done = u64::try_from(succeeded.get(&row.id).copied().unwrap_or(0)).unwrap_or(0);
                Ok(RunSummaryDto {
                    id: RunId::parse(&row.id).ok_or_else(|| malformed("id"))?,
                    stream_id: StreamId::parse(&row.stream_id)
                        .ok_or_else(|| malformed("streamId"))?,
                    kind: row.kind,
                    subject_id: row.subject_id.clone(),
                    title: row
                        .title
                        .clone()
                        .or_else(|| row.subject_id.clone())
                        .unwrap_or_else(|| row.id.clone()),
                    status: row.status,
                    progress: RunProgressDto {
                        done: done.min(total),
                        total,
                    },
                    active_node_id: active.get(&row.id).cloned(),
                    approval: pending.get(&row.id).cloned(),
                    outcome: row.outcome.clone(),
                    error: row.error.clone(),
                    retry_of_id: row.retry_of_id.as_deref().and_then(RunId::parse),
                    created_at: row.created_at,
                    started_at: row.started_at,
                    finished_at: row.finished_at,
                    pruned_at: row.pruned_at,
                })
            })
            .collect()
    }

    pub fn steps(&self, id: &RunId) -> Result<Vec<StepDto>, AppError> {
        let id = id.to_string();
        read_run(&self.inner.database, &id)?;
        self.inner
            .database
            .with_connection(|connection| Ok(Repositories::new(connection).runs.steps(&id)?))?
            .into_iter()
            .map(step_dto)
            .collect()
    }

    pub fn step_rows(&self, id: &str) -> Result<Vec<StepRow>, AppError> {
        read_run(&self.inner.database, id)?;
        self.inner
            .database
            .with_connection(|connection| Ok(Repositories::new(connection).runs.steps(id)?))
    }

    pub fn recover(&self) -> Result<(), AppError> {
        if self.inner.closed.load(Ordering::SeqCst) {
            return Ok(());
        }
        let unfinished = self
            .inner
            .database
            .with_connection(|connection| Ok(Repositories::new(connection).runs.unfinished()?))?;
        for run in unfinished {
            if self.active().contains_key(&run.id) {
                continue;
            }
            if let Err(error) = self.recover_run(run.clone()) {
                self.inner.logger.log(
                    LogLevel::Error,
                    "kernel",
                    "Could not recover a run",
                    &[("runId", json!(run.id)), ("error", json!(error.message))],
                );
            }
        }
        Ok(())
    }

    fn recover_run(&self, run: RunRow) -> Result<(), AppError> {
        let stream = self.open_stream(&run)?;
        let finished_at = self.inner.clock.now();
        let steps = self.inner.database.with_connection(|connection| {
            Ok(Repositories::new(connection).runs.steps(&run.id)?)
        })?;
        let abandoned: Vec<StepRow> = steps
            .into_iter()
            .filter(|step| step.status == StepStatus::Running)
            .map(|step| StepRow {
                status: StepStatus::Abandoned,
                finished_at: Some(finished_at),
                error: Some(ABANDONED_ON_EXIT.to_owned()),
                ..step
            })
            .collect();
        stream.commit(
            abandoned
                .iter()
                .map(|step| HostEventDraft::Step {
                    step: step_event(step),
                })
                .collect(),
            |repositories| {
                for step in &abandoned {
                    repositories.runs.finish_step(
                        &step.id,
                        StepStatus::Abandoned,
                        ABANDONED_ON_EXIT,
                        finished_at,
                    )?;
                }
                Ok(())
            },
        )?;
        let pending = self.inner.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .permissions
                .approvals_for_run(&run.id)?)
        })?;
        if pending.iter().any(|row| row.decision.is_none())
            || (run.status == RunStatus::Blocked && !pending.is_empty())
        {
            self.inner.database.with_connection(|connection| {
                Repositories::new(connection)
                    .permissions
                    .deny_unfinished(&run.id)?;
                Ok(())
            })?;
            return Scheduler::new(run, stream, Arc::clone(&self.inner.options))?.finish(
                Finish::Failed,
                Some(APPROVAL_INTERRUPTED.to_owned()),
                Some(AppErrorCode::ApprovalDenied),
            );
        }
        if self.inner.registry.can_resume(&run.graph) {
            stream.emit(HostEventDraft::Log {
                line: log_line(&[
                    ("runId", json!(run.id)),
                    ("message", json!(RESUMING_MESSAGE)),
                ]),
            })?;
            self.launch(run, stream)
        } else {
            Scheduler::new(run, stream, Arc::clone(&self.inner.options))?.finish(
                Finish::Interrupted,
                Some(NOT_RESUMABLE.to_owned()),
                None,
            )
        }
    }

    pub async fn dispose(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);
        let active: Vec<(Arc<Scheduler>, Done)> = self
            .active()
            .values()
            .map(|active| (Arc::clone(&active.scheduler), active.done.clone()))
            .collect();
        for (scheduler, _) in &active {
            scheduler.suspend();
        }
        join_all(active.into_iter().map(|(_, done)| done)).await;
        self.inner.observers.clear();
    }

    pub fn observers(&self, id: &str) -> usize {
        self.inner.observers.count(id)
    }

    pub fn record(&self, id: &str) -> Result<Vec<HostEvent>, AppError> {
        self.inner
            .database
            .with_connection(|connection| Ok(Repositories::new(connection).runs.events(id)?))
    }
}

#[cfg(test)]
mod tests;
