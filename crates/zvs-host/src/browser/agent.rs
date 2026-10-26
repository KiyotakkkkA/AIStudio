use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use async_trait::async_trait;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, BrowserActionState, BrowserAgentAction, BrowserAgentApproval,
    BrowserAgentState, BrowserAgentStatus, BrowserAuthorisation, BrowserGrantDto, BrowserSiteGrant,
    RunId,
};

use super::capability::BrowserCapability;
use super::grants::BrowserGrants;
use crate::kernel::types::cancelled;
use crate::platform::clock::SharedClock;
use crate::services::{RunService, SettingService};

pub const RATE_LIMIT_KEY: &str = "browser.agent.actionsPerMinute";
pub const DEFAULT_RATE_LIMIT: u64 = 20;
pub const ACTIONS_KEPT: usize = 200;
pub const RATE_WINDOW_MS: i64 = 60_000;

pub const NO_SESSION: &str = "Сеанс агента не запущен";
pub const NOT_PENDING: &str = "Подтверждение уже не ожидается";
pub const ALWAYS_REFUSED: &str =
    "Это действие подтверждается каждый раз и не разрешается для сайта целиком";
pub const RATE_PAUSED: &str = "Превышен предел действий в минуту";
pub const GRANT_NOT_FOUND: &str = "Разрешение не найдено";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[async_trait]
pub trait AgentRuns: Send + Sync {
    fn title(&self, run_id: &str) -> Option<String>;

    fn approve(&self, run_id: &str, approval_id: &str) -> Result<(), AppError>;

    fn deny(&self, run_id: &str, approval_id: &str) -> Result<(), AppError>;

    async fn cancel(&self, run_id: &str) -> Result<(), AppError>;

    async fn wait(&self, run_id: &str);
}

fn run_id(id: &str) -> Result<RunId, AppError> {
    RunId::parse(id).ok_or_else(|| AppError::new(AppErrorCode::NotFound, NO_SESSION))
}

#[async_trait]
impl AgentRuns for RunService {
    fn title(&self, id: &str) -> Option<String> {
        let run = self.get(&RunId::parse(id)?).ok()?;
        run.title.filter(|title| !title.trim().is_empty())
    }

    fn approve(&self, id: &str, approval_id: &str) -> Result<(), AppError> {
        Self::approve(self, &run_id(id)?, approval_id, false)
    }

    fn deny(&self, id: &str, approval_id: &str) -> Result<(), AppError> {
        Self::deny(self, &run_id(id)?, approval_id)
    }

    async fn cancel(&self, id: &str) -> Result<(), AppError> {
        Self::cancel(self, &run_id(id)?).await
    }

    async fn wait(&self, id: &str) {
        if let Ok(id) = run_id(id) {
            Self::wait(self, &id).await;
        }
    }
}

pub type RateLimit = Arc<dyn Fn() -> u64 + Send + Sync>;

pub fn setting_limit(settings: SettingService) -> RateLimit {
    Arc::new(move || {
        settings
            .get(RATE_LIMIT_KEY)
            .ok()
            .flatten()
            .and_then(|setting| setting.value.as_f64())
            .filter(|limit| limit.is_finite() && *limit >= 1.0)
            .map_or(DEFAULT_RATE_LIMIT, |limit| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let limit = limit.floor() as u64;
                limit
            })
    })
}

#[derive(Clone)]
pub struct AgentControls {
    pub runs: Arc<dyn AgentRuns>,
    pub grants: BrowserGrants,
    pub limit: RateLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentChange {
    Started,
    Updated,
}

type Listener = Arc<dyn Fn(AgentChange) + Send + Sync>;

#[derive(Default)]
struct State {
    status: BrowserAgentStatus,
    run_id: Option<String>,
    task: Option<String>,
    steps: u64,
    window: VecDeque<i64>,
    target: Option<String>,
    approval: Option<BrowserAgentApproval>,
    actions: VecDeque<BrowserAgentAction>,
    next: u64,
}

impl State {
    fn action(&mut self, id: u64) -> Option<&mut BrowserAgentAction> {
        self.actions.iter_mut().find(|action| action.id == id)
    }

    fn push(&mut self, action: BrowserAgentAction) {
        self.actions.push_back(action);
        while self.actions.len() > ACTIONS_KEPT {
            self.actions.pop_front();
        }
    }

    fn prune(&mut self, now: i64) {
        while self
            .window
            .front()
            .is_some_and(|at| now - *at >= RATE_WINDOW_MS)
        {
            self.window.pop_front();
        }
    }

    const fn active(&self) -> bool {
        matches!(
            self.status,
            BrowserAgentStatus::Running
                | BrowserAgentStatus::Approval
                | BrowserAgentStatus::Paused
                | BrowserAgentStatus::Control
        )
    }
}

struct Inner {
    clock: SharedClock,
    state: Mutex<State>,
    revision: watch::Sender<u64>,
    controls: OnceLock<AgentControls>,
    listeners: Mutex<Vec<Listener>>,
}

#[derive(Clone)]
pub struct BrowserAgent(Arc<Inner>);

impl BrowserAgent {
    pub fn new(clock: SharedClock) -> Self {
        Self(Arc::new(Inner {
            clock,
            state: Mutex::default(),
            revision: watch::channel(0).0,
            controls: OnceLock::new(),
            listeners: Mutex::default(),
        }))
    }

    pub fn attach(&self, controls: AgentControls) {
        let _ = self.0.controls.set(controls);
    }

    pub fn subscribe(&self, listener: impl Fn(AgentChange) + Send + Sync + 'static) {
        lock(&self.0.listeners).push(Arc::new(listener));
    }

    fn controls(&self) -> Result<&AgentControls, AppError> {
        self.0
            .controls
            .get()
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, NO_SESSION))
    }

    pub fn limit(&self) -> u64 {
        self.0
            .controls
            .get()
            .map_or(DEFAULT_RATE_LIMIT, |controls| (controls.limit)().max(1))
    }

    fn changed(&self, change: AgentChange) {
        self.0.revision.send_modify(|revision| *revision += 1);
        let listeners: Vec<Listener> = lock(&self.0.listeners).iter().cloned().collect();
        for listener in listeners {
            listener(change);
        }
    }

    pub fn snapshot(&self, origin: Option<&str>) -> BrowserAgentState {
        let now = self.0.clock.now();
        let limit = self.limit();
        let mut state = lock(&self.0.state);
        state.prune(now);
        let grants = match (origin, self.0.controls.get()) {
            (Some(origin), Some(controls)) => controls
                .grants
                .list()
                .unwrap_or_default()
                .into_iter()
                .filter(|grant| grant.origin == origin)
                .map(|grant| BrowserSiteGrant {
                    id: grant.id,
                    tool: grant.capability.key().to_owned(),
                    granted_at: grant.granted_at,
                })
                .collect(),
            _ => Vec::new(),
        };
        BrowserAgentState {
            status: state.status,
            run_id: state.run_id.clone(),
            task: state.task.clone(),
            steps: state.steps,
            pace: u64::try_from(state.window.len()).unwrap_or(u64::MAX),
            limit,
            target: state.target.clone(),
            approval: state.approval.clone(),
            actions: state.actions.iter().cloned().collect(),
            origin: origin.map(str::to_owned),
            grants,
        }
    }

    fn open(&self, run_id: &str) {
        let fresh = {
            let mut state = lock(&self.0.state);
            if state.run_id.as_deref() == Some(run_id) {
                false
            } else {
                let held = matches!(
                    state.status,
                    BrowserAgentStatus::Paused | BrowserAgentStatus::Control
                );
                *state = State {
                    status: if held {
                        state.status
                    } else {
                        BrowserAgentStatus::Running
                    },
                    run_id: Some(run_id.to_owned()),
                    window: std::mem::take(&mut state.window),
                    task: self
                        .0
                        .controls
                        .get()
                        .and_then(|controls| controls.runs.title(run_id)),
                    next: state.next,
                    ..State::default()
                };
                true
            }
        };
        if !fresh {
            return;
        }
        if let Some(controls) = self.0.controls.get() {
            let runs = Arc::clone(&controls.runs);
            let agent = self.clone();
            let id = run_id.to_owned();
            tokio::spawn(async move {
                runs.wait(&id).await;
                agent.ended(&id);
            });
        }
        self.changed(AgentChange::Started);
    }

    fn ended(&self, run_id: &str) {
        {
            let mut state = lock(&self.0.state);
            if state.run_id.as_deref() != Some(run_id) {
                return;
            }
            if matches!(
                state.status,
                BrowserAgentStatus::Running | BrowserAgentStatus::Approval
            ) {
                state.status = BrowserAgentStatus::Finished;
            }
            state.approval = None;
            state.target = None;
        }
        self.changed(AgentChange::Updated);
    }

    pub async fn begin(
        &self,
        run_id: &str,
        cancel: &CancellationToken,
        tool: &str,
        target: String,
    ) -> Result<u64, AppError> {
        self.open(run_id);
        let mut revisions = self.0.revision.subscribe();
        loop {
            if cancel.is_cancelled() {
                return Err(cancelled());
            }
            let now = self.0.clock.now();
            let limit = self.limit();
            let outcome = {
                let mut state = lock(&self.0.state);
                if state.run_id.as_deref() != Some(run_id) {
                    return Err(cancelled());
                }
                match state.status {
                    BrowserAgentStatus::Paused | BrowserAgentStatus::Control => None,
                    BrowserAgentStatus::Stopped | BrowserAgentStatus::Finished => {
                        return Err(cancelled());
                    }
                    _ => {
                        state.prune(now);
                        let id = state.next;
                        state.next += 1;
                        if u64::try_from(state.window.len()).unwrap_or(u64::MAX) >= limit {
                            state.status = BrowserAgentStatus::Paused;
                            state.push(BrowserAgentAction {
                                id,
                                tool: tool.to_owned(),
                                target: target.clone(),
                                state: BrowserActionState::Paused,
                                authorisation: None,
                                started_at: now,
                                duration_ms: None,
                                error: Some(RATE_PAUSED.to_owned()),
                            });
                            Some(None)
                        } else {
                            state.window.push_back(now);
                            state.steps += 1;
                            state.target = Some(target.clone());
                            state.push(BrowserAgentAction {
                                id,
                                tool: tool.to_owned(),
                                target: target.clone(),
                                state: BrowserActionState::Running,
                                authorisation: None,
                                started_at: now,
                                duration_ms: None,
                                error: None,
                            });
                            Some(Some(id))
                        }
                    }
                }
            };
            match outcome {
                Some(Some(id)) => {
                    self.changed(AgentChange::Updated);
                    return Ok(id);
                }
                Some(None) => {
                    self.changed(AgentChange::Updated);
                    let _ = revisions.borrow_and_update();
                }
                None => {}
            }
            tokio::select! {
                changed = revisions.changed() => {
                    if changed.is_err() {
                        return Err(cancelled());
                    }
                }
                () = cancel.cancelled() => return Err(cancelled()),
            }
        }
    }

    pub fn authorised(&self, action: u64, authorisation: BrowserAuthorisation) {
        if let Some(entry) = lock(&self.0.state).action(action) {
            entry.authorisation = Some(authorisation);
        }
        self.changed(AgentChange::Updated);
    }

    pub fn awaiting(&self, action: u64, approval: BrowserAgentApproval) {
        {
            let mut state = lock(&self.0.state);
            if let Some(entry) = state.action(action) {
                entry.state = BrowserActionState::Waiting;
            }
            if state.status == BrowserAgentStatus::Running {
                state.status = BrowserAgentStatus::Approval;
            }
            state.approval = Some(approval);
        }
        self.changed(AgentChange::Updated);
    }

    pub fn decided(&self, action: u64, approved: bool) {
        let now = self.0.clock.now();
        {
            let mut state = lock(&self.0.state);
            if let Some(entry) = state.action(action) {
                if approved {
                    entry.state = BrowserActionState::Running;
                    entry.authorisation = Some(BrowserAuthorisation::Approved { at: now });
                } else {
                    entry.state = BrowserActionState::Denied;
                }
            }
            state.approval = None;
            if state.status == BrowserAgentStatus::Approval {
                state.status = BrowserAgentStatus::Running;
            }
        }
        self.changed(AgentChange::Updated);
    }

    pub fn finish(&self, action: u64, error: Option<&str>) {
        let now = self.0.clock.now();
        {
            let mut state = lock(&self.0.state);
            if let Some(entry) = state.action(action) {
                entry.duration_ms = u64::try_from(now - entry.started_at).ok();
                if entry.state != BrowserActionState::Denied {
                    entry.state = if error.is_some() {
                        BrowserActionState::Failed
                    } else {
                        BrowserActionState::Done
                    };
                    entry.error = error.map(str::to_owned);
                }
            }
            state.target = None;
        }
        self.changed(AgentChange::Updated);
    }

    fn pending(&self, approval_id: &str) -> Result<(String, BrowserAgentApproval), AppError> {
        let state = lock(&self.0.state);
        match (&state.run_id, &state.approval) {
            (Some(run_id), Some(approval)) if approval.id == approval_id => {
                Ok((run_id.clone(), approval.clone()))
            }
            _ => Err(AppError::new(AppErrorCode::Conflict, NOT_PENDING)),
        }
    }

    pub fn approve(&self, approval_id: &str) -> Result<(), AppError> {
        let (run_id, _) = self.pending(approval_id)?;
        self.controls()?.runs.approve(&run_id, approval_id)
    }

    pub fn always(&self, approval_id: &str) -> Result<(), AppError> {
        let (run_id, approval) = self.pending(approval_id)?;
        let capability = BrowserCapability::from_key(&approval.tool)
            .filter(|capability| approval.can_always && capability.grantable())
            .ok_or_else(|| AppError::new(AppErrorCode::PermissionDenied, ALWAYS_REFUSED))?;
        let controls = self.controls()?;
        controls.grants.grant(&approval.origin, capability)?;
        controls.runs.approve(&run_id, approval_id)
    }

    pub fn deny(&self, approval_id: &str) -> Result<(), AppError> {
        let (run_id, _) = self.pending(approval_id)?;
        self.controls()?.runs.deny(&run_id, approval_id)
    }

    pub fn take_control(&self) -> Result<(), AppError> {
        {
            let mut state = lock(&self.0.state);
            if !state.active() {
                return Err(AppError::new(AppErrorCode::Conflict, NO_SESSION));
            }
            state.status = BrowserAgentStatus::Control;
        }
        self.changed(AgentChange::Updated);
        Ok(())
    }

    pub fn resume(&self) -> Result<(), AppError> {
        {
            let mut state = lock(&self.0.state);
            if !matches!(
                state.status,
                BrowserAgentStatus::Paused | BrowserAgentStatus::Control
            ) {
                return Err(AppError::new(AppErrorCode::Conflict, NO_SESSION));
            }
            state.status = if state.approval.is_some() {
                BrowserAgentStatus::Approval
            } else {
                BrowserAgentStatus::Running
            };
            state.window.clear();
        }
        self.changed(AgentChange::Updated);
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), AppError> {
        let run_id = {
            let mut state = lock(&self.0.state);
            if !state.active() {
                return Err(AppError::new(AppErrorCode::Conflict, NO_SESSION));
            }
            state.status = BrowserAgentStatus::Stopped;
            state.approval = None;
            state.target = None;
            state.run_id.clone()
        };
        self.changed(AgentChange::Updated);
        match run_id {
            Some(run_id) => self.controls()?.runs.cancel(&run_id).await,
            None => Ok(()),
        }
    }

    pub fn revoke(&self, grant_id: &str) -> Result<(), AppError> {
        if !self.controls()?.grants.revoke(grant_id)? {
            return Err(AppError::new(AppErrorCode::NotFound, GRANT_NOT_FOUND));
        }
        self.changed(AgentChange::Updated);
        Ok(())
    }

    pub fn site_grants(&self) -> Result<Vec<BrowserGrantDto>, AppError> {
        let Some(controls) = self.0.controls.get() else {
            return Ok(Vec::new());
        };
        Ok(controls
            .grants
            .list()?
            .into_iter()
            .map(|grant| BrowserGrantDto {
                id: grant.id,
                origin: grant.origin,
                tool: grant.capability.key().to_owned(),
                granted_at: grant.granted_at,
            })
            .collect())
    }

    pub fn has_approval(&self) -> bool {
        lock(&self.0.state).approval.is_some()
    }

    pub fn status(&self) -> BrowserAgentStatus {
        lock(&self.0.state).status
    }
}
