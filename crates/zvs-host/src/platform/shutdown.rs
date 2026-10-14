use std::future::Future;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use futures::FutureExt;
use futures::future::{BoxFuture, join_all};
use serde_json::json;

use super::logger::{LogLevel, SharedLogger};

pub const DEFAULT_SHUTDOWN_CAP: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShutdownStage {
    Stop,
    Drain,
    Dispose,
}

impl ShutdownStage {
    pub const ALL: &[Self] = &[Self::Stop, Self::Drain, Self::Dispose];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Drain => "drain",
            Self::Dispose => "dispose",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitDecision {
    Begin,
    Wait,
    Exit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownReport {
    pub finished: Vec<&'static str>,
    pub timed_out: bool,
}

type ShutdownTask = Box<dyn FnOnce() -> BoxFuture<'static, ()> + Send>;

struct Registered {
    stage: ShutdownStage,
    name: &'static str,
    task: ShutdownTask,
}

const IDLE: u8 = 0;
const RUNNING: u8 = 1;
const DONE: u8 = 2;

pub struct ShutdownCoordinator {
    logger: SharedLogger,
    cap: Duration,
    tasks: Mutex<Vec<Registered>>,
    finished: Mutex<Vec<&'static str>>,
    state: AtomicU8,
}

impl ShutdownCoordinator {
    pub fn new(logger: SharedLogger, cap: Duration) -> Self {
        Self {
            logger,
            cap,
            tasks: Mutex::new(Vec::new()),
            finished: Mutex::new(Vec::new()),
            state: AtomicU8::new(IDLE),
        }
    }

    pub fn register<F, Fut>(&self, stage: ShutdownStage, name: &'static str, task: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.tasks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Registered {
                stage,
                name,
                task: Box::new(move || task().boxed()),
            });
    }

    pub fn registered(&self) -> Vec<(ShutdownStage, &'static str)> {
        let mut tasks: Vec<_> = self
            .tasks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|registered| (registered.stage, registered.name))
            .collect();
        tasks.sort_by_key(|(stage, _)| *stage);
        tasks
    }

    pub fn request_exit(&self) -> ExitDecision {
        match self
            .state
            .compare_exchange(IDLE, RUNNING, Ordering::SeqCst, Ordering::SeqCst)
        {
            Ok(_) => ExitDecision::Begin,
            Err(RUNNING) => ExitDecision::Wait,
            Err(_) => ExitDecision::Exit,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.state.load(Ordering::SeqCst) == DONE
    }

    pub async fn run(&self) -> ShutdownReport {
        self.state.store(RUNNING, Ordering::SeqCst);
        let tasks = std::mem::take(&mut *self.tasks.lock().unwrap_or_else(PoisonError::into_inner));
        self.logger.log(
            LogLevel::Info,
            "host",
            "Shutdown started",
            &[("tasks", json!(tasks.len()))],
        );
        let timed_out = tokio::time::timeout(self.cap, self.run_stages(tasks))
            .await
            .is_err();
        if timed_out {
            self.logger.log(
                LogLevel::Warn,
                "host",
                "Shutdown hit its time limit",
                &[("limitMs", json!(self.cap.as_millis()))],
            );
        }
        let finished = self
            .finished
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        self.logger.log(
            LogLevel::Info,
            "host",
            "Shutdown finished",
            &[("timedOut", json!(timed_out))],
        );
        self.state.store(DONE, Ordering::SeqCst);
        ShutdownReport {
            finished,
            timed_out,
        }
    }

    async fn run_stages(&self, mut tasks: Vec<Registered>) {
        for stage in ShutdownStage::ALL {
            let (current, rest): (Vec<_>, Vec<_>) =
                tasks.into_iter().partition(|task| task.stage == *stage);
            tasks = rest;
            if current.is_empty() {
                continue;
            }
            let names: Vec<&str> = current.iter().map(|task| task.name).collect();
            self.logger.log(
                LogLevel::Info,
                "host",
                "Shutdown stage started",
                &[("stage", json!(stage.as_str())), ("tasks", json!(names))],
            );
            join_all(current.into_iter().map(|registered| {
                let run = (registered.task)();
                async move {
                    run.await;
                    self.finished
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(registered.name);
                }
            }))
            .await;
            self.logger.log(
                LogLevel::Info,
                "host",
                "Shutdown stage finished",
                &[("stage", json!(stage.as_str()))],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::testing::MemoryLogger;

    fn recorder() -> Arc<Mutex<Vec<String>>> {
        Arc::new(Mutex::new(Vec::new()))
    }

    fn push(log: &Arc<Mutex<Vec<String>>>, entry: &str) {
        log.lock().expect("lock").push(entry.to_owned());
    }

    #[tokio::test(start_paused = true)]
    async fn stages_run_in_order_and_tasks_within_a_stage_run_together() {
        let logger = MemoryLogger::shared();
        let coordinator = ShutdownCoordinator::new(logger.clone(), DEFAULT_SHUTDOWN_CAP);
        let log = recorder();
        for (stage, name, delay) in [
            (ShutdownStage::Dispose, "providers", 30),
            (ShutdownStage::Stop, "health-check.stop", 0),
            (ShutdownStage::Dispose, "health-check", 10),
            (ShutdownStage::Drain, "runs", 5),
        ] {
            let log = Arc::clone(&log);
            coordinator.register(stage, name, move || async move {
                push(&log, &format!("start {name}"));
                tokio::time::sleep(Duration::from_millis(delay)).await;
                push(&log, &format!("end {name}"));
            });
        }
        assert_eq!(
            coordinator.registered(),
            [
                (ShutdownStage::Stop, "health-check.stop"),
                (ShutdownStage::Drain, "runs"),
                (ShutdownStage::Dispose, "providers"),
                (ShutdownStage::Dispose, "health-check"),
            ]
        );
        assert_eq!(coordinator.request_exit(), ExitDecision::Begin);
        assert_eq!(coordinator.request_exit(), ExitDecision::Wait);
        let report = coordinator.run().await;
        assert!(!report.timed_out);
        assert_eq!(
            report.finished,
            ["health-check.stop", "runs", "health-check", "providers"]
        );
        assert_eq!(
            *log.lock().expect("lock"),
            [
                "start health-check.stop",
                "end health-check.stop",
                "start runs",
                "end runs",
                "start providers",
                "start health-check",
                "end health-check",
                "end providers",
            ]
        );
        assert!(coordinator.is_finished());
        assert_eq!(coordinator.request_exit(), ExitDecision::Exit);
        let stages: Vec<_> = logger
            .matching("Shutdown stage started")
            .into_iter()
            .map(|entry| entry.fields["stage"].clone())
            .collect();
        assert_eq!(stages, [json!("stop"), json!("drain"), json!("dispose")]);
        assert_eq!(logger.matching("Shutdown finished").len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_stuck_task_is_cut_off_by_the_cap_and_the_exit_still_goes_ahead() {
        let logger = MemoryLogger::shared();
        let coordinator = ShutdownCoordinator::new(logger.clone(), Duration::from_secs(2));
        coordinator.register(ShutdownStage::Stop, "quick", || async {});
        coordinator.register(ShutdownStage::Dispose, "stuck", std::future::pending);
        let report = coordinator.run().await;
        assert!(report.timed_out);
        assert_eq!(report.finished, ["quick"]);
        assert_eq!(logger.matching("Shutdown hit its time limit").len(), 1);
        assert_eq!(coordinator.request_exit(), ExitDecision::Exit);
    }
}
