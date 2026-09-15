use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc::UnboundedSender;
use zvs_core::{CancellationToken, Error, Result};

use crate::{
    document::{DOCUMENT_JOB, extract},
    download::{DOWNLOAD_JOB, download},
    protocol::Response,
};

pub const SLEEP_JOB: &str = "job.sleep";
pub const MAX_SLEEP_STEPS: u64 = 100_000;
pub const MAX_INTERVAL_MS: u64 = 60_000;

pub struct JobContext {
    id: String,
    sender: UnboundedSender<Response>,
    token: CancellationToken,
    interval: Duration,
    reported: Mutex<Option<Instant>>,
}

impl JobContext {
    pub fn new(
        id: impl Into<String>,
        sender: UnboundedSender<Response>,
        token: CancellationToken,
        interval: Duration,
    ) -> Self {
        Self {
            id: id.into(),
            sender,
            token,
            interval,
            reported: Mutex::new(None),
        }
    }

    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    /// Reports progress, dropping reports that arrive faster than the sidecar's rate limit.
    /// The first and the last report of a job are always delivered.
    pub fn progress(&self, done: u64, total: u64, message: Option<String>) {
        self.progress_at(done, total, None, message);
    }

    /// Reports progress alongside a smoothed transfer rate in bytes per second.
    pub fn progress_at(&self, done: u64, total: u64, rate: Option<f64>, message: Option<String>) {
        let now = Instant::now();
        {
            let mut reported = self
                .reported
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if done > 0
                && done < total
                && reported.is_some_and(|at| now.duration_since(at) < self.interval)
            {
                return;
            }
            *reported = Some(now);
        }
        let _ = self.sender.send(Response::JobProgress {
            id: self.id.clone(),
            done,
            total,
            rate,
            message,
        });
    }
}

pub async fn dispatch(job: &str, params: Value, context: &JobContext) -> Result<Value> {
    match job {
        SLEEP_JOB => sleep(params, context).await,
        DOWNLOAD_JOB => download(params, context).await,
        DOCUMENT_JOB => extract(params, context).await,
        unknown => Err(Error::InvalidInput(format!("Unknown job: {unknown}"))),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SleepParams {
    steps: u64,
    #[serde(default = "default_interval_ms")]
    interval_ms: u64,
}

fn default_interval_ms() -> u64 {
    100
}

async fn sleep(params: Value, context: &JobContext) -> Result<Value> {
    let params: SleepParams = serde_json::from_value(params)
        .map_err(|error| Error::InvalidInput(format!("Invalid {SLEEP_JOB} parameters: {error}")))?;
    if params.steps == 0 || params.steps > MAX_SLEEP_STEPS {
        return Err(Error::InvalidInput(format!(
            "{SLEEP_JOB} runs between 1 and {MAX_SLEEP_STEPS} steps"
        )));
    }
    if params.interval_ms > MAX_INTERVAL_MS {
        return Err(Error::InvalidInput(format!(
            "{SLEEP_JOB} steps are at most {MAX_INTERVAL_MS} ms apart"
        )));
    }
    let step = Duration::from_millis(params.interval_ms);
    context.progress(0, params.steps, None);
    for done in 1..=params.steps {
        tokio::select! {
            () = context.token().cancelled() => return Err(Error::Cancelled),
            () = tokio::time::sleep(step) => {}
        }
        context.progress(done, params.steps, None);
    }
    Ok(json!({ "steps": params.steps }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;

    fn context(interval: Duration) -> (JobContext, tokio::sync::mpsc::UnboundedReceiver<Response>) {
        let (sender, receiver) = unbounded_channel();
        (
            JobContext::new("a", sender, CancellationToken::new(), interval),
            receiver,
        )
    }

    #[tokio::test]
    async fn the_demo_job_counts_to_the_requested_step_count() {
        let (context, mut receiver) = context(Duration::ZERO);
        let result = dispatch(SLEEP_JOB, json!({ "steps": 3, "intervalMs": 0 }), &context)
            .await
            .unwrap();
        assert_eq!(result, json!({ "steps": 3 }));
        let mut seen = Vec::new();
        while let Ok(Response::JobProgress { done, total, .. }) = receiver.try_recv() {
            seen.push((done, total));
        }
        assert_eq!(seen, [(0, 3), (1, 3), (2, 3), (3, 3)]);
    }

    #[tokio::test]
    async fn progress_is_rate_limited_but_keeps_the_first_and_last_report() {
        let (context, mut receiver) = context(Duration::from_secs(60));
        dispatch(SLEEP_JOB, json!({ "steps": 4, "intervalMs": 0 }), &context)
            .await
            .unwrap();
        let mut seen = Vec::new();
        while let Ok(Response::JobProgress { done, .. }) = receiver.try_recv() {
            seen.push(done);
        }
        assert_eq!(seen, [0, 4]);
    }

    #[tokio::test]
    async fn cancellation_and_bad_parameters_are_typed_errors() {
        let (context, _receiver) = context(Duration::ZERO);
        context.token().cancel();
        let cancelled = dispatch(SLEEP_JOB, json!({ "steps": 2 }), &context)
            .await
            .unwrap_err();
        assert_eq!(cancelled.code(), "RUN_CANCELLED");
        for params in [
            json!({ "steps": 0 }),
            json!({}),
            json!({ "steps": 1, "nope": 1 }),
        ] {
            let error = dispatch(SLEEP_JOB, params, &context).await.unwrap_err();
            assert_eq!(error.code(), "VALIDATION_FAILED");
        }
        let unknown = dispatch("job.nope", Value::Null, &context)
            .await
            .unwrap_err();
        assert_eq!(unknown.code(), "VALIDATION_FAILED");
    }
}
