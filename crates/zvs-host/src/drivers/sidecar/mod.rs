mod protocol;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, AppErrorCode};

pub use self::protocol::{JobProgress, SidecarRequest, SidecarResponse};
use crate::platform::ids::new_id;
use crate::platform::logger::{Field, LogLevel, SharedLogger, silent_logger};

pub const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;
pub const BUILD_HINT: &str = "Выполните cargo build -p zvs-jobd из корня репозитория.";
pub const STOPPED_MESSAGE: &str = "Сервис задач остановлен";
pub const CANCELLED_MESSAGE: &str = "Операция отменена";
pub const PING_TIMEOUT_MESSAGE: &str = "Sidecar не ответил на ping";
pub const NOT_RUNNING_MESSAGE: &str = "Sidecar не запущен";

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub type ProgressSink = Arc<dyn Fn(JobProgress) + Send + Sync>;

#[derive(Clone, Default)]
pub struct JobRunOptions {
    pub cancel: Option<CancellationToken>,
    pub on_progress: Option<ProgressSink>,
}

#[async_trait]
pub trait SidecarJobs: Send + Sync {
    async fn run(
        &self,
        job: &str,
        params: Value,
        options: JobRunOptions,
    ) -> Result<Value, AppError>;
}

pub type SharedSidecarJobs = Arc<dyn SidecarJobs>;

#[derive(Clone)]
pub struct SidecarOptions {
    pub binary_path: PathBuf,
    pub args: Vec<String>,
    pub logger: SharedLogger,
    pub ping_interval: Duration,
    pub ping_timeout: Duration,
    pub cancel_grace: Duration,
    pub backoff: Duration,
    pub max_backoff: Duration,
}

impl SidecarOptions {
    pub fn new(binary_path: PathBuf) -> Self {
        Self {
            binary_path,
            args: Vec::new(),
            logger: silent_logger(),
            ping_interval: Duration::from_secs(5),
            ping_timeout: Duration::from_secs(2),
            cancel_grace: Duration::from_secs(2),
            backoff: Duration::from_millis(250),
            max_backoff: Duration::from_secs(30),
        }
    }
}

struct Pending {
    reply: oneshot::Sender<Result<Value, AppError>>,
    on_progress: Option<ProgressSink>,
}

struct Running {
    generation: u64,
    pid: Option<u32>,
    input: mpsc::UnboundedSender<String>,
    kill: CancellationToken,
    exited: watch::Receiver<bool>,
}

#[derive(Default)]
struct State {
    child: Option<Running>,
    jobs: HashMap<String, Pending>,
    pings: HashMap<String, oneshot::Sender<()>>,
    restarts: u32,
    next_spawn_at: Option<Instant>,
    generation: u64,
}

struct Inner {
    options: SidecarOptions,
    state: Mutex<State>,
    starting: tokio::sync::Mutex<()>,
    closed: AtomicBool,
}

#[derive(Clone)]
pub struct SidecarDriver {
    inner: Arc<Inner>,
}

fn unavailable(message: impl Into<String>) -> AppError {
    AppError::new(AppErrorCode::SidecarUnavailable, message)
}

fn sidecar_error(code: &str, message: String) -> AppError {
    let code = AppErrorCode::ALL
        .iter()
        .copied()
        .find(|candidate| candidate.as_str() == code)
        .unwrap_or(AppErrorCode::NativeError);
    AppError::new(code, message)
}

fn exit_reason(status: std::io::Result<ExitStatus>) -> String {
    match status {
        Ok(status) => {
            let code = status
                .code()
                .map_or_else(|| "null".to_owned(), |code| code.to_string());
            #[cfg(unix)]
            let signal = std::os::unix::process::ExitStatusExt::signal(&status)
                .map_or_else(|| "null".to_owned(), |signal| signal.to_string());
            #[cfg(not(unix))]
            let signal = "null";
            format!("Sidecar завершился (code={code}, signal={signal})")
        }
        Err(error) => format!("Sidecar завершился ({error})"),
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

struct JobSlot<'a> {
    inner: &'a Inner,
    id: String,
}

impl Drop for JobSlot<'_> {
    fn drop(&mut self) {
        self.inner.state().jobs.remove(&self.id);
    }
}

struct PingSlot<'a> {
    inner: &'a Inner,
    id: String,
}

impl Drop for PingSlot<'_> {
    fn drop(&mut self) {
        self.inner.state().pings.remove(&self.id);
    }
}

impl SidecarDriver {
    pub fn new(options: SidecarOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                state: Mutex::new(State::default()),
                starting: tokio::sync::Mutex::new(()),
                closed: AtomicBool::new(false),
            }),
        }
    }

    pub fn pid(&self) -> Option<u32> {
        self.inner
            .state()
            .child
            .as_ref()
            .and_then(|child| child.pid)
    }

    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::SeqCst)
    }

    pub async fn ping(&self) -> Result<(), AppError> {
        let generation = self.ensure().await?;
        Inner::ping_child(&self.inner, generation).await
    }

    pub async fn dispose(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);
        let running = self
            .inner
            .state()
            .child
            .as_ref()
            .map(|child| (child.kill.clone(), child.exited.clone()));
        if let Some((kill, mut exited)) = running {
            kill.cancel();
            let _ = exited.wait_for(|done| *done).await;
        }
    }

    async fn ensure(&self) -> Result<u64, AppError> {
        if let Some(generation) = self.inner.current() {
            return Ok(generation);
        }
        let _starting = self.inner.starting.lock().await;
        if let Some(generation) = self.inner.current() {
            return Ok(generation);
        }
        Inner::spawn(&self.inner).await
    }

    fn cancel_job(&self, id: &str, job: &str) -> bool {
        self.inner.log(
            LogLevel::Info,
            "Cancelling a sidecar job",
            &[("id", json!(id)), ("job", json!(job))],
        );
        match self.inner.send(&SidecarRequest::JobCancel { id }) {
            Ok(()) => true,
            Err(error) => {
                self.inner.log(
                    LogLevel::Warn,
                    "Could not deliver a cancellation",
                    &[
                        ("id", json!(id)),
                        ("job", json!(job)),
                        ("error", json!(error.to_string())),
                    ],
                );
                false
            }
        }
    }
}

#[async_trait]
impl SidecarJobs for SidecarDriver {
    async fn run(
        &self,
        job: &str,
        params: Value,
        options: JobRunOptions,
    ) -> Result<Value, AppError> {
        if self.is_closed() {
            return Err(AppError::new(AppErrorCode::Conflict, STOPPED_MESSAGE));
        }
        let cancel = options.cancel.unwrap_or_default();
        if cancel.is_cancelled() {
            return Err(AppError::new(AppErrorCode::RunCancelled, CANCELLED_MESSAGE));
        }
        self.ensure().await?;
        let id = new_id().to_string();
        let (reply, mut answer) = oneshot::channel();
        self.inner.state().jobs.insert(
            id.clone(),
            Pending {
                reply,
                on_progress: options.on_progress,
            },
        );
        let _slot = JobSlot {
            inner: &self.inner,
            id: id.clone(),
        };
        self.inner.send(&SidecarRequest::JobStart {
            id: &id,
            job,
            params: &params,
        })?;
        let grace = tokio::time::sleep(Duration::ZERO);
        tokio::pin!(grace);
        let mut armed = false;
        let mut cancelling = false;
        loop {
            tokio::select! {
                outcome = &mut answer => {
                    return outcome.unwrap_or_else(|_| Err(unavailable(NOT_RUNNING_MESSAGE)));
                }
                () = cancel.cancelled(), if !cancelling => {
                    cancelling = true;
                    if self.cancel_job(&id, job) {
                        grace.as_mut().reset(Instant::now() + self.inner.options.cancel_grace);
                        armed = true;
                    }
                }
                () = &mut grace, if armed => {
                    armed = false;
                    if self.inner.state().jobs.contains_key(&id) {
                        self.inner.log(
                            LogLevel::Error,
                            "A job ignored cancellation; killing the sidecar",
                            &[
                                ("id", json!(id)),
                                ("job", json!(job)),
                                ("graceMs", json!(millis(self.inner.options.cancel_grace))),
                            ],
                        );
                        self.inner.kill();
                    }
                }
            }
        }
    }
}

impl Inner {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn log(&self, level: LogLevel, message: &str, fields: &[Field<'_>]) {
        self.options.logger.log(level, "sidecar", message, fields);
    }

    fn closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn current(&self) -> Option<u64> {
        self.state().child.as_ref().map(|child| child.generation)
    }

    fn kill(&self) {
        if let Some(child) = &self.state().child {
            child.kill.cancel();
        }
    }

    fn send(&self, request: &SidecarRequest<'_>) -> Result<(), AppError> {
        let state = self.state();
        let child = state
            .child
            .as_ref()
            .ok_or_else(|| unavailable(NOT_RUNNING_MESSAGE))?;
        Self::write(child, request)
    }

    fn write(child: &Running, request: &SidecarRequest<'_>) -> Result<(), AppError> {
        let mut line = serde_json::to_string(request)
            .map_err(|error| AppError::new(AppErrorCode::ValidationFailed, error.to_string()))?;
        line.push('\n');
        child
            .input
            .send(line)
            .map_err(|_| unavailable(NOT_RUNNING_MESSAGE))
    }

    async fn ping_child(inner: &Arc<Self>, generation: u64) -> Result<(), AppError> {
        let id = new_id().to_string();
        let (sender, receiver) = oneshot::channel();
        {
            let mut state = inner.state();
            let Some(child) = state
                .child
                .as_ref()
                .filter(|child| child.generation == generation)
            else {
                return Err(unavailable(NOT_RUNNING_MESSAGE));
            };
            Self::write(child, &SidecarRequest::Ping { id: &id })?;
            state.pings.insert(id.clone(), sender);
        }
        let _slot = PingSlot { inner, id };
        match tokio::time::timeout(inner.options.ping_timeout, receiver).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(unavailable(NOT_RUNNING_MESSAGE)),
            Err(_) => Err(unavailable(PING_TIMEOUT_MESSAGE)),
        }
    }

    async fn spawn(inner: &Arc<Self>) -> Result<u64, AppError> {
        let path = &inner.options.binary_path;
        if !path.is_file() {
            return Err(unavailable(format!(
                "Не найден sidecar zvs-jobd: {}. {BUILD_HINT}",
                path.display()
            )));
        }
        let next_spawn_at = inner.state().next_spawn_at;
        if let Some(at) = next_spawn_at {
            let wait = at.saturating_duration_since(Instant::now());
            if !wait.is_zero() {
                inner.log(
                    LogLevel::Info,
                    "Waiting out the sidecar restart backoff",
                    &[("waitMs", json!(millis(wait)))],
                );
                tokio::time::sleep(wait).await;
            }
        }
        if inner.closed() {
            return Err(AppError::new(AppErrorCode::Conflict, STOPPED_MESSAGE));
        }
        let mut command = Command::new(path);
        command
            .args(&inner.options.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);
        let mut child = command.spawn().map_err(|error| {
            inner.log(
                LogLevel::Error,
                "The sidecar could not run",
                &[("raw", json!(error.to_string()))],
            );
            unavailable(format!(
                "Не удалось запустить sidecar zvs-jobd: {}. {BUILD_HINT}",
                path.display()
            ))
        })?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            let _ = child.start_kill();
            return Err(unavailable(NOT_RUNNING_MESSAGE));
        };
        let pid = child.id();
        let kill = CancellationToken::new();
        let (input, lines) = mpsc::unbounded_channel();
        let (exited_sender, exited) = watch::channel(false);
        let generation = {
            let mut state = inner.state();
            state.generation += 1;
            let generation = state.generation;
            state.child = Some(Running {
                generation,
                pid,
                input,
                kill: kill.clone(),
                exited,
            });
            generation
        };

        tokio::spawn(Self::write_input(Arc::clone(inner), stdin, lines));
        tokio::spawn(Self::read_output(Arc::clone(inner), stdout, kill.clone()));
        tokio::spawn(Self::read_diagnostics(Arc::clone(inner), stderr));
        tokio::spawn(Self::health(Arc::clone(inner), generation, kill.clone()));
        {
            let inner = Arc::clone(inner);
            tokio::spawn(async move {
                let status = tokio::select! {
                    status = child.wait() => status,
                    () = kill.cancelled() => {
                        let _ = child.start_kill();
                        child.wait().await
                    }
                };
                kill.cancel();
                inner.retire(generation, exit_reason(status));
                let _ = exited_sender.send(true);
            });
        }
        inner.log(
            LogLevel::Info,
            "Started the sidecar",
            &[
                ("binaryPath", json!(path.display().to_string())),
                ("pid", json!(pid)),
            ],
        );
        Ok(generation)
    }

    async fn write_input(
        inner: Arc<Self>,
        mut stdin: ChildStdin,
        mut lines: mpsc::UnboundedReceiver<String>,
    ) {
        while let Some(line) = lines.recv().await {
            let written = async {
                stdin.write_all(line.as_bytes()).await?;
                stdin.flush().await
            }
            .await;
            if let Err(error) = written {
                inner.log(
                    LogLevel::Warn,
                    "Sidecar input failed",
                    &[("raw", json!(error.to_string()))],
                );
                return;
            }
        }
    }

    async fn read_output(
        inner: Arc<Self>,
        stdout: impl AsyncRead + Unpin,
        kill: CancellationToken,
    ) {
        let mut reader = BufReader::new(stdout);
        let mut line: Vec<u8> = Vec::new();
        loop {
            let available = match reader.fill_buf().await {
                Ok([]) | Err(_) => return,
                Ok(available) => available,
            };
            let (taken, complete) = match available.iter().position(|byte| *byte == b'\n') {
                Some(index) => {
                    line.extend_from_slice(&available[..index]);
                    (index + 1, true)
                }
                None => {
                    line.extend_from_slice(available);
                    (available.len(), false)
                }
            };
            reader.consume(taken);
            if complete {
                let text = String::from_utf8_lossy(&line);
                let text = text.trim();
                if !text.is_empty() {
                    inner.route(text);
                }
                line.clear();
            } else if line.len() > MAX_LINE_BYTES {
                inner.log(
                    LogLevel::Error,
                    "The sidecar sent an oversized line; killing it",
                    &[("bytes", json!(line.len()))],
                );
                kill.cancel();
                return;
            }
        }
    }

    async fn read_diagnostics(inner: Arc<Self>, stderr: impl AsyncRead + Unpin) {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim();
            if !line.is_empty() {
                inner.log(
                    LogLevel::Warn,
                    "Sidecar diagnostics",
                    &[("raw", json!(line))],
                );
            }
        }
    }

    async fn health(inner: Arc<Self>, generation: u64, kill: CancellationToken) {
        let period = inner.options.ping_interval;
        let mut ticker = tokio::time::interval_at(Instant::now() + period, period);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = kill.cancelled() => return,
                _ = ticker.tick() => {}
            }
            if inner.closed() || inner.current() != Some(generation) {
                return;
            }
            if let Err(error) = Self::ping_child(&inner, generation).await {
                if inner.current() != Some(generation) {
                    return;
                }
                inner.log(
                    LogLevel::Error,
                    "The sidecar stopped answering ping; killing it",
                    &[("raw", json!(error.to_string()))],
                );
                kill.cancel();
                return;
            }
        }
    }

    fn retire(&self, generation: u64, reason: String) {
        let (abandoned, restarts, backoff) = {
            let mut state = self.state();
            if state
                .child
                .as_ref()
                .is_none_or(|child| child.generation != generation)
            {
                return;
            }
            state.child = None;
            state.pings.clear();
            let abandoned: Vec<Pending> = state.jobs.drain().map(|(_, pending)| pending).collect();
            if self.closed() {
                (abandoned, None, None)
            } else {
                let factor = 2u32.saturating_pow(state.restarts);
                let backoff = self
                    .options
                    .backoff
                    .saturating_mul(factor)
                    .min(self.options.max_backoff);
                state.restarts = state.restarts.saturating_add(1);
                state.next_spawn_at = Some(Instant::now() + backoff);
                (abandoned, Some(state.restarts), Some(backoff))
            }
        };
        let count = abandoned.len();
        for pending in abandoned {
            let _ = pending.reply.send(Err(unavailable(reason.clone())));
        }
        if let (Some(restarts), Some(backoff)) = (restarts, backoff) {
            self.log(
                LogLevel::Warn,
                "The sidecar exited; it restarts on the next job",
                &[
                    ("reason", json!(reason)),
                    ("abandoned", json!(count)),
                    ("restarts", json!(restarts)),
                    ("backoffMs", json!(millis(backoff))),
                ],
            );
        }
    }

    fn route(&self, line: &str) {
        let message = match SidecarResponse::parse(line) {
            Ok(message) => message,
            Err(error) => {
                self.log(
                    LogLevel::Error,
                    "Unreadable sidecar message",
                    &[("raw", json!(line)), ("error", json!(error))],
                );
                return;
            }
        };
        match message {
            SidecarResponse::Ping { id } => {
                let mut state = self.state();
                state.restarts = 0;
                if let Some(waiting) = state.pings.remove(&id) {
                    let _ = waiting.send(());
                }
            }
            SidecarResponse::JobProgress {
                id,
                done,
                total,
                rate,
                message,
            } => {
                let sink = {
                    let state = self.state();
                    match state.jobs.get(&id) {
                        Some(pending) => pending.on_progress.clone(),
                        None => {
                            drop(state);
                            self.unknown(&id);
                            return;
                        }
                    }
                };
                if let Some(sink) = sink {
                    sink(JobProgress {
                        done,
                        total,
                        rate,
                        message,
                    });
                }
            }
            SidecarResponse::JobDone { id, result } => self.settle(&id, Ok(result)),
            SidecarResponse::JobError { id, code, message } => {
                self.settle(&id, Err(sidecar_error(&code, message)));
            }
        }
    }

    fn settle(&self, id: &str, outcome: Result<Value, AppError>) {
        let pending = self.state().jobs.remove(id);
        match pending {
            Some(pending) => {
                let _ = pending.reply.send(outcome);
            }
            None => self.unknown(id),
        }
    }

    fn unknown(&self, id: &str) {
        self.log(
            LogLevel::Debug,
            "The sidecar answered an unknown request",
            &[("id", json!(id))],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MemoryLogger, temporary_directory};

    #[tokio::test]
    async fn a_missing_binary_names_the_expected_path_and_the_build_command() {
        let directory = temporary_directory("studio-sidecar-");
        let binary = directory.join("zvs-jobd.exe");
        let driver = SidecarDriver::new(SidecarOptions::new(binary.clone()));
        let error = driver
            .run("job.sleep", Value::Null, JobRunOptions::default())
            .await
            .expect_err("no binary");
        assert_eq!(error.code, AppErrorCode::SidecarUnavailable);
        assert!(error.message.contains(&binary.display().to_string()));
        assert!(error.message.contains("cargo build -p zvs-jobd"));
        assert!(driver.pid().is_none());
    }

    #[tokio::test]
    async fn a_cancelled_or_disposed_run_never_spawns() {
        let logger = MemoryLogger::shared();
        let driver = SidecarDriver::new(SidecarOptions {
            logger: logger.clone(),
            ..SidecarOptions::new(PathBuf::from("missing-zvs-jobd"))
        });
        let cancel = CancellationToken::new();
        cancel.cancel();
        let error = driver
            .run(
                "job.sleep",
                Value::Null,
                JobRunOptions {
                    cancel: Some(cancel),
                    on_progress: None,
                },
            )
            .await
            .expect_err("cancelled");
        assert_eq!(error.code, AppErrorCode::RunCancelled);
        driver.dispose().await;
        let error = driver
            .run("job.sleep", Value::Null, JobRunOptions::default())
            .await
            .expect_err("disposed");
        assert_eq!(error.code, AppErrorCode::Conflict);
        assert!(logger.entries().is_empty());
    }

    #[test]
    fn sidecar_codes_are_kept_and_unknown_ones_become_native_errors() {
        assert_eq!(
            sidecar_error("RUN_CANCELLED", "m".to_owned()).code,
            AppErrorCode::RunCancelled
        );
        assert_eq!(
            sidecar_error("future-code", "m".to_owned()).code,
            AppErrorCode::NativeError
        );
    }
}
