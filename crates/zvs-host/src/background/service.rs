use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Child;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, ArtifactDto, BackgroundShellDto, BackgroundShellId,
    BackgroundShellStatus, BackgroundShellStatusDto, HostEventDraft, RunOutcomeDto,
    RunOutcomeStatus, SHELL_BUFFER_MAX, SHELL_LIFETIME_SECS, SHELL_STEP_DOMAIN, SHELL_TAIL_MAX,
    SHELLS_PER_OWNER_MAX, SHELLS_TOTAL_MAX, ShellOutputDto, Timestamp,
};

use crate::drivers::mcp::{default_spawn, kill_tree};
use crate::platform::clock::SharedClock;
use crate::platform::events::{EventBus, StreamHandle};
use crate::platform::ids::new_id;
use crate::terminal::service::{decode, trim_scrollback};
use crate::toolchain::LaunchSpec;

pub const SHELL_MISSING: &str = "Фоновый процесс не найден";
pub const OWNER_LIMIT: &str = "Запущено максимальное число фоновых процессов для этого диалога";
pub const TOTAL_LIMIT: &str = "Запущено максимальное число фоновых процессов в приложении";
pub const ENDED_KEPT: usize = 8;

const READ_CHUNK: usize = 8192;
const DRAIN_GRACE: Duration = Duration::from_secs(2);
const STOP_GRACE: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellLimits {
    pub per_owner: usize,
    pub total: usize,
    pub lifetime: Duration,
    pub buffer: usize,
    pub kept: usize,
}

impl Default for ShellLimits {
    fn default() -> Self {
        Self {
            per_owner: SHELLS_PER_OWNER_MAX,
            total: SHELLS_TOTAL_MAX,
            lifetime: Duration::from_secs(SHELL_LIFETIME_SECS),
            buffer: SHELL_BUFFER_MAX,
            kept: ENDED_KEPT,
        }
    }
}

pub struct BackgroundShellOptions {
    pub events: EventBus,
    pub clock: SharedClock,
    pub limits: ShellLimits,
}

impl BackgroundShellOptions {
    pub fn new(events: EventBus, clock: SharedClock) -> Self {
        Self {
            events,
            clock,
            limits: ShellLimits::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndedShell {
    pub shell: BackgroundShellDto,
    pub tail: String,
    pub artifacts: Vec<ArtifactDto>,
}

pub type ShellFinish = Box<dyn FnOnce() -> Vec<ArtifactDto> + Send>;
pub type EndedListener = Arc<dyn Fn(&str, bool) + Send + Sync>;

#[derive(Default)]
pub struct ShellStart {
    pub notify: bool,
    pub finish: Option<ShellFinish>,
}

impl ShellStart {
    pub fn notify(notify: bool) -> Self {
        Self {
            notify,
            finish: None,
        }
    }

    #[must_use]
    pub fn finishing(mut self, finish: ShellFinish) -> Self {
        self.finish = Some(finish);
        self
    }
}

#[derive(Clone)]
pub struct BackgroundShells {
    inner: Arc<Inner>,
}

struct Inner {
    options: BackgroundShellOptions,
    shells: Mutex<Vec<Arc<Shell>>>,
    listeners: Arc<Mutex<Vec<EndedListener>>>,
}

struct Shell {
    id: BackgroundShellId,
    owner: String,
    program: String,
    argv: Vec<String>,
    cwd: String,
    started_at: Timestamp,
    stream: StreamHandle,
    stop: CancellationToken,
    state: Mutex<ShellState>,
    task: Mutex<Option<JoinHandle<()>>>,
    finish: Mutex<Option<ShellFinish>>,
}

struct ShellState {
    status: BackgroundShellStatus,
    exit_code: Option<i32>,
    ended_at: Option<Timestamp>,
    buffer: String,
    start: u64,
    total: u64,
    reported: bool,
    closed: bool,
    events: u64,
    notify: bool,
    artifacts: Vec<ArtifactDto>,
}

enum Ending {
    Exited(Option<i32>),
    Failed,
    TimedOut,
    Stopped,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn missing() -> AppError {
    AppError::new(AppErrorCode::NotFound, SHELL_MISSING)
}

fn byte_len(text: &str) -> u64 {
    u64::try_from(text.len()).unwrap_or(u64::MAX)
}

fn last_lines(text: &str, lines: usize) -> &str {
    let trimmed = text.trim_end_matches('\n');
    let mut cut = trimmed.len();
    for _ in 0..lines {
        match trimmed[..cut].rfind('\n') {
            Some(index) => cut = index,
            None => return trimmed,
        }
    }
    &trimmed[cut + 1..]
}

fn clipped_tail(text: &str, lines: usize) -> String {
    let tail = last_lines(text, lines);
    if tail.len() <= SHELL_TAIL_MAX {
        return tail.to_owned();
    }
    let mut cut = tail.len() - SHELL_TAIL_MAX;
    while !tail.is_char_boundary(cut) {
        cut += 1;
    }
    tail[cut..].to_owned()
}

impl Shell {
    fn dto(&self, state: &ShellState) -> BackgroundShellDto {
        BackgroundShellDto {
            id: self.id,
            owner_id: self.owner.clone(),
            program: self.program.clone(),
            argv: self.argv.clone(),
            cwd: self.cwd.clone(),
            status: state.status,
            exit_code: state.exit_code,
            stream_id: self.stream.id(),
            started_at: self.started_at,
            ended_at: state.ended_at,
        }
    }

    fn emit(&self, state: &mut ShellState, draft: HostEventDraft) {
        if state.closed {
            return;
        }
        self.stream.emit(draft);
        state.events += 1;
    }

    fn emit_status(&self, state: &mut ShellState) {
        let dto = BackgroundShellStatusDto {
            domain: SHELL_STEP_DOMAIN.to_owned(),
            id: self.id,
            owner_id: self.owner.clone(),
            status: state.status,
            exit_code: state.exit_code,
        };
        if let Ok(Value::Object(step)) = serde_json::to_value(&dto) {
            self.emit(state, HostEventDraft::Step { step });
        }
    }

    fn append(&self, text: String, limit: usize) {
        if text.is_empty() {
            return;
        }
        let mut state = lock(&self.state);
        if state.closed {
            return;
        }
        state.total += byte_len(&text);
        state.buffer.push_str(&text);
        let before = state.buffer.len();
        trim_scrollback(&mut state.buffer, limit);
        let removed = before - state.buffer.len();
        state.start += u64::try_from(removed).unwrap_or(u64::MAX);
        self.emit(
            &mut state,
            HostEventDraft::Token {
                delta: text,
                kind: None,
            },
        );
    }

    fn finish(&self, ending: &Ending, now: Timestamp, artifacts: Vec<ArtifactDto>) {
        let mut state = lock(&self.state);
        state.artifacts = artifacts;
        let (status, code) = match ending {
            Ending::Exited(code) => (BackgroundShellStatus::Exited, *code),
            Ending::Failed => (BackgroundShellStatus::Failed, None),
            Ending::TimedOut => (BackgroundShellStatus::TimedOut, None),
            Ending::Stopped => (BackgroundShellStatus::Stopped, None),
        };
        state.status = status;
        state.exit_code = code;
        state.ended_at = Some(now);
        self.emit_status(&mut state);
    }

    fn read(&self, since: Option<u64>) -> (String, u64, bool, u64) {
        let state = lock(&self.state);
        let since = since.unwrap_or(0);
        if since >= state.total {
            return (String::new(), state.total, false, state.events);
        }
        let truncated = since < state.start;
        let offset = usize::try_from(since.saturating_sub(state.start)).unwrap_or(usize::MAX);
        let mut offset = offset.min(state.buffer.len());
        while !state.buffer.is_char_boundary(offset) {
            offset += 1;
        }
        (
            state.buffer[offset..].to_owned(),
            state.total,
            truncated,
            state.events,
        )
    }

    fn close(&self) {
        let mut state = lock(&self.state);
        if state.closed {
            return;
        }
        state.closed = true;
        self.stream.end(RunOutcomeDto {
            status: RunOutcomeStatus::Ok,
            code: None,
            message: None,
        });
    }

    async fn halt(&self) {
        self.stop.cancel();
        let task = lock(&self.task).take();
        if let Some(task) = task {
            let handle = task.abort_handle();
            if tokio::time::timeout(STOP_GRACE, task).await.is_err() {
                handle.abort();
            }
        }
        drop(lock(&self.finish).take());
    }
}

async fn drain(shell: Arc<Shell>, mut stream: impl AsyncRead + Unpin, limit: usize) {
    let mut buffer = [0_u8; READ_CHUNK];
    let mut pending = Vec::new();
    loop {
        match stream.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                pending.extend_from_slice(&buffer[..read]);
                shell.append(decode(&mut pending), limit);
            }
        }
    }
    if !pending.is_empty() {
        shell.append(String::from_utf8_lossy(&pending).into_owned(), limit);
    }
}

async fn supervise(
    shell: Arc<Shell>,
    mut child: Child,
    limits: ShellLimits,
    clock: SharedClock,
    listeners: Arc<Mutex<Vec<EndedListener>>>,
) {
    let mut readers = Vec::new();
    if let Some(pipe) = child.stdout.take() {
        readers.push(tokio::spawn(drain(Arc::clone(&shell), pipe, limits.buffer)));
    }
    if let Some(pipe) = child.stderr.take() {
        readers.push(tokio::spawn(drain(Arc::clone(&shell), pipe, limits.buffer)));
    }
    let ending = tokio::select! {
        status = child.wait() => match status {
            Ok(status) => Ending::Exited(status.code()),
            Err(_) => Ending::Failed,
        },
        () = tokio::time::sleep(limits.lifetime) => Ending::TimedOut,
        () = shell.stop.cancelled() => Ending::Stopped,
    };
    if matches!(ending, Ending::TimedOut | Ending::Stopped) {
        kill_tree(&mut child).await;
        let _ = child.wait().await;
    }
    for reader in readers {
        let handle = reader.abort_handle();
        if tokio::time::timeout(DRAIN_GRACE, reader).await.is_err() {
            handle.abort();
        }
    }
    let finish = lock(&shell.finish).take();
    let artifacts = match finish {
        Some(finish) => tokio::task::spawn_blocking(finish)
            .await
            .unwrap_or_default(),
        None => Vec::new(),
    };
    shell.finish(&ending, clock.now(), artifacts);
    let notify = lock(&shell.state).notify;
    let listeners = lock(&listeners).clone();
    for listener in listeners {
        listener(&shell.owner, notify);
    }
}

impl BackgroundShells {
    pub fn new(options: BackgroundShellOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                shells: Mutex::new(Vec::new()),
                listeners: Arc::new(Mutex::new(Vec::new())),
            }),
        }
    }

    pub fn on_ended(&self, listener: EndedListener) {
        lock(&self.inner.listeners).push(listener);
    }

    pub fn limits(&self) -> ShellLimits {
        self.inner.options.limits
    }

    fn shells(&self) -> MutexGuard<'_, Vec<Arc<Shell>>> {
        lock(&self.inner.shells)
    }

    fn find(&self, owner: Option<&str>, id: &BackgroundShellId) -> Result<Arc<Shell>, AppError> {
        self.shells()
            .iter()
            .find(|shell| shell.id == *id && owner.is_none_or(|owner| shell.owner == owner))
            .cloned()
            .ok_or_else(missing)
    }

    fn running(shell: &Shell) -> bool {
        lock(&shell.state).status.running()
    }

    pub fn start(
        &self,
        owner: &str,
        spec: &LaunchSpec,
        argv: &[String],
        not_started: &str,
        launch: ShellStart,
    ) -> Result<BackgroundShellDto, AppError> {
        let limits = self.inner.options.limits;
        let mut shells = self.shells();
        let running: Vec<&Arc<Shell>> =
            shells.iter().filter(|shell| Self::running(shell)).collect();
        if running.iter().filter(|shell| shell.owner == owner).count() >= limits.per_owner {
            return Err(AppError::new(AppErrorCode::Conflict, OWNER_LIMIT)
                .with_detail("limit", limits.per_owner));
        }
        if running.len() >= limits.total {
            return Err(AppError::new(AppErrorCode::Conflict, TOTAL_LIMIT)
                .with_detail("limit", limits.total));
        }
        let stream = self.inner.options.events.open_stream()?;
        let mut child = match default_spawn(spec) {
            Ok(child) => child,
            Err(error) => {
                stream.end(RunOutcomeDto {
                    status: RunOutcomeStatus::Failed,
                    code: Some(AppErrorCode::NativeError),
                    message: Some(not_started.to_owned()),
                });
                return Err(AppError::new(AppErrorCode::NativeError, not_started)
                    .with_detail("reason", error.to_string()));
            }
        };
        drop(child.stdin.take());
        let shell = Arc::new(Shell {
            id: BackgroundShellId::new(new_id()),
            owner: owner.to_owned(),
            program: spec.command.clone(),
            argv: argv.to_vec(),
            cwd: spec
                .cwd
                .as_ref()
                .map(|cwd| cwd.to_string_lossy().into_owned())
                .unwrap_or_default(),
            started_at: self.inner.options.clock.now(),
            stream,
            stop: CancellationToken::new(),
            state: Mutex::new(ShellState {
                status: BackgroundShellStatus::Running,
                exit_code: None,
                ended_at: None,
                buffer: String::new(),
                start: 0,
                total: 0,
                reported: false,
                closed: false,
                events: 0,
                notify: launch.notify,
                artifacts: Vec::new(),
            }),
            task: Mutex::new(None),
            finish: Mutex::new(launch.finish),
        });
        let dto = {
            let mut state = lock(&shell.state);
            shell.emit_status(&mut state);
            shell.dto(&state)
        };
        let task = tokio::spawn(supervise(
            Arc::clone(&shell),
            child,
            limits,
            Arc::clone(&self.inner.options.clock),
            Arc::clone(&self.inner.listeners),
        ));
        *lock(&shell.task) = Some(task);
        shells.push(shell);
        let evicted = Self::evict(&mut shells, owner, limits.kept);
        drop(shells);
        for shell in evicted {
            shell.close();
        }
        Ok(dto)
    }

    fn evict(shells: &mut Vec<Arc<Shell>>, owner: &str, kept: usize) -> Vec<Arc<Shell>> {
        let ended: Vec<BackgroundShellId> = shells
            .iter()
            .filter(|shell| shell.owner == owner && !Self::running(shell))
            .map(|shell| shell.id)
            .collect();
        let excess = ended.len().saturating_sub(kept);
        let doomed = &ended[..excess];
        let mut evicted = Vec::new();
        shells.retain(|shell| {
            if doomed.contains(&shell.id) {
                evicted.push(Arc::clone(shell));
                false
            } else {
                true
            }
        });
        evicted
    }

    pub fn list(&self, owner: &str) -> Vec<BackgroundShellDto> {
        self.shells()
            .iter()
            .filter(|shell| shell.owner == owner)
            .map(|shell| shell.dto(&lock(&shell.state)))
            .collect()
    }

    pub fn get(
        &self,
        owner: Option<&str>,
        id: &BackgroundShellId,
    ) -> Result<BackgroundShellDto, AppError> {
        let shell = self.find(owner, id)?;
        let state = lock(&shell.state);
        Ok(shell.dto(&state))
    }

    pub fn output(
        &self,
        owner: Option<&str>,
        id: &BackgroundShellId,
        since: Option<u64>,
    ) -> Result<ShellOutputDto, AppError> {
        let shell = self.find(owner, id)?;
        let (text, next_seq, truncated, event_seq) = shell.read(since);
        let dto = shell.dto(&lock(&shell.state));
        Ok(ShellOutputDto {
            shell: dto,
            text,
            next_seq,
            truncated,
            event_seq,
        })
    }

    pub fn tail(
        &self,
        owner: Option<&str>,
        id: &BackgroundShellId,
        lines: usize,
    ) -> Result<(BackgroundShellDto, String, bool), AppError> {
        let output = self.output(owner, id, None)?;
        let tail = last_lines(&output.text, lines).to_owned();
        let cut = output.truncated || tail.len() < output.text.trim_end_matches('\n').len();
        Ok((output.shell, tail, cut))
    }

    pub async fn kill(
        &self,
        owner: Option<&str>,
        id: &BackgroundShellId,
    ) -> Result<BackgroundShellDto, AppError> {
        let shell = self.find(owner, id)?;
        shell.halt().await;
        let state = lock(&shell.state);
        Ok(shell.dto(&state))
    }

    pub fn take_ended(&self, owner: &str, lines: usize) -> Vec<EndedShell> {
        self.collect(owner, lines, false, true)
    }

    pub fn peek_ended(&self, owner: &str, lines: usize) -> Vec<EndedShell> {
        self.collect(owner, lines, false, false)
    }

    pub fn take_notified(&self, owner: &str, lines: usize) -> Vec<EndedShell> {
        self.collect(owner, lines, true, true)
    }

    fn unreported(&self, owner: &str) -> Vec<Arc<Shell>> {
        self.shells()
            .iter()
            .filter(|shell| shell.owner == owner)
            .filter(|shell| {
                let state = lock(&shell.state);
                !state.status.running() && !state.reported
            })
            .cloned()
            .collect()
    }

    pub fn has_notified(&self, owner: &str) -> bool {
        self.unreported(owner)
            .iter()
            .any(|shell| lock(&shell.state).notify)
    }

    pub fn demote(&self, owner: &str) -> usize {
        let mut demoted = 0;
        for shell in self.unreported(owner) {
            let mut state = lock(&shell.state);
            if state.notify {
                state.notify = false;
                demoted += 1;
            }
        }
        demoted
    }

    fn collect(&self, owner: &str, lines: usize, notified: bool, mark: bool) -> Vec<EndedShell> {
        let shells: Vec<Arc<Shell>> = self
            .shells()
            .iter()
            .filter(|shell| shell.owner == owner)
            .cloned()
            .collect();
        let mut ended = Vec::new();
        for shell in shells {
            let (text, _, _, _) = shell.read(None);
            let mut state = lock(&shell.state);
            if state.status.running() || state.reported || state.notify != notified {
                continue;
            }
            if mark {
                state.reported = true;
            }
            ended.push(EndedShell {
                shell: shell.dto(&state),
                tail: clipped_tail(&text, lines),
                artifacts: state.artifacts.clone(),
            });
        }
        ended
    }

    async fn remove(&self, owner: Option<&str>) {
        let removed: Vec<Arc<Shell>> = {
            let mut shells = self.shells();
            let mut removed = Vec::new();
            shells.retain(|shell| {
                if owner.is_none_or(|owner| shell.owner == owner) {
                    removed.push(Arc::clone(shell));
                    false
                } else {
                    true
                }
            });
            removed
        };
        for shell in &removed {
            shell.halt().await;
            shell.close();
        }
    }

    pub async fn close_owner(&self, owner: &str) {
        self.remove(Some(owner)).await;
    }

    pub async fn dispose(&self) {
        self.remove(None).await;
    }

    pub fn running_count(&self) -> usize {
        self.shells()
            .iter()
            .filter(|shell| Self::running(shell))
            .count()
    }
}
