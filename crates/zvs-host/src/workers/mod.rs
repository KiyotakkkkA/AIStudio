use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures::future::{BoxFuture, join_all};
use serde_json::json;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use zvs_contract::integrations::check_worker_key;
use zvs_contract::{AppError, AppErrorCode, Timestamp, WorkerDto, WorkerStatus};

use crate::platform::clock::{SharedClock, SystemClock};
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};

pub const WORKER_NOT_FOUND: &str = "Фоновый процесс не найден";
pub const WORKER_PANICKED: &str = "Фоновый процесс завершился аварийно";
pub const WORKER_NO_RUNTIME: &str = "Фоновый процесс нельзя запустить вне асинхронной среды";

const STOP_GRACE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    pub initial: Duration,
    pub max: Duration,
    pub stable: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(300),
            stable: Duration::from_secs(30),
        }
    }
}

impl Backoff {
    pub fn delay(&self, attempt: u32) -> Duration {
        let factor = 2u32.saturating_pow(attempt.saturating_sub(1).min(16));
        self.initial.saturating_mul(factor).min(self.max)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkerFault {
    pub error: AppError,
    pub permanent: bool,
}

impl WorkerFault {
    pub const fn retry(error: AppError) -> Self {
        Self {
            error,
            permanent: false,
        }
    }

    pub const fn permanent(error: AppError) -> Self {
        Self {
            error,
            permanent: true,
        }
    }
}

impl From<AppError> for WorkerFault {
    fn from(error: AppError) -> Self {
        Self::retry(error)
    }
}

pub type WorkerBody =
    Arc<dyn Fn(WorkerContext) -> BoxFuture<'static, Result<(), WorkerFault>> + Send + Sync>;
pub type WorkerListener = Arc<dyn Fn(&WorkerDto) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerSpec {
    pub key: String,
    pub label: String,
    pub backoff: Backoff,
}

impl WorkerSpec {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            backoff: Backoff::default(),
        }
    }

    #[must_use]
    pub const fn with_backoff(mut self, backoff: Backoff) -> Self {
        self.backoff = backoff;
        self
    }
}

struct Slot {
    spec: WorkerSpec,
    body: WorkerBody,
    state: WorkerDto,
    cancel: CancellationToken,
    handle: Option<JoinHandle<()>>,
    generation: u64,
    ready_at: Option<Instant>,
}

struct Inner {
    clock: SharedClock,
    logger: SharedLogger,
    slots: Mutex<HashMap<String, Slot>>,
    listeners: Mutex<Vec<WorkerListener>>,
    generations: Mutex<u64>,
}

impl Inner {
    fn slots(&self) -> MutexGuard<'_, HashMap<String, Slot>> {
        self.slots.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn now(&self) -> Timestamp {
        self.clock.now()
    }

    fn update(
        &self,
        key: &str,
        generation: u64,
        change: impl FnOnce(&mut Slot, Timestamp),
    ) -> Option<WorkerDto> {
        let now = self.now();
        let snapshot = {
            let mut slots = self.slots();
            let slot = slots
                .get_mut(key)
                .filter(|slot| slot.generation == generation)?;
            change(slot, now);
            slot.state.clone()
        };
        self.notify(&snapshot);
        Some(snapshot)
    }

    fn notify(&self, snapshot: &WorkerDto) {
        let listeners = self
            .listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        for listener in listeners {
            listener(snapshot);
        }
    }
}

#[derive(Clone)]
pub struct WorkerContext {
    key: String,
    generation: u64,
    cancel: CancellationToken,
    inner: Arc<Inner>,
}

impl WorkerContext {
    pub fn key(&self) -> &str {
        &self.key
    }

    pub const fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub async fn cancelled(&self) {
        self.cancel.cancelled().await;
    }

    pub async fn sleep(&self, duration: Duration) -> bool {
        tokio::select! {
            () = self.cancel.cancelled() => false,
            () = tokio::time::sleep(duration) => true,
        }
    }

    pub fn ready(&self) {
        self.inner.update(&self.key, self.generation, |slot, now| {
            slot.state.status = WorkerStatus::Running;
            slot.state.heartbeat_at = Some(now);
            slot.state.retry_at = None;
            slot.ready_at = Some(Instant::now());
        });
    }

    pub fn heartbeat(&self) {
        self.inner.update(&self.key, self.generation, |slot, now| {
            slot.state.heartbeat_at = Some(now);
        });
    }

    pub fn detail(&self, detail: Option<String>) {
        self.inner.update(&self.key, self.generation, |slot, _| {
            slot.state.detail = detail;
        });
    }
}

#[derive(Clone)]
pub struct WorkerSupervisor {
    inner: Arc<Inner>,
}

impl Default for WorkerSupervisor {
    fn default() -> Self {
        Self::new(Arc::new(SystemClock), silent_logger())
    }
}

fn not_found(key: &str) -> AppError {
    AppError::new(AppErrorCode::NotFound, WORKER_NOT_FOUND).with_detail("key", key)
}

fn millis(duration: Duration) -> Timestamp {
    Timestamp::try_from(duration.as_millis()).unwrap_or(Timestamp::MAX)
}

impl WorkerSupervisor {
    pub fn new(clock: SharedClock, logger: SharedLogger) -> Self {
        Self {
            inner: Arc::new(Inner {
                clock,
                logger,
                slots: Mutex::new(HashMap::new()),
                listeners: Mutex::new(Vec::new()),
                generations: Mutex::new(0),
            }),
        }
    }

    pub fn watch(&self, listener: WorkerListener) {
        self.inner
            .listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(listener);
    }

    pub fn list(&self) -> Vec<WorkerDto> {
        let mut workers: Vec<WorkerDto> = self
            .inner
            .slots()
            .values()
            .map(|slot| slot.state.clone())
            .collect();
        workers.sort_by(|left, right| left.key.cmp(&right.key));
        workers
    }

    pub fn get(&self, key: &str) -> Option<WorkerDto> {
        self.inner.slots().get(key).map(|slot| slot.state.clone())
    }

    pub async fn spawn(&self, spec: WorkerSpec, body: WorkerBody) -> Result<WorkerDto, AppError> {
        check_worker_key("key", &spec.key)?;
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|_| AppError::new(AppErrorCode::Unknown, WORKER_NO_RUNTIME))?;
        self.halt(&spec.key).await;
        let generation = {
            let mut counter = self
                .inner
                .generations
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            *counter += 1;
            *counter
        };
        let cancel = CancellationToken::new();
        let key = spec.key.clone();
        let snapshot = {
            let mut slots = self.inner.slots();
            let restarts = slots.get(&key).map_or(0, |slot| slot.state.restarts);
            let state = WorkerDto {
                key: key.clone(),
                label: spec.label.clone(),
                status: WorkerStatus::Starting,
                restarts,
                detail: None,
                last_error: None,
                started_at: None,
                heartbeat_at: None,
                retry_at: None,
            };
            let handle = runtime.spawn(supervise(
                Arc::clone(&self.inner),
                key.clone(),
                generation,
                cancel.clone(),
                Arc::clone(&body),
                spec.backoff,
            ));
            slots.insert(
                key.clone(),
                Slot {
                    spec,
                    body,
                    state: state.clone(),
                    cancel,
                    handle: Some(handle),
                    generation,
                    ready_at: None,
                },
            );
            state
        };
        self.inner.notify(&snapshot);
        Ok(snapshot)
    }

    pub async fn restart(&self, key: &str) -> Result<WorkerDto, AppError> {
        let (spec, body) = {
            let slots = self.inner.slots();
            let slot = slots.get(key).ok_or_else(|| not_found(key))?;
            (slot.spec.clone(), Arc::clone(&slot.body))
        };
        self.spawn(spec, body).await
    }

    pub async fn stop(&self, key: &str) -> Option<WorkerDto> {
        self.halt(key).await;
        let snapshot = {
            let mut slots = self.inner.slots();
            let slot = slots.get_mut(key)?;
            slot.state.status = WorkerStatus::Stopped;
            slot.state.retry_at = None;
            slot.state.clone()
        };
        self.inner.notify(&snapshot);
        Some(snapshot)
    }

    pub async fn remove(&self, key: &str) -> bool {
        self.halt(key).await;
        self.inner.slots().remove(key).is_some()
    }

    pub async fn dispose(&self) {
        let handles: Vec<JoinHandle<()>> = self
            .inner
            .slots()
            .values_mut()
            .filter_map(|slot| {
                slot.cancel.cancel();
                slot.handle.take()
            })
            .collect();
        join_all(handles.into_iter().map(finish)).await;
    }

    async fn halt(&self, key: &str) {
        let handle = self.inner.slots().get_mut(key).and_then(|slot| {
            slot.cancel.cancel();
            slot.handle.take()
        });
        if let Some(handle) = handle {
            finish(handle).await;
        }
    }
}

async fn finish(handle: JoinHandle<()>) {
    let abort = handle.abort_handle();
    if tokio::time::timeout(STOP_GRACE + Duration::from_secs(1), handle)
        .await
        .is_err()
    {
        abort.abort();
    }
}

async fn attempt(
    body: &WorkerBody,
    context: WorkerContext,
    cancel: &CancellationToken,
) -> Option<Result<(), WorkerFault>> {
    let mut task = tokio::spawn(body(context));
    tokio::select! {
        joined = &mut task => Some(match joined {
            Ok(result) => result,
            Err(error) if error.is_panic() => Err(WorkerFault::retry(AppError::new(
                AppErrorCode::Unknown,
                WORKER_PANICKED,
            ))),
            Err(_) => Ok(()),
        }),
        () = async {
            cancel.cancelled().await;
            tokio::time::sleep(STOP_GRACE).await;
        } => {
            task.abort();
            None
        }
    }
}

async fn supervise(
    inner: Arc<Inner>,
    key: String,
    generation: u64,
    cancel: CancellationToken,
    body: WorkerBody,
    backoff: Backoff,
) {
    let mut failures: u32 = 0;
    while !cancel.is_cancelled() {
        inner.update(&key, generation, |slot, now| {
            slot.state.status = WorkerStatus::Starting;
            slot.state.started_at = Some(now);
            slot.state.retry_at = None;
            slot.state.detail = None;
            slot.ready_at = None;
        });
        let context = WorkerContext {
            key: key.clone(),
            generation,
            cancel: cancel.clone(),
            inner: Arc::clone(&inner),
        };
        let Some(result) = attempt(&body, context, &cancel).await else {
            break;
        };
        if cancel.is_cancelled() {
            break;
        }
        let fault = match result {
            Ok(()) => {
                inner.update(&key, generation, |slot, _| {
                    slot.state.status = WorkerStatus::Stopped;
                });
                return;
            }
            Err(fault) => fault,
        };
        inner.logger.log(
            LogLevel::Warn,
            "workers",
            "A background worker failed",
            &[
                ("key", json!(key)),
                ("permanent", json!(fault.permanent)),
                ("error", json!(fault.error.message)),
            ],
        );
        if fault.permanent {
            inner.update(&key, generation, |slot, _| {
                slot.state.status = WorkerStatus::Failed;
                slot.state.last_error = Some(fault.error.message.clone());
            });
            return;
        }
        let settled = inner
            .slots()
            .get(&key)
            .filter(|slot| slot.generation == generation)
            .and_then(|slot| slot.ready_at)
            .is_some_and(|ready| ready.elapsed() >= backoff.stable);
        failures = if settled { 1 } else { failures + 1 };
        let delay = backoff.delay(failures);
        inner.update(&key, generation, |slot, now| {
            slot.state.status = WorkerStatus::Retrying;
            slot.state.restarts += 1;
            slot.state.last_error = Some(fault.error.message.clone());
            slot.state.retry_at = Some(now.saturating_add(millis(delay)));
        });
        tokio::select! {
            () = cancel.cancelled() => break,
            () = tokio::time::sleep(delay) => {}
        }
    }
    inner.update(&key, generation, |slot, _| {
        slot.state.status = WorkerStatus::Stopped;
        slot.state.retry_at = None;
    });
}

#[cfg(test)]
mod tests;
