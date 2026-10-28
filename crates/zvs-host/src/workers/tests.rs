use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures::FutureExt;
use zvs_contract::{AppError, AppErrorCode, WorkerStatus};

use super::*;
use crate::platform::clock::FakeClock;

fn supervisor() -> WorkerSupervisor {
    WorkerSupervisor::new(Arc::new(FakeClock::default()), silent_logger())
}

fn fast() -> Backoff {
    Backoff {
        initial: Duration::from_secs(1),
        max: Duration::from_secs(8),
        stable: Duration::from_secs(30),
    }
}

fn steady() -> WorkerBody {
    Arc::new(|context: WorkerContext| {
        async move {
            context.ready();
            context.cancelled().await;
            Ok(())
        }
        .boxed()
    })
}

fn failing(attempts: Arc<AtomicUsize>) -> WorkerBody {
    Arc::new(move |_context: WorkerContext| {
        let attempts = Arc::clone(&attempts);
        async move {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err(WorkerFault::retry(AppError::new(
                AppErrorCode::ProviderUnreachable,
                "Сервер недоступен",
            )))
        }
        .boxed()
    })
}

async fn settle() {
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
}

fn status(workers: &WorkerSupervisor, key: &str) -> WorkerStatus {
    workers.get(key).expect("the worker is listed").status
}

#[test]
fn the_backoff_doubles_up_to_its_cap() {
    let backoff = fast();
    assert_eq!(backoff.delay(1), Duration::from_secs(1));
    assert_eq!(backoff.delay(2), Duration::from_secs(2));
    assert_eq!(backoff.delay(3), Duration::from_secs(4));
    assert_eq!(backoff.delay(10), Duration::from_secs(8));
    assert_eq!(backoff.delay(u32::MAX), Duration::from_secs(8));
}

#[tokio::test(start_paused = true)]
async fn a_worker_that_signals_ready_is_running() {
    let workers = supervisor();
    workers
        .spawn(WorkerSpec::new("test.steady", "Стабильный"), steady())
        .await
        .expect("spawns");
    settle().await;
    let worker = workers.get("test.steady").expect("listed");
    assert_eq!(worker.status, WorkerStatus::Running);
    assert!(worker.started_at.is_some());
    assert_eq!(worker.restarts, 0);
}

#[tokio::test(start_paused = true)]
async fn a_failing_worker_restarts_with_a_growing_delay() {
    let workers = supervisor();
    let attempts = Arc::new(AtomicUsize::new(0));
    workers
        .spawn(
            WorkerSpec::new("test.failing", "Сбойный").with_backoff(fast()),
            failing(Arc::clone(&attempts)),
        )
        .await
        .expect("spawns");
    settle().await;
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    let worker = workers.get("test.failing").expect("listed");
    assert_eq!(worker.status, WorkerStatus::Retrying);
    assert_eq!(worker.last_error.as_deref(), Some("Сервер недоступен"));
    assert!(worker.retry_at.is_some());

    tokio::time::sleep(Duration::from_millis(1_100)).await;
    settle().await;
    assert_eq!(attempts.load(Ordering::SeqCst), 2);

    tokio::time::sleep(Duration::from_millis(1_100)).await;
    settle().await;
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        2,
        "the second delay is two seconds"
    );

    tokio::time::sleep(Duration::from_millis(1_000)).await;
    settle().await;
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    assert_eq!(workers.get("test.failing").expect("listed").restarts, 3);
}

#[tokio::test(start_paused = true)]
async fn one_failing_worker_does_not_block_the_others() {
    let workers = supervisor();
    let attempts = Arc::new(AtomicUsize::new(0));
    workers
        .spawn(
            WorkerSpec::new("test.failing", "Сбойный").with_backoff(fast()),
            failing(Arc::clone(&attempts)),
        )
        .await
        .expect("spawns");
    workers
        .spawn(WorkerSpec::new("test.steady", "Стабильный"), steady())
        .await
        .expect("spawns");
    tokio::time::sleep(Duration::from_secs(5)).await;
    settle().await;
    assert!(attempts.load(Ordering::SeqCst) >= 2);
    assert_eq!(status(&workers, "test.steady"), WorkerStatus::Running);
    assert_ne!(status(&workers, "test.failing"), WorkerStatus::Running);
}

#[tokio::test(start_paused = true)]
async fn a_panicking_worker_is_restarted_like_a_failing_one() {
    let workers = supervisor();
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&attempts);
    let body: WorkerBody = Arc::new(move |_context: WorkerContext| {
        let counted = Arc::clone(&counted);
        async move {
            counted.fetch_add(1, Ordering::SeqCst);
            panic!("the worker panics");
        }
        .boxed()
    });
    workers
        .spawn(
            WorkerSpec::new("test.panic", "Аварийный").with_backoff(fast()),
            body,
        )
        .await
        .expect("spawns");
    settle().await;
    let worker = workers.get("test.panic").expect("listed");
    assert_eq!(worker.status, WorkerStatus::Retrying);
    assert_eq!(worker.last_error.as_deref(), Some(WORKER_PANICKED));
    tokio::time::sleep(Duration::from_millis(1_100)).await;
    settle().await;
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn a_permanent_fault_stops_retrying() {
    let workers = supervisor();
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&attempts);
    let body: WorkerBody = Arc::new(move |_context: WorkerContext| {
        let counted = Arc::clone(&counted);
        async move {
            counted.fetch_add(1, Ordering::SeqCst);
            Err(WorkerFault::permanent(AppError::new(
                AppErrorCode::SecretMissing,
                "Секрет не найден",
            )))
        }
        .boxed()
    });
    workers
        .spawn(WorkerSpec::new("test.permanent", "Неисправимый"), body)
        .await
        .expect("spawns");
    tokio::time::sleep(Duration::from_secs(60)).await;
    settle().await;
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    let worker = workers.get("test.permanent").expect("listed");
    assert_eq!(worker.status, WorkerStatus::Failed);
    assert_eq!(worker.last_error.as_deref(), Some("Секрет не найден"));
}

#[tokio::test(start_paused = true)]
async fn stop_cancels_the_worker_and_restart_brings_it_back() {
    let workers = supervisor();
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&attempts);
    let body: WorkerBody = Arc::new(move |context: WorkerContext| {
        let counted = Arc::clone(&counted);
        async move {
            counted.fetch_add(1, Ordering::SeqCst);
            context.ready();
            context.cancelled().await;
            Ok(())
        }
        .boxed()
    });
    workers
        .spawn(WorkerSpec::new("test.stop", "Останавливаемый"), body)
        .await
        .expect("spawns");
    settle().await;
    let stopped = workers.stop("test.stop").await.expect("listed");
    assert_eq!(stopped.status, WorkerStatus::Stopped);

    let restarted = workers.restart("test.stop").await.expect("restarts");
    assert_eq!(restarted.status, WorkerStatus::Starting);
    settle().await;
    assert_eq!(status(&workers, "test.stop"), WorkerStatus::Running);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);

    assert!(workers.remove("test.stop").await);
    assert!(workers.get("test.stop").is_none());
    assert_eq!(
        workers
            .restart("test.stop")
            .await
            .expect_err("removed")
            .code,
        AppErrorCode::NotFound
    );
}

#[tokio::test(start_paused = true)]
async fn a_worker_that_ignores_cancellation_is_aborted_after_the_grace_period() {
    let workers = supervisor();
    let body: WorkerBody = Arc::new(|context: WorkerContext| {
        async move {
            context.ready();
            std::future::pending::<()>().await;
            Ok(())
        }
        .boxed()
    });
    workers
        .spawn(WorkerSpec::new("test.stubborn", "Упрямый"), body)
        .await
        .expect("spawns");
    settle().await;
    let stopped = workers.stop("test.stubborn").await.expect("listed");
    assert_eq!(stopped.status, WorkerStatus::Stopped);
}

#[tokio::test(start_paused = true)]
async fn dispose_stops_every_worker_and_listeners_see_each_change() {
    let workers = supervisor();
    let seen = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&seen);
    workers.watch(Arc::new(move |_worker| {
        counted.fetch_add(1, Ordering::SeqCst);
    }));
    for key in ["test.one", "test.two"] {
        workers
            .spawn(WorkerSpec::new(key, "Фоновый"), steady())
            .await
            .expect("spawns");
    }
    settle().await;
    workers.dispose().await;
    settle().await;
    assert!(
        workers
            .list()
            .iter()
            .all(|worker| worker.status == WorkerStatus::Stopped)
    );
    assert!(seen.load(Ordering::SeqCst) >= 6);
}

#[tokio::test]
async fn a_malformed_key_is_refused() {
    let error = supervisor()
        .spawn(WorkerSpec::new("Bad Key", "Плохой"), steady())
        .await
        .expect_err("refused");
    assert_eq!(error.code, AppErrorCode::ValidationFailed);
}
