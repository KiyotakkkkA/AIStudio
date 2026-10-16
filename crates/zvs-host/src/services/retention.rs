use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;

use super::settings::SettingService;
use crate::data::{Database, Repositories};
use crate::platform::clock::{SharedClock, SystemClock};
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};

pub const RETENTION_DAYS_KEY: &str = "runs.retention.days";
pub const DEFAULT_RETENTION_DAYS: i64 = 30;
pub const RETENTION_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
pub const RETENTION_BATCH: i64 = 200;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

pub struct RetentionServiceOptions {
    pub database: Arc<Database>,
    pub settings: Option<SettingService>,
    pub logger: SharedLogger,
    pub interval: Duration,
    pub batch: i64,
    pub days: i64,
    pub clock: SharedClock,
    pub runtime: Option<Handle>,
}

impl RetentionServiceOptions {
    pub fn new(database: Arc<Database>) -> Self {
        Self {
            database,
            settings: None,
            logger: silent_logger(),
            interval: RETENTION_INTERVAL,
            batch: RETENTION_BATCH,
            days: DEFAULT_RETENTION_DAYS,
            clock: Arc::new(SystemClock),
            runtime: None,
        }
    }
}

struct Inner {
    options: RetentionServiceOptions,
    timer: Mutex<Option<CancellationToken>>,
}

#[derive(Clone)]
pub struct RetentionService {
    inner: Arc<Inner>,
}

impl RetentionService {
    pub fn new(options: RetentionServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                timer: Mutex::new(None),
            }),
        }
    }

    pub fn days(&self) -> i64 {
        let stored = self
            .inner
            .options
            .settings
            .as_ref()
            .and_then(|settings| settings.get(RETENTION_DAYS_KEY).ok().flatten())
            .map(|setting| setting.value);
        match stored.as_ref().and_then(Value::as_f64) {
            #[allow(clippy::cast_possible_truncation)]
            Some(days) if days.is_finite() && days > 0.0 => days.floor() as i64,
            _ => self.inner.options.days,
        }
    }

    pub fn start(&self) {
        let token = {
            let mut timer = self
                .inner
                .timer
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if timer.is_some() {
                return;
            }
            let token = CancellationToken::new();
            *timer = Some(token.clone());
            token
        };
        let service = self.clone();
        let interval = self.inner.options.interval;
        let work = async move {
            loop {
                tokio::select! {
                    () = token.cancelled() => return,
                    () = tokio::time::sleep(interval) => {}
                }
                service.sweep();
            }
        };
        match &self.inner.options.runtime {
            Some(runtime) => drop(runtime.spawn(work)),
            None => drop(tokio::spawn(work)),
        }
        self.sweep();
    }

    pub fn stop(&self) {
        if let Some(token) = self
            .inner
            .timer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            token.cancel();
        }
    }

    pub fn sweep(&self) -> usize {
        let options = &self.inner.options;
        let days = self.days();
        let now = options.clock.now();
        let before = now - days.saturating_mul(DAY_MS);
        let pruned = options.database.transaction(|transaction| {
            let runs = Repositories::new(transaction).runs;
            let ids = runs.prunable(before, options.batch)?;
            runs.drop_payloads(&ids, now)?;
            Ok(ids.len())
        });
        match pruned {
            Ok(pruned) => {
                if pruned > 0 {
                    options.logger.log(
                        LogLevel::Info,
                        "runs",
                        "Pruned step payloads past retention",
                        &[("runs", json!(pruned)), ("days", json!(days))],
                    );
                }
                pruned
            }
            Err(error) => {
                options.logger.log(
                    LogLevel::Error,
                    "runs",
                    "Retention sweep failed",
                    &[("error", json!(error.to_string()))],
                );
                0
            }
        }
    }
}
