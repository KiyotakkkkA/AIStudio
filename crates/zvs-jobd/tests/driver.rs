use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use zvs_contract::AppErrorCode;
use zvs_host::drivers::sidecar::{JobRunOptions, SidecarDriver, SidecarJobs, SidecarOptions};
use zvs_host::testing::MemoryLogger;

fn jobd() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_zvs-jobd"))
}

fn driver_with(
    args: &[&str],
    tune: impl FnOnce(&mut SidecarOptions),
) -> (SidecarDriver, Arc<MemoryLogger>) {
    let logger = MemoryLogger::shared();
    let mut options = SidecarOptions {
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        logger: logger.clone(),
        backoff: Duration::from_millis(10),
        cancel_grace: Duration::from_millis(150),
        ping_interval: Duration::from_secs(60),
        ping_timeout: Duration::from_millis(500),
        ..SidecarOptions::new(jobd())
    };
    tune(&mut options);
    (SidecarDriver::new(options), logger)
}

fn driver() -> (SidecarDriver, Arc<MemoryLogger>) {
    driver_with(&[], |_| {})
}

fn logged(logger: &MemoryLogger, message: &str) -> bool {
    logger
        .entries()
        .iter()
        .any(|entry| entry.message == message)
}

fn long_sleep() -> Value {
    json!({ "steps": 100_000, "intervalMs": 10 })
}

fn kill_process(pid: u32) {
    let status = if cfg!(windows) {
        std::process::Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .output()
    } else {
        std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output()
    };
    assert!(status.expect("the kill command runs").status.success());
}

#[tokio::test]
async fn concurrent_jobs_are_matched_to_their_requests_and_report_progress() {
    let (driver, logger) = driver();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = {
        let seen = Arc::clone(&seen);
        JobRunOptions {
            cancel: None,
            on_progress: Some(Arc::new(move |progress| {
                seen.lock().expect("progress").push(progress.done);
            })),
        }
    };
    let (slow, quick, third) = tokio::join!(
        driver.run(
            "job.sleep",
            json!({ "steps": 3, "intervalMs": 80 }),
            JobRunOptions::default()
        ),
        driver.run(
            "job.sleep",
            json!({ "steps": 1, "intervalMs": 0 }),
            JobRunOptions::default()
        ),
        driver.run("job.sleep", json!({ "steps": 2, "intervalMs": 10 }), sink),
    );
    assert_eq!(slow.expect("slow"), json!({ "steps": 3 }));
    assert_eq!(quick.expect("quick"), json!({ "steps": 1 }));
    assert_eq!(third.expect("third"), json!({ "steps": 2 }));
    assert_eq!(*seen.lock().expect("progress"), [0, 2]);
    assert_eq!(logger.matching("Started the sidecar").len(), 1);
    driver.dispose().await;
}

#[tokio::test]
async fn a_job_error_keeps_its_code_and_the_process() {
    let (driver, _logger) = driver();
    let error = driver
        .run("job.nope", Value::Null, JobRunOptions::default())
        .await
        .expect_err("unknown job");
    assert_eq!(error.code, AppErrorCode::ValidationFailed);
    let pid = driver.pid();
    assert!(pid.is_some());
    driver
        .run(
            "job.sleep",
            json!({ "steps": 1, "intervalMs": 0 }),
            JobRunOptions::default(),
        )
        .await
        .expect("still serving");
    assert_eq!(driver.pid(), pid);
    driver.dispose().await;
}

#[tokio::test]
async fn a_sidecar_killed_from_outside_is_restarted_on_the_next_job() {
    let (driver, logger) = driver();
    driver.ping().await.expect("answers");
    let pid = driver.pid().expect("running");
    let pending = {
        let driver = driver.clone();
        tokio::spawn(async move {
            driver
                .run("job.sleep", long_sleep(), JobRunOptions::default())
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    kill_process(pid);
    let error = pending.await.expect("joins").expect_err("abandoned");
    assert_eq!(error.code, AppErrorCode::SidecarUnavailable);
    assert!(logged(
        &logger,
        "The sidecar exited; it restarts on the next job"
    ));
    driver.ping().await.expect("a new process answers");
    assert_ne!(driver.pid(), Some(pid));
    driver.dispose().await;
}

#[tokio::test]
async fn ping_round_trips_and_a_disposed_driver_refuses_work() {
    let (driver, logger) = driver();
    driver.ping().await.expect("answers");
    let pending = {
        let driver = driver.clone();
        tokio::spawn(async move {
            driver
                .run("job.sleep", long_sleep(), JobRunOptions::default())
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    driver.dispose().await;
    assert!(driver.pid().is_none());
    let error = pending.await.expect("joins").expect_err("disposed");
    assert_eq!(error.code, AppErrorCode::SidecarUnavailable);
    let error = driver
        .run("job.sleep", Value::Null, JobRunOptions::default())
        .await
        .expect_err("refused");
    assert_eq!(error.code, AppErrorCode::Conflict);
    assert!(!logged(
        &logger,
        "The sidecar exited; it restarts on the next job"
    ));
}

#[cfg(debug_assertions)]
mod faults {
    use std::sync::atomic::{AtomicBool, Ordering};

    use tokio_util::sync::CancellationToken;

    use super::*;

    async fn until(condition: impl Fn() -> bool, label: &str) {
        for _ in 0..200 {
            if condition() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("Timed out waiting for {label}");
    }

    fn cancellable(token: &CancellationToken) -> JobRunOptions {
        JobRunOptions {
            cancel: Some(token.clone()),
            on_progress: None,
        }
    }

    #[tokio::test]
    async fn cancelling_stops_a_job_and_a_job_that_ignores_it_loses_the_process() {
        let (driver, logger) = driver();
        let polite = CancellationToken::new();
        let stopped = {
            let driver = driver.clone();
            let polite = polite.clone();
            tokio::spawn(async move {
                driver
                    .run("job.sleep", long_sleep(), cancellable(&polite))
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(100)).await;
        polite.cancel();
        let error = stopped.await.expect("joins").expect_err("cancelled");
        assert_eq!(error.code, AppErrorCode::RunCancelled);
        assert!(logged(&logger, "Cancelling a sidecar job"));
        assert!(!logged(
            &logger,
            "A job ignored cancellation; killing the sidecar"
        ));

        let stubborn = CancellationToken::new();
        let hung = {
            let driver = driver.clone();
            let stubborn = stubborn.clone();
            tokio::spawn(async move {
                driver
                    .run("debug.hang", Value::Null, cancellable(&stubborn))
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(100)).await;
        stubborn.cancel();
        let error = hung.await.expect("joins").expect_err("killed");
        assert_eq!(error.code, AppErrorCode::SidecarUnavailable);
        assert!(logged(
            &logger,
            "A job ignored cancellation; killing the sidecar"
        ));

        assert_eq!(
            driver
                .run(
                    "job.sleep",
                    json!({ "steps": 1, "intervalMs": 0 }),
                    JobRunOptions::default()
                )
                .await
                .expect("restarted"),
            json!({ "steps": 1 })
        );
        driver.dispose().await;
    }

    #[tokio::test]
    async fn a_crashed_sidecar_fails_its_jobs_is_logged_and_restarts_on_the_next_one() {
        let (driver, logger) = driver();
        let started = Arc::new(AtomicBool::new(false));
        let survivor = {
            let driver = driver.clone();
            let started = Arc::clone(&started);
            tokio::spawn(async move {
                driver
                    .run(
                        "job.sleep",
                        long_sleep(),
                        JobRunOptions {
                            cancel: None,
                            on_progress: Some(Arc::new(move |_| {
                                started.store(true, Ordering::SeqCst)
                            })),
                        },
                    )
                    .await
            })
        };
        until(|| started.load(Ordering::SeqCst), "the survivor to start").await;
        let error = driver
            .run("debug.crash", Value::Null, JobRunOptions::default())
            .await
            .expect_err("crashed");
        assert_eq!(error.code, AppErrorCode::SidecarUnavailable);
        assert!(error.message.starts_with("Sidecar завершился"));
        let error = survivor.await.expect("joins").expect_err("abandoned");
        assert_eq!(error.code, AppErrorCode::SidecarUnavailable);

        let restarts = logger.matching("The sidecar exited; it restarts on the next job");
        assert_eq!(restarts.len(), 1);
        assert_eq!(restarts[0].fields["restarts"], json!(1));
        assert_eq!(restarts[0].fields["backoffMs"], json!(10));
        assert_eq!(restarts[0].fields["abandoned"], json!(2));

        assert_eq!(
            driver
                .run(
                    "job.sleep",
                    json!({ "steps": 1, "intervalMs": 0 }),
                    JobRunOptions::default()
                )
                .await
                .expect("restarted"),
            json!({ "steps": 1 })
        );
        assert_eq!(logger.matching("Started the sidecar").len(), 2);
        driver.dispose().await;
    }

    #[tokio::test]
    async fn a_sidecar_that_stops_answering_ping_is_killed_and_replaced() {
        let (driver, logger) = driver_with(&["--deaf"], |options| {
            options.ping_interval = Duration::from_millis(50);
            options.ping_timeout = Duration::from_millis(100);
        });
        let error = driver
            .run("debug.hang", Value::Null, JobRunOptions::default())
            .await
            .expect_err("abandoned");
        assert_eq!(error.code, AppErrorCode::SidecarUnavailable);
        until(
            || logged(&logger, "The sidecar stopped answering ping; killing it"),
            "the health check to give up",
        )
        .await;
        assert!(logged(
            &logger,
            "The sidecar exited; it restarts on the next job"
        ));
        driver.dispose().await;
    }
}
