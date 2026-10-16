use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::{FutureExt, StreamExt};
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, ApprovalRequestDto, GraphBinding, GraphNode, HostEventDraft, RunId,
    RunOutcomeDto, RunOutcomeStatus, RunStatus, StepEventDto, StepStatus, Timestamp, Validate,
    log_line,
};

use super::registry::NodeRegistry;
use super::stream::RunStream;
use super::types::{
    AdmissionOrigin, ApprovalDecision, KernelServices, PermissionRequirement, StepContext,
    StepLink, cancelled,
};
use crate::data::repositories::RunPatch;
use crate::data::schema::{RunRow, StepRow};
use crate::data::{DataError, Database, Repositories};
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger};

pub const ABANDONED_MESSAGE: &str = "Узел не остановился после отмены";
pub const INTERRUPTED_MESSAGE: &str = "Выполнение прервано";
pub const EXHAUSTED_MESSAGE: &str = "Попытки исчерпаны";
pub const NO_READY_NODES: &str = "Нет доступных узлов";
pub const ONLY_KERNEL_ENDS: &str = "Only the kernel can end a run";
pub const NO_PERMISSION_SERVICE: &str = "Сервис разрешений недоступен";
pub const NO_APPROVAL_SERVICE: &str = "Сервис согласования недоступен";
pub const NODE_PANICKED: &str = "Узел завершился аварийно";

pub type ApprovalHook = Arc<
    dyn Fn(
            ApprovalRequestDto,
            CancellationToken,
        ) -> BoxFuture<'static, Result<ApprovalDecision, AppError>>
        + Send
        + Sync,
>;

pub type AdmitHook = Arc<
    dyn Fn(
            PermissionRequirement,
            StepContext,
            AdmissionOrigin,
        ) -> BoxFuture<'static, Result<(), AppError>>
        + Send
        + Sync,
>;

pub struct SchedulerOptions {
    pub database: Arc<Database>,
    pub registry: Arc<NodeRegistry>,
    pub services: KernelServices,
    pub clock: SharedClock,
    pub grace: Duration,
    pub logger: SharedLogger,
    pub approval: Option<ApprovalHook>,
    pub admit: Option<AdmitHook>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

impl Finish {
    const fn status(self) -> RunStatus {
        match self {
            Self::Succeeded => RunStatus::Succeeded,
            Self::Failed => RunStatus::Failed,
            Self::Cancelled => RunStatus::Cancelled,
            Self::Interrupted => RunStatus::Interrupted,
        }
    }
}

struct State {
    pending_approvals: usize,
    completed: HashMap<String, StepRow>,
    attempts: HashMap<String, i64>,
    latest: HashMap<String, StepRow>,
    ready_at: HashMap<String, Timestamp>,
    failure: Option<String>,
    failure_code: AppErrorCode,
}

enum Outcome {
    Done(Result<Value, AppError>),
    Aborted,
    Abandoned,
}

type Active = FuturesUnordered<BoxFuture<'static, (String, Result<(), AppError>)>>;

pub fn step_event(step: &StepRow) -> StepEventDto {
    let value = json!({
        "id": step.id,
        "runId": step.run_id,
        "nodeId": step.node_id,
        "type": step.step_type,
        "status": step.status,
        "input": step.input,
        "output": step.output,
        "error": step.error,
        "startedAt": step.started_at,
        "finishedAt": step.finished_at,
        "attempt": step.attempt,
    });
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn data_error(error: rusqlite::Error) -> DataError {
    DataError::from(error)
}

pub struct Scheduler {
    run: RunRow,
    run_id: RunId,
    stream: Arc<RunStream>,
    options: Arc<SchedulerOptions>,
    cancel: CancellationToken,
    stopping: AtomicBool,
    terminal: AtomicBool,
    finishing: AtomicBool,
    state: Mutex<State>,
}

impl Scheduler {
    pub fn new(
        run: RunRow,
        stream: Arc<RunStream>,
        options: Arc<SchedulerOptions>,
    ) -> Result<Self, AppError> {
        let run_id = RunId::parse(&run.id).ok_or_else(|| {
            AppError::new(
                AppErrorCode::ValidationFailed,
                "Некорректный идентификатор запуска",
            )
        })?;
        Ok(Self {
            run,
            run_id,
            stream,
            options,
            cancel: CancellationToken::new(),
            stopping: AtomicBool::new(false),
            terminal: AtomicBool::new(false),
            finishing: AtomicBool::new(false),
            state: Mutex::new(State {
                pending_approvals: 0,
                completed: HashMap::new(),
                attempts: HashMap::new(),
                latest: HashMap::new(),
                ready_at: HashMap::new(),
                failure: None,
                failure_code: AppErrorCode::RunFailed,
            }),
        })
    }

    pub const fn token(&self) -> &CancellationToken {
        &self.cancel
    }

    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub fn suspend(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        self.cancel.cancel();
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn now(&self) -> Timestamp {
        self.options.clock.now()
    }

    fn live_drafts(&self, drafts: Vec<HostEventDraft>) -> Vec<HostEventDraft> {
        if self.terminal.load(Ordering::SeqCst) {
            Vec::new()
        } else {
            drafts
        }
    }

    fn emit(&self, draft: HostEventDraft) -> Result<(), AppError> {
        if self.terminal.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.stream.emit(draft)
    }

    fn status_line(&self, status: RunStatus) -> HostEventDraft {
        HostEventDraft::Log {
            line: log_line(&[("runId", json!(self.run.id)), ("status", json!(status))]),
        }
    }

    fn set_status(&self, status: RunStatus) -> Result<(), AppError> {
        let id = self.run.id.clone();
        self.stream.commit(
            self.live_drafts(vec![self.status_line(status)]),
            |repositories| {
                repositories
                    .runs
                    .update(
                        &id,
                        &RunPatch {
                            status: Some(status),
                            ..RunPatch::default()
                        },
                    )
                    .map_err(data_error)?;
                Ok(())
            },
        )
    }

    fn progress(&self) -> HostEventDraft {
        HostEventDraft::Progress {
            done: self.state().completed.len() as u64,
            total: self.run.graph.nodes.len() as u64,
        }
    }

    pub fn finish(
        &self,
        finish: Finish,
        error: Option<String>,
        code: Option<AppErrorCode>,
    ) -> Result<(), AppError> {
        if self.terminal.load(Ordering::SeqCst) || self.finishing.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let code = code.unwrap_or_else(|| self.state().failure_code);
        let outcome = match finish {
            Finish::Succeeded => RunOutcomeDto {
                status: RunOutcomeStatus::Ok,
                code: None,
                message: None,
            },
            Finish::Cancelled => RunOutcomeDto {
                status: RunOutcomeStatus::Cancelled,
                code: Some(AppErrorCode::RunCancelled),
                message: Some(
                    error
                        .clone()
                        .unwrap_or_else(|| super::types::CANCELLED_MESSAGE.to_owned()),
                ),
            },
            Finish::Failed | Finish::Interrupted => RunOutcomeDto {
                status: RunOutcomeStatus::Failed,
                code: Some(code),
                message: Some(error.clone().unwrap_or_else(|| {
                    if finish == Finish::Interrupted {
                        INTERRUPTED_MESSAGE.to_owned()
                    } else {
                        super::types::CANCELLED_MESSAGE.to_owned()
                    }
                })),
            },
        };
        let status = finish.status();
        let id = self.run.id.clone();
        let patch = RunPatch {
            status: Some(status),
            outcome: Some(outcome.clone()),
            error: Some(error),
            finished_at: Some(self.now()),
            ..RunPatch::default()
        };
        let committed = self.stream.commit(
            vec![self.status_line(status), HostEventDraft::End { outcome }],
            |repositories| {
                repositories.runs.update(&id, &patch).map_err(data_error)?;
                Ok(())
            },
        );
        match committed {
            Ok(()) => {
                self.terminal.store(true, Ordering::SeqCst);
                Ok(())
            }
            Err(error) => {
                self.finishing.store(false, Ordering::SeqCst);
                Err(error)
            }
        }
    }

    pub async fn execute(self: Arc<Self>) -> Result<(), AppError> {
        let mut active: Active = FuturesUnordered::new();
        let mut running: HashSet<String> = HashSet::new();
        let Err(error) = self.drive(&mut active, &mut running).await else {
            return Ok(());
        };
        self.cancel.cancel();
        while active.next().await.is_some() {}
        self.options.logger.log(
            LogLevel::Error,
            "kernel",
            "Run failed",
            &[
                ("runId", json!(self.run.id)),
                ("error", json!(error.message)),
            ],
        );
        if self.stopping.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.finish(Finish::Failed, Some(error.message), None)
    }

    fn load_checkpoints(&self) -> Result<(), AppError> {
        let steps = self.options.database.with_connection(|connection| {
            Ok(Repositories::new(connection).runs.steps(&self.run.id)?)
        })?;
        let mut state = self.state();
        for step in steps {
            let Some(node) = self
                .run
                .graph
                .nodes
                .iter()
                .find(|node| node.id == step.node_id)
            else {
                continue;
            };
            if step.attempt
                > state
                    .latest
                    .get(&step.node_id)
                    .map_or(0, |latest| latest.attempt)
            {
                state.latest.insert(step.node_id.clone(), step.clone());
            }
            let attempts = state.attempts.entry(step.node_id.clone()).or_insert(0);
            *attempts = (*attempts).max(step.attempt);
            match step.status {
                StepStatus::Succeeded => {
                    state.completed.insert(step.node_id.clone(), step);
                }
                StepStatus::Failed => {
                    let due = step.finished_at.unwrap_or(0) + backoff(node) * step.attempt;
                    state.ready_at.insert(step.node_id.clone(), due);
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn drive(
        self: &Arc<Self>,
        active: &mut Active,
        running: &mut HashSet<String>,
    ) -> Result<(), AppError> {
        self.load_checkpoints()?;
        self.options.registry.validate(&self.run.graph)?;
        let started_at = self.run.started_at.unwrap_or_else(|| self.now());
        self.options.database.with_connection(|connection| {
            Repositories::new(connection).runs.update(
                &self.run.id,
                &RunPatch {
                    started_at: Some(started_at),
                    ..RunPatch::default()
                },
            )?;
            Ok(())
        })?;
        self.set_status(RunStatus::Running)?;
        self.emit(self.progress())?;
        let mut nodes = self.run.graph.nodes.clone();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        let total = nodes.len();
        let concurrency = usize::try_from(self.run.concurrency).unwrap_or(1).max(1);
        while !self.cancel.is_cancelled() && self.state().completed.len() < total {
            let mut next: Option<Timestamp> = None;
            for node in &nodes {
                let mut state = self.state();
                if state.pending_approvals > 0 || running.len() >= concurrency {
                    break;
                }
                if state.completed.contains_key(&node.id)
                    || running.contains(&node.id)
                    || !node
                        .dependencies()
                        .iter()
                        .all(|id| state.completed.contains_key(id))
                {
                    continue;
                }
                if let Some(previous) = state.latest.get(&node.id)
                    && previous.status == StepStatus::Failed
                    && previous.attempt >= max_attempts(node)
                {
                    state.failure = Some(
                        previous
                            .error
                            .clone()
                            .unwrap_or_else(|| EXHAUSTED_MESSAGE.to_owned()),
                    );
                    drop(state);
                    self.cancel.cancel();
                    break;
                }
                let due = state.ready_at.get(&node.id).copied().unwrap_or(0);
                drop(state);
                if due > self.now() {
                    next = Some(next.map_or(due, |current| current.min(due)));
                    continue;
                }
                running.insert(node.id.clone());
                active.push(Arc::clone(self).execute_node(node.clone()));
            }
            if self.cancel.is_cancelled() {
                break;
            }
            if !active.is_empty() {
                if let Some(due) = next {
                    tokio::select! {
                        biased;
                        done = active.next() => settle(done, running)?,
                        () = self.pause(due) => {}
                    }
                } else {
                    settle(active.next().await, running)?;
                }
            } else if let Some(due) = next {
                self.pause(due).await;
            } else if self.state().completed.len() < total {
                return Err(AppError::new(AppErrorCode::Unknown, NO_READY_NODES));
            }
        }
        while let Some(done) = active.next().await {
            settle(Some(done), running)?;
        }
        if self.stopping.load(Ordering::SeqCst) {
            return Ok(());
        }
        let failure = self.state().failure.clone();
        if let Some(failure) = failure {
            self.finish(Finish::Failed, Some(failure), None)
        } else if self.cancel.is_cancelled() {
            self.finish(Finish::Cancelled, None, None)
        } else {
            self.finish(Finish::Succeeded, None, None)
        }
    }

    async fn pause(&self, due: Timestamp) {
        let wait = u64::try_from(due - self.now()).unwrap_or(0);
        tokio::select! {
            () = tokio::time::sleep(Duration::from_millis(wait)) => {}
            () = self.cancel.cancelled() => {}
        }
    }

    fn execute_node(
        self: Arc<Self>,
        node: GraphNode,
    ) -> BoxFuture<'static, (String, Result<(), AppError>)> {
        async move {
            let id = node.id.clone();
            let result = self.run_node(node).await;
            (id, result)
        }
        .boxed()
    }

    fn bind(&self, node: &GraphNode, state: &State) -> Value {
        if !node.has_bindings() {
            return node.input().clone();
        }
        let mut input = node.input().as_object().cloned().unwrap_or_default();
        for (key, binding) in node.bindings() {
            let value = match binding {
                GraphBinding::Run {} => self.run.input.clone(),
                GraphBinding::Node { node_id } => state
                    .completed
                    .get(node_id)
                    .map_or(Value::Null, |step| step.output.clone()),
            };
            input.insert(key.clone(), value);
        }
        Value::Object(input)
    }

    fn commit_step(&self, step: &StepRow, inserting: bool) -> Result<(), AppError> {
        let mut drafts = vec![HostEventDraft::Step {
            step: step_event(step),
        }];
        if !inserting {
            drafts.push(self.progress());
        }
        self.stream
            .commit(self.live_drafts(drafts), |repositories| {
                if inserting {
                    repositories.runs.add_step(step).map_err(data_error)?;
                } else {
                    repositories.runs.update_step(step).map_err(data_error)?;
                }
                Ok(())
            })
    }

    async fn run_node(self: Arc<Self>, node: GraphNode) -> Result<(), AppError> {
        let (attempt, input) = {
            let mut state = self.state();
            let attempt = state.attempts.get(&node.id).copied().unwrap_or(0) + 1;
            state.attempts.insert(node.id.clone(), attempt);
            (attempt, self.bind(&node, &state))
        };
        let mut step = StepRow {
            id: new_id().hyphenated().to_string(),
            run_id: self.run.id.clone(),
            node_id: node.id.clone(),
            step_type: node.node_type.clone(),
            status: StepStatus::Running,
            input: input.clone(),
            output: Value::Null,
            error: None,
            started_at: self.now(),
            finished_at: None,
            attempt,
        };
        self.commit_step(&step, true)?;
        let live = Arc::new(AtomicBool::new(true));
        let context = StepContext::new(
            self.run_id,
            self.cancel.clone(),
            self.options.services.clone(),
            Arc::new(NodeLink {
                scheduler: Arc::clone(&self),
                live: Arc::clone(&live),
            }),
        );
        let registry = Arc::clone(&self.options.registry);
        let admit = self.options.admit.clone();
        let node_type = node.node_type.clone();
        let mut work = tokio::spawn(async move {
            context.check_cancelled()?;
            let definition = registry.get(&node_type)?;
            if let Some(admit) = admit {
                admit(
                    definition.permission().clone(),
                    context.clone(),
                    AdmissionOrigin::Graph,
                )
                .await?;
            } else if definition.permission().tool_key().is_some() {
                return Err(AppError::new(
                    AppErrorCode::PermissionDenied,
                    NO_PERMISSION_SERVICE,
                ));
            }
            context.check_cancelled()?;
            definition.execute(&context, input).await
        });
        let joined = |result: Result<Result<Value, AppError>, tokio::task::JoinError>| {
            result.unwrap_or_else(|_| Err(AppError::new(AppErrorCode::Unknown, NODE_PANICKED)))
        };
        let outcome = tokio::select! {
            biased;
            result = &mut work => Outcome::Done(joined(result)),
            () = self.cancel.cancelled() => Outcome::Aborted,
        };
        let outcome = match outcome {
            Outcome::Aborted => match tokio::time::timeout(self.options.grace, &mut work).await {
                Ok(result) => Outcome::Done(joined(result)),
                Err(_) => Outcome::Abandoned,
            },
            other => other,
        };
        live.store(false, Ordering::SeqCst);
        step.finished_at = Some(self.now());
        match outcome {
            Outcome::Abandoned => {
                step.status = StepStatus::Abandoned;
                step.error = Some(ABANDONED_MESSAGE.to_owned());
                self.options.logger.log(
                    LogLevel::Error,
                    "kernel",
                    "Abandoned node ignored cancellation",
                    &[
                        ("runId", json!(self.run.id)),
                        ("nodeId", json!(node.id)),
                        ("attempt", json!(attempt)),
                    ],
                );
                self.emit(HostEventDraft::Log {
                    line: log_line(&[
                        ("level", json!("error")),
                        ("message", json!(ABANDONED_MESSAGE)),
                        ("nodeId", json!(node.id)),
                    ]),
                })?;
            }
            _ if self.cancel.is_cancelled() => {
                step.status = StepStatus::Cancelled;
                step.error = Some(super::types::CANCELLED_MESSAGE.to_owned());
            }
            Outcome::Done(Err(error)) => {
                step.status = StepStatus::Failed;
                step.error = Some(error.message.clone());
                let now = self.now();
                let mut state = self.state();
                state
                    .ready_at
                    .insert(node.id.clone(), now + backoff(&node) * attempt);
                let permission_failure = matches!(
                    error.code,
                    AppErrorCode::ApprovalDenied | AppErrorCode::PermissionDenied
                );
                if permission_failure {
                    state.failure_code = error.code;
                }
                if permission_failure || attempt >= max_attempts(&node) {
                    state.failure = Some(error.message);
                    drop(state);
                    self.cancel.cancel();
                }
            }
            Outcome::Done(Ok(output)) => {
                step.status = StepStatus::Succeeded;
                step.output = output;
                self.state().completed.insert(node.id.clone(), step.clone());
            }
            Outcome::Aborted => unreachable!("an aborted wait resolves to done or abandoned"),
        }
        self.state().latest.insert(node.id.clone(), step.clone());
        self.commit_step(&step, false)
    }

    fn approval_finished(&self, live: &AtomicBool) -> Result<(), AppError> {
        let remaining = {
            let mut state = self.state();
            state.pending_approvals = state.pending_approvals.saturating_sub(1);
            state.pending_approvals
        };
        if live.load(Ordering::SeqCst) && !self.cancel.is_cancelled() && remaining == 0 {
            self.set_status(RunStatus::Running)?;
        }
        Ok(())
    }
}

fn settle(
    done: Option<(String, Result<(), AppError>)>,
    running: &mut HashSet<String>,
) -> Result<(), AppError> {
    let Some((id, result)) = done else {
        return Ok(());
    };
    running.remove(&id);
    result
}

fn backoff(node: &GraphNode) -> i64 {
    i64::try_from(node.retry().backoff_ms).unwrap_or(i64::MAX)
}

fn max_attempts(node: &GraphNode) -> i64 {
    i64::try_from(node.retry().max_attempts).unwrap_or(i64::MAX)
}

struct NodeLink {
    scheduler: Arc<Scheduler>,
    live: Arc<AtomicBool>,
}

impl StepLink for NodeLink {
    fn emit(&self, event: HostEventDraft) -> Result<(), AppError> {
        if !self.live.load(Ordering::SeqCst) || self.scheduler.cancel.is_cancelled() {
            return Ok(());
        }
        if matches!(event, HostEventDraft::End { .. }) {
            return Err(AppError::new(AppErrorCode::Unknown, ONLY_KERNEL_ENDS));
        }
        event.validate()?;
        self.scheduler.emit(event)
    }

    fn request_approval(
        &self,
        request: ApprovalRequestDto,
    ) -> BoxFuture<'static, Result<ApprovalDecision, AppError>> {
        let scheduler = Arc::clone(&self.scheduler);
        let live = Arc::clone(&self.live);
        async move {
            if !live.load(Ordering::SeqCst) || scheduler.cancel.is_cancelled() {
                return Err(cancelled());
            }
            let Some(approval) = scheduler.options.approval.clone() else {
                return Err(AppError::new(
                    AppErrorCode::ApprovalDenied,
                    NO_APPROVAL_SERVICE,
                ));
            };
            scheduler.state().pending_approvals += 1;
            let decision = async {
                scheduler.set_status(RunStatus::Blocked)?;
                scheduler.emit(HostEventDraft::Approval {
                    request: request.clone(),
                })?;
                approval(request, scheduler.cancel.clone()).await
            }
            .await;
            scheduler.approval_finished(&live)?;
            decision
        }
        .boxed()
    }
}
