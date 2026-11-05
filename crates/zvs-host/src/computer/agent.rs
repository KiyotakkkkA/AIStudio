use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use zvs_contract::{
    APPS_APPROVAL_KIND, AppError, AppErrorCode, AppGrantDto, ComputerAction, ComputerActionState,
    ComputerAppDto, ComputerApproval, ComputerDisplay, ComputerFeed, ComputerState, ComputerStatus,
    FolderAccessAnswer, FolderGrantChatDto, FolderGrantScope, HostEvent, RunId,
};

use super::progress::{Absorbed, absorb, approval_id, approval_kind, approval_run, derived_status};
use super::sidecar::SharedDesktop;
use crate::browser::AgentRuns;
use crate::data::schema::AppGrantRow;
use crate::data::{DataError, Database, Repositories};
use crate::kernel::Subscription;
use crate::platform::clock::SharedClock;
use crate::platform::logger::{LogLevel, SharedLogger};
use crate::workspace::FolderAccessService;

pub const ACTIONS_KEPT: usize = 60;
pub const ACTIONS_PER_MINUTE: usize = 120;
pub const RATE_WINDOW_MS: i64 = 60_000;
pub const TOKEN_NOTIFY_MS: i64 = 80;

pub const BUSY: &str = "Компьютер уже управляется другой сессией.";
pub const NO_SESSION: &str = "Управление компьютером не запущено";
pub const NOT_PENDING: &str = "Сеанс подтверждения истёк.";
pub const RATE_LIMITED: &str = "Превышен предел действий на компьютере в минуту";
pub const GRANT_NOT_FOUND: &str = "Разрешение не найдено";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub trait ComputerChat: Send + Sync {
    fn send(&self, conversation_id: &str, text: &str) -> Result<String, AppError>;
}

pub trait ComputerFolders: Send + Sync {
    fn answer(&self, run: &str, approval: &str, answer: FolderAccessAnswer)
    -> Result<(), AppError>;
}

impl ComputerFolders for FolderAccessService {
    fn answer(
        &self,
        run: &str,
        approval: &str,
        answer: FolderAccessAnswer,
    ) -> Result<(), AppError> {
        let run =
            RunId::parse(run).ok_or_else(|| AppError::new(AppErrorCode::NotFound, NO_SESSION))?;
        Self::answer(self, &run, approval, answer)
    }
}

#[derive(Clone)]
pub struct ComputerControls {
    pub runs: Arc<dyn AgentRuns>,
    pub chat: Arc<dyn ComputerChat>,
    pub folders: Option<Arc<dyn ComputerFolders>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputerChange {
    Started,
    Updated,
    Ended,
}

type Listener = Arc<dyn Fn(ComputerChange) + Send + Sync>;

#[derive(Default)]
struct Session {
    conversation: Option<String>,
    run: Option<String>,
    title: Option<String>,
    status: ComputerStatus,
    display: Option<ComputerDisplay>,
    actions: VecDeque<ComputerAction>,
    approval: Option<ComputerApproval>,
    collapsed: bool,
    window: VecDeque<i64>,
    next: u64,
    handoff: bool,
    started_at: Option<i64>,
    feed: ComputerFeed,
    seq: Option<u64>,
    held: Option<Vec<HostEvent>>,
    token_notified: i64,
}

struct PendingAccess {
    run: String,
    conversation: String,
    apps: Vec<ComputerAppDto>,
    answer: Option<FolderAccessAnswer>,
}

struct Inner {
    clock: SharedClock,
    logger: SharedLogger,
    desktop: SharedDesktop,
    database: Arc<Database>,
    session: Mutex<Session>,
    grants: Mutex<HashMap<String, BTreeSet<String>>>,
    pending: Mutex<HashMap<String, PendingAccess>>,
    listeners: Mutex<Vec<Listener>>,
    controls: OnceLock<ComputerControls>,
    observing: Mutex<Option<Subscription>>,
}

#[derive(Clone)]
pub struct ComputerAgent(Arc<Inner>);

fn apply(session: &mut Session, event: &HostEvent) -> Absorbed {
    if session.seq.is_some_and(|seen| event.seq <= seen) {
        return Absorbed::Nothing;
    }
    session.seq = Some(event.seq);
    absorb(&mut session.feed, &event.body)
}

impl ComputerAgent {
    pub fn new(
        desktop: SharedDesktop,
        database: Arc<Database>,
        clock: SharedClock,
        logger: SharedLogger,
    ) -> Self {
        Self(Arc::new(Inner {
            clock,
            logger,
            desktop,
            database,
            session: Mutex::new(Session::default()),
            grants: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            listeners: Mutex::new(Vec::new()),
            controls: OnceLock::new(),
            observing: Mutex::new(None),
        }))
    }

    pub fn attach(&self, controls: ComputerControls) {
        let _ = self.0.controls.set(controls);
    }

    pub fn desktop(&self) -> SharedDesktop {
        Arc::clone(&self.0.desktop)
    }

    pub fn subscribe(&self, listener: impl Fn(ComputerChange) + Send + Sync + 'static) {
        lock(&self.0.listeners).push(Arc::new(listener));
    }

    fn notify(&self, change: ComputerChange) {
        let listeners = lock(&self.0.listeners).clone();
        for listener in listeners {
            listener(change);
        }
    }

    fn log(&self, message: &str, fields: &[(&str, serde_json::Value)]) {
        self.0
            .logger
            .log(LogLevel::Info, "computer", message, fields);
    }

    pub fn state(&self) -> ComputerState {
        let (conversation, run, mut feed) = {
            let session = lock(&self.0.session);
            (
                session.conversation.clone(),
                session.run.clone(),
                session.feed.clone(),
            )
        };
        if let Some(controls) = self.0.controls.get() {
            feed.approvals.retain(|request| {
                match (
                    approval_run(request).or(run.as_deref()),
                    approval_id(request),
                ) {
                    (Some(owner), Some(id)) => controls.runs.pending(owner, id),
                    _ => false,
                }
            });
        }
        let apps = conversation
            .as_deref()
            .map(|conversation| {
                self.grants(conversation, run.as_deref().unwrap_or_default())
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default();
        let session = lock(&self.0.session);
        let actions: Vec<ComputerAction> = session.actions.iter().cloned().collect();
        ComputerState {
            active: session.conversation.is_some(),
            status: derived_status(session.status, &feed, &actions, session.approval.is_some()),
            conversation_id: session.conversation.clone(),
            title: session.title.clone(),
            display: session.display,
            apps,
            actions,
            approval: session.approval.clone(),
            collapsed: session.collapsed,
            appearance: zvs_contract::BrowserAppearance::default(),
            started_at: session.started_at,
            feed,
            verifications: Vec::new(),
        }
    }

    pub fn active_conversation(&self) -> Option<String> {
        lock(&self.0.session).conversation.clone()
    }

    pub fn begin(&self, conversation: &str, run: &str) -> Result<(), AppError> {
        let (started, watch) = {
            let mut session = lock(&self.0.session);
            match session.conversation.as_deref() {
                Some(active) if active != conversation => {
                    return Err(AppError::new(AppErrorCode::Conflict, BUSY));
                }
                Some(_) => {
                    let watch = session.run.as_deref() != Some(run);
                    session.run = Some(run.to_owned());
                    session.handoff = false;
                    if session.status == ComputerStatus::Starting {
                        session.status = ComputerStatus::Working;
                    }
                    (false, watch)
                }
                None => {
                    *session = Session {
                        conversation: Some(conversation.to_owned()),
                        run: Some(run.to_owned()),
                        status: ComputerStatus::Working,
                        next: session.next,
                        ..Session::default()
                    };
                    (true, true)
                }
            }
        };
        if let Some(controls) = self.0.controls.get()
            && started
        {
            lock(&self.0.session).title = controls.runs.title(run);
        }
        if watch {
            self.watch(run);
        }
        if started {
            self.log(
                "Started a computer session",
                &[("conversation", serde_json::json!(conversation))],
            );
            self.notify(ComputerChange::Started);
        } else {
            self.notify(ComputerChange::Updated);
        }
        Ok(())
    }

    fn watch(&self, run: &str) {
        let Some(controls) = self.0.controls.get().cloned() else {
            return;
        };
        let started = controls
            .runs
            .started(run)
            .unwrap_or_else(|| self.0.clock.now());
        {
            let mut session = lock(&self.0.session);
            session.started_at = Some(started);
            session.feed = ComputerFeed::default();
            session.seq = None;
            session.held = Some(Vec::new());
        }
        let observer = self.clone();
        let subscription = controls.runs.observe(
            run,
            Arc::new(move |event: &HostEvent| observer.absorb(event)),
        );
        if let Some(previous) = std::mem::replace(&mut *lock(&self.0.observing), subscription) {
            previous.unsubscribe();
        }
        let recorded = controls.runs.record(run);
        {
            let mut session = lock(&self.0.session);
            let held = session.held.take().unwrap_or_default();
            for event in recorded.iter().chain(held.iter()) {
                apply(&mut session, event);
            }
        }
        self.notify(ComputerChange::Updated);
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let agent = self.clone();
        let run = run.to_owned();
        handle.spawn(async move {
            controls.runs.wait(&run).await;
            agent.run_ended(&run);
        });
    }

    pub fn run_ended(&self, run: &str) {
        lock(&self.0.grants).remove(run);
        lock(&self.0.pending).retain(|_, pending| pending.run != run);
        let ended = {
            let session = lock(&self.0.session);
            session.run.as_deref() == Some(run) && !session.handoff
        };
        if ended {
            self.end();
        }
    }

    pub fn absorb(&self, event: &HostEvent) {
        let now = self.0.clock.now();
        let changed = {
            let mut session = lock(&self.0.session);
            if session.conversation.is_none() {
                return;
            }
            if let Some(held) = session.held.as_mut() {
                held.push(event.clone());
                return;
            }
            match apply(&mut session, event) {
                Absorbed::Nothing => false,
                Absorbed::Changed => true,
                Absorbed::Token => {
                    let due = now - session.token_notified >= TOKEN_NOTIFY_MS;
                    if due {
                        session.token_notified = now;
                    }
                    due
                }
            }
        };
        if changed {
            self.notify(ComputerChange::Updated);
        }
    }

    fn feed_request(&self, id: &str) -> Option<(String, Option<String>)> {
        let session = lock(&self.0.session);
        let request = session
            .feed
            .approvals
            .iter()
            .find(|request| approval_id(request) == Some(id))?;
        let run = approval_run(request)
            .map(str::to_owned)
            .or_else(|| session.run.clone())?;
        Some((run, approval_kind(request).map(str::to_owned)))
    }

    fn settle_request(&self, id: &str) {
        lock(&self.0.session)
            .feed
            .approvals
            .retain(|request| approval_id(request) != Some(id));
        self.notify(ComputerChange::Updated);
    }

    pub fn end(&self) {
        if let Some(previous) = lock(&self.0.observing).take() {
            previous.unsubscribe();
        }
        let conversation = {
            let mut session = lock(&self.0.session);
            let conversation = session.conversation.take();
            let next = session.next;
            *session = Session {
                next,
                ..Session::default()
            };
            conversation
        };
        let Some(conversation) = conversation else {
            return;
        };
        self.log(
            "Ended a computer session",
            &[("conversation", serde_json::json!(conversation))],
        );
        self.notify(ComputerChange::Ended);
        let desktop = self.desktop();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move { desktop.close().await });
        }
    }

    pub fn action(&self, tool: &str, label: String) -> Result<u64, AppError> {
        let now = self.0.clock.now();
        let id = {
            let mut session = lock(&self.0.session);
            while session
                .window
                .front()
                .is_some_and(|at| now - *at >= RATE_WINDOW_MS)
            {
                session.window.pop_front();
            }
            if session.window.len() >= ACTIONS_PER_MINUTE {
                return Err(AppError::new(AppErrorCode::RateLimited, RATE_LIMITED));
            }
            session.window.push_back(now);
            session.next += 1;
            let id = session.next;
            session.actions.push_back(ComputerAction {
                id,
                tool: tool.to_owned(),
                label,
                state: ComputerActionState::Running,
                started_at: now,
                error: None,
            });
            while session.actions.len() > ACTIONS_KEPT {
                session.actions.pop_front();
            }
            session.status = ComputerStatus::Working;
            id
        };
        self.notify(ComputerChange::Updated);
        Ok(id)
    }

    pub fn finish(&self, id: u64, error: Option<&AppError>) {
        {
            let mut session = lock(&self.0.session);
            if let Some(action) = session.actions.iter_mut().find(|action| action.id == id) {
                action.state = match error.map(|error| error.code) {
                    None => ComputerActionState::Done,
                    Some(AppErrorCode::ApprovalDenied) => ComputerActionState::Denied,
                    Some(_) => ComputerActionState::Failed,
                };
                action.error = error.map(|error| error.message.clone());
            }
            if session.approval.is_some() {
                session.approval = None;
            }
            if session.status == ComputerStatus::Awaiting {
                session.status = ComputerStatus::Working;
            }
        }
        self.notify(ComputerChange::Updated);
    }

    pub fn awaiting(&self, id: u64, approval: ComputerApproval) {
        {
            let mut session = lock(&self.0.session);
            if let Some(action) = session.actions.iter_mut().find(|action| action.id == id) {
                action.state = ComputerActionState::Waiting;
            }
            session.approval = Some(approval);
            session.status = ComputerStatus::Awaiting;
        }
        self.notify(ComputerChange::Updated);
    }

    pub fn decided(&self, id: u64) {
        {
            let mut session = lock(&self.0.session);
            if let Some(action) = session.actions.iter_mut().find(|action| action.id == id)
                && action.state == ComputerActionState::Waiting
            {
                action.state = ComputerActionState::Running;
            }
            session.approval = None;
            session.status = ComputerStatus::Working;
        }
        self.notify(ComputerChange::Updated);
    }

    pub fn set_display(&self, display: ComputerDisplay) {
        let changed = {
            let mut session = lock(&self.0.session);
            let changed = session.display != Some(display);
            session.display = Some(display);
            changed
        };
        if changed {
            self.notify(ComputerChange::Updated);
        }
    }

    fn repositories<T>(
        &self,
        work: impl FnOnce(&Repositories<'_>) -> Result<T, DataError>,
    ) -> Result<T, AppError> {
        self.0
            .database
            .with_connection(|connection| work(&Repositories::new(connection)))
    }

    pub fn grants(&self, conversation: &str, run: &str) -> BTreeSet<String> {
        let mut apps: BTreeSet<String> = self
            .repositories(|repositories| Ok(repositories.app_grants.usable(conversation)?))
            .unwrap_or_default()
            .into_iter()
            .map(|row| row.app)
            .collect();
        if let Some(granted) = lock(&self.0.grants).get(run) {
            apps.extend(granted.iter().cloned());
        }
        apps
    }

    pub fn grant_run(&self, run: &str, apps: &[ComputerAppDto]) {
        lock(&self.0.grants)
            .entry(run.to_owned())
            .or_default()
            .extend(apps.iter().map(|app| app.app.clone()));
        self.granted("generation", apps);
    }

    pub fn grant_stored(
        &self,
        conversation: Option<&str>,
        apps: &[ComputerAppDto],
    ) -> Result<(), AppError> {
        let now = self.0.clock.now();
        self.repositories(|repositories| {
            for app in apps {
                repositories.app_grants.save(&AppGrantRow {
                    id: crate::platform::ids::new_id().hyphenated().to_string(),
                    app: app.app.clone(),
                    path: app.path.clone(),
                    icon: app.icon.clone(),
                    scope: if conversation.is_some() {
                        FolderGrantScope::Conversation.as_str().to_owned()
                    } else {
                        FolderGrantScope::Always.as_str().to_owned()
                    },
                    conversation_id: conversation.map(str::to_owned),
                    conversation_title: None,
                    granted_at: now,
                    last_used_at: None,
                })?;
            }
            Ok(())
        })?;
        self.granted(
            if conversation.is_some() {
                "conversation"
            } else {
                "always"
            },
            apps,
        );
        Ok(())
    }

    fn granted(&self, scope: &str, apps: &[ComputerAppDto]) {
        let names: Vec<&str> = apps.iter().map(|app| app.app.as_str()).collect();
        self.log(
            "Granted apps for computer use",
            &[
                ("scope", serde_json::json!(scope)),
                ("apps", serde_json::json!(names)),
            ],
        );
        self.notify(ComputerChange::Updated);
    }

    pub fn touch(&self, conversation: &str, apps: &BTreeSet<String>) {
        let now = self.0.clock.now();
        let _ = self.repositories(|repositories| {
            for app in apps {
                repositories.app_grants.touch(app, conversation, now)?;
            }
            Ok(())
        });
    }

    pub fn app_grants(&self) -> Result<Vec<AppGrantDto>, AppError> {
        let rows = self.repositories(|repositories| Ok(repositories.app_grants.list()?))?;
        Ok(rows
            .into_iter()
            .map(|row| AppGrantDto {
                scope: if row.conversation_id.is_some() {
                    FolderGrantScope::Conversation
                } else {
                    FolderGrantScope::Always
                },
                conversation: row.conversation_id.as_deref().and_then(|id| {
                    Some(FolderGrantChatDto {
                        id: zvs_contract::ConversationId::parse(id)?,
                        title: row.conversation_title.clone().unwrap_or_default(),
                    })
                }),
                id: row.id,
                app: row.app,
                path: row.path,
                icon: row.icon,
                granted_at: row.granted_at,
                last_used_at: row.last_used_at,
            })
            .filter(|grant| grant.scope == FolderGrantScope::Always || grant.conversation.is_some())
            .collect())
    }

    pub fn revoke(&self, id: &str) -> Result<(), AppError> {
        let removed =
            self.repositories(|repositories| Ok(repositories.app_grants.remove(id)?))?;
        if !removed {
            return Err(AppError::new(AppErrorCode::NotFound, GRANT_NOT_FOUND));
        }
        self.log("Revoked an app grant", &[("id", serde_json::json!(id))]);
        self.notify(ComputerChange::Updated);
        Ok(())
    }

    pub fn register_access(
        &self,
        approval: &str,
        run: &str,
        conversation: &str,
        apps: Vec<ComputerAppDto>,
    ) {
        lock(&self.0.pending).insert(
            approval.to_owned(),
            PendingAccess {
                run: run.to_owned(),
                conversation: conversation.to_owned(),
                apps,
                answer: None,
            },
        );
    }

    pub fn take_answer(&self, approval: &str) -> Option<FolderAccessAnswer> {
        lock(&self.0.pending)
            .remove(approval)
            .and_then(|pending| pending.answer)
    }

    pub fn answer(
        &self,
        run: &str,
        approval: &str,
        answer: FolderAccessAnswer,
    ) -> Result<(), AppError> {
        let (conversation, apps) = {
            let pending = lock(&self.0.pending);
            let entry = pending
                .get(approval)
                .filter(|entry| entry.run == run && entry.answer.is_none())
                .ok_or_else(|| AppError::new(AppErrorCode::Conflict, NOT_PENDING))?;
            (entry.conversation.clone(), entry.apps.clone())
        };
        let controls = self.controls()?;
        match answer {
            FolderAccessAnswer::Deny => {}
            FolderAccessAnswer::Once => self.grant_run(run, &apps),
            FolderAccessAnswer::Conversation => self.grant_stored(Some(&conversation), &apps)?,
            FolderAccessAnswer::Always => self.grant_stored(None, &apps)?,
        }
        if let Some(entry) = lock(&self.0.pending).get_mut(approval) {
            entry.answer = Some(answer);
        }
        if answer == FolderAccessAnswer::Deny {
            controls.runs.deny(run, approval)
        } else {
            controls.runs.approve(run, approval)
        }
    }

    pub fn answer_current(
        &self,
        approval: &str,
        answer: FolderAccessAnswer,
    ) -> Result<(), AppError> {
        match self.feed_request(approval) {
            Some((run, kind)) if kind.as_deref() != Some(APPS_APPROVAL_KIND) => {
                let folders = self
                    .controls()?
                    .folders
                    .ok_or_else(|| AppError::new(AppErrorCode::Conflict, NOT_PENDING))?;
                folders.answer(&run, approval, answer)?;
            }
            Some((run, _)) => self.answer(&run, approval, answer)?,
            None => {
                let run = self.pending(approval)?;
                self.answer(&run, approval, answer)?;
            }
        }
        self.settle_request(approval);
        Ok(())
    }

    pub fn collapse(&self, collapsed: bool) {
        lock(&self.0.session).collapsed = collapsed;
        self.notify(ComputerChange::Updated);
    }

    fn controls(&self) -> Result<ComputerControls, AppError> {
        self.0
            .controls
            .get()
            .cloned()
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, NO_SESSION))
    }

    fn current_run(&self) -> Result<String, AppError> {
        lock(&self.0.session)
            .run
            .clone()
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, NO_SESSION))
    }

    pub async fn stop(&self) -> Result<(), AppError> {
        let run = self.current_run()?;
        {
            let mut session = lock(&self.0.session);
            session.status = ComputerStatus::Stopping;
            session.handoff = false;
        }
        self.notify(ComputerChange::Updated);
        self.log("Stopped computer use", &[("run", serde_json::json!(run))]);
        let controls = self.controls()?;
        let cancelled = controls.runs.cancel(&run).await;
        self.end();
        cancelled
    }

    fn pending(&self, id: &str) -> Result<String, AppError> {
        let session = lock(&self.0.session);
        match (&session.approval, &session.run) {
            (Some(approval), Some(run)) if approval.id == id => Ok(run.clone()),
            _ => Err(AppError::new(AppErrorCode::Conflict, NOT_PENDING)),
        }
    }

    pub fn approve(&self, id: &str) -> Result<(), AppError> {
        let run = match self.feed_request(id) {
            Some((run, _)) => run,
            None => self.pending(id)?,
        };
        self.controls()?.runs.approve(&run, id)?;
        self.settle_request(id);
        Ok(())
    }

    pub fn deny(&self, id: &str) -> Result<(), AppError> {
        let run = match self.feed_request(id) {
            Some((run, _)) => run,
            None => self.pending(id)?,
        };
        self.controls()?.runs.deny(&run, id)?;
        self.settle_request(id);
        Ok(())
    }

    pub async fn say(&self, text: &str) -> Result<(), AppError> {
        let controls = self.controls()?;
        let (conversation, run) = {
            let mut session = lock(&self.0.session);
            let conversation = session
                .conversation
                .clone()
                .ok_or_else(|| AppError::new(AppErrorCode::NotFound, NO_SESSION))?;
            session.handoff = true;
            session.status = ComputerStatus::Starting;
            session.approval = None;
            (conversation, session.run.clone())
        };
        self.notify(ComputerChange::Updated);
        if let Some(run) = &run {
            let _ = controls.runs.cancel(run).await;
            controls.runs.wait(run).await;
        }
        match controls.chat.send(&conversation, text) {
            Ok(next) => {
                self.log(
                    "Sent a correction during computer use",
                    &[("run", serde_json::json!(next))],
                );
                self.begin(&conversation, &next)
            }
            Err(error) => {
                lock(&self.0.session).handoff = false;
                self.end();
                Err(error)
            }
        }
    }
}
