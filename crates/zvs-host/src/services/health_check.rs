use std::future::ready;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use futures::FutureExt;
use futures::future::{BoxFuture, Shared};
use serde_json::{Value, json};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior, interval_at};
use tokio_util::sync::CancellationToken;
use zvs_contract::{HostEventDraft, ProviderListFilter, RunOutcomeDto, RunOutcomeStatus};

use super::providers::{ProbeInput, ProviderService};
use super::settings::SettingService;
use crate::platform::clock::SharedClock;
use crate::platform::events::EventBus;
use crate::platform::logger::{LogLevel, SharedLogger};

pub const HEALTH_CHECK_ENABLED_KEY: &str = "providers.healthcheck.enabled";
pub const HEALTH_CHECK_INTERVAL_KEY: &str = "providers.healthcheck.minutes";
pub const DEFAULT_HEALTH_CHECK_INTERVAL_MINUTES: u64 = 15;
pub const MIN_HEALTH_CHECK_INTERVAL_MINUTES: u64 = 1;
pub const HEALTH_CHECK_ENABLED_BY_DEFAULT: bool = false;

pub fn is_health_check_key(key: &str) -> bool {
    key == HEALTH_CHECK_ENABLED_KEY || key == HEALTH_CHECK_INTERVAL_KEY
}

pub struct HealthCheckOptions {
    pub providers: ProviderService,
    pub settings: SettingService,
    pub logger: SharedLogger,
    pub clock: SharedClock,
    pub events: Option<EventBus>,
    pub runtime: Option<Handle>,
}

type SharedSweep = Shared<BoxFuture<'static, ()>>;

#[derive(Default)]
struct HealthState {
    ticker: Option<JoinHandle<()>>,
    sweep: Option<SharedSweep>,
    generation: u64,
    controller: Option<CancellationToken>,
}

struct HealthInner {
    providers: ProviderService,
    settings: SettingService,
    logger: SharedLogger,
    clock: SharedClock,
    events: Option<EventBus>,
    runtime: Option<Handle>,
    state: Mutex<HealthState>,
}

#[derive(Clone)]
pub struct HealthCheckService {
    inner: Arc<HealthInner>,
}

struct SweepSettle(Arc<HealthInner>);

impl Drop for SweepSettle {
    fn drop(&mut self) {
        let mut state = self.0.state();
        state.sweep = None;
        state.controller = None;
    }
}

impl HealthCheckService {
    pub fn new(options: HealthCheckOptions) -> Self {
        Self {
            inner: Arc::new(HealthInner {
                providers: options.providers,
                settings: options.settings,
                logger: options.logger,
                clock: options.clock,
                events: options.events,
                runtime: options.runtime,
                state: Mutex::new(HealthState::default()),
            }),
        }
    }

    pub fn running(&self) -> bool {
        self.inner.state().ticker.is_some()
    }

    pub fn enabled(&self) -> bool {
        self.inner.enabled()
    }

    pub fn interval_minutes(&self) -> u64 {
        self.inner.interval_minutes()
    }

    pub fn start(&self) -> bool {
        let inner = &self.inner;
        if self.running() {
            return true;
        }
        if !inner.enabled() {
            inner.logger.log(
                LogLevel::Info,
                "providers",
                "Background health check stays off",
                &[("setting", json!(HEALTH_CHECK_ENABLED_KEY))],
            );
            return false;
        }
        let Some(runtime) = inner.runtime() else {
            inner.logger.log(
                LogLevel::Warn,
                "providers",
                "Background health check has no runtime to run on",
                &[],
            );
            return false;
        };
        let minutes = inner.interval_minutes();
        let period = Duration::from_secs(minutes.saturating_mul(60));
        let weak = Arc::downgrade(inner);
        let ticker = runtime.spawn(tick(weak, period));
        let mut state = inner.state();
        if state.ticker.is_some() {
            ticker.abort();
            return true;
        }
        state.ticker = Some(ticker);
        drop(state);
        inner.logger.log(
            LogLevel::Info,
            "providers",
            "Started the background health check",
            &[("minutes", json!(minutes))],
        );
        true
    }

    pub fn stop(&self) {
        let (ticker, controller) = {
            let mut state = self.inner.state();
            state.generation += 1;
            (state.ticker.take(), state.controller.clone())
        };
        if let Some(controller) = controller {
            controller.cancel();
        }
        let Some(ticker) = ticker else {
            return;
        };
        ticker.abort();
        self.inner.logger.log(
            LogLevel::Info,
            "providers",
            "Stopped the background health check",
            &[],
        );
    }

    pub async fn dispose(&self) {
        self.stop();
        let sweep = self.inner.state().sweep.clone();
        if let Some(sweep) = sweep {
            sweep.await;
        }
    }

    pub fn run_once(&self) -> BoxFuture<'static, ()> {
        run_once(&self.inner)
    }
}

async fn tick(weak: Weak<HealthInner>, period: Duration) {
    let mut ticks = interval_at(Instant::now() + period, period);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        let Some(inner) = weak.upgrade() else {
            return;
        };
        drop(run_once(&inner));
    }
}

fn run_once(inner: &Arc<HealthInner>) -> BoxFuture<'static, ()> {
    let mut state = inner.state();
    if let Some(sweep) = &state.sweep {
        return sweep.clone().boxed();
    }
    let Some(runtime) = inner.runtime() else {
        return ready(()).boxed();
    };
    let generation = state.generation;
    let controller = CancellationToken::new();
    state.controller = Some(controller.clone());
    let settle = SweepSettle(Arc::clone(inner));
    let task = runtime.spawn(async move {
        settle.0.sweep(generation, controller).await;
        drop(settle);
    });
    let sweep = async move {
        let _ = task.await;
    }
    .boxed()
    .shared();
    state.sweep = Some(sweep.clone());
    drop(state);
    sweep.boxed()
}

impl HealthInner {
    fn state(&self) -> MutexGuard<'_, HealthState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn runtime(&self) -> Option<Handle> {
        self.runtime.clone().or_else(|| Handle::try_current().ok())
    }

    fn setting(&self, key: &str) -> Option<Value> {
        self.settings
            .get(key)
            .ok()
            .flatten()
            .map(|setting| setting.value)
    }

    fn enabled(&self) -> bool {
        self.setting(HEALTH_CHECK_ENABLED_KEY)
            .and_then(|value| value.as_bool())
            .unwrap_or(HEALTH_CHECK_ENABLED_BY_DEFAULT)
    }

    fn interval_minutes(&self) -> u64 {
        let Some(minutes) = self
            .setting(HEALTH_CHECK_INTERVAL_KEY)
            .and_then(|value| value.as_f64())
            .filter(|value| value.is_finite())
        else {
            return DEFAULT_HEALTH_CHECK_INTERVAL_MINUTES;
        };
        let rounded = (minutes + 0.5).floor();
        if rounded < MIN_HEALTH_CHECK_INTERVAL_MINUTES as f64 {
            MIN_HEALTH_CHECK_INTERVAL_MINUTES
        } else {
            rounded as u64
        }
    }

    fn generation(&self) -> u64 {
        self.state().generation
    }

    async fn sweep(&self, generation: u64, controller: CancellationToken) {
        if !self.enabled() {
            return;
        }
        let providers = &self.providers;
        let Ok(listed) = providers.list(&ProviderListFilter {
            capability: None,
            enabled: Some(true),
        }) else {
            self.logger.log(
                LogLevel::Warn,
                "providers",
                "Health check sweep stopped early",
                &[
                    ("done", json!(0)),
                    ("total", json!(0)),
                    ("failed", json!(true)),
                ],
            );
            return;
        };
        let targets: Vec<String> = listed
            .into_iter()
            .map(|provider| provider.id.to_string())
            .filter(|id| !providers.is_probing(id))
            .collect();
        if targets.is_empty() {
            return;
        }
        let stream = self
            .events
            .as_ref()
            .and_then(|events| events.open_stream().ok());
        let total = targets.len() as u64;
        let mut done = 0_u64;
        let mut failed = false;
        for id in targets {
            if generation != self.generation() || !self.enabled() {
                break;
            }
            if providers.is_probing(&id) {
                continue;
            }
            if providers
                .probe(ProbeInput::Saved(id), Some(controller.clone()))
                .await
                .is_err()
            {
                failed = true;
                break;
            }
            done += 1;
            if let Some(stream) = &stream {
                stream.emit(HostEventDraft::Progress { done, total });
            }
        }
        if failed {
            self.logger.log(
                LogLevel::Warn,
                "providers",
                "Health check sweep stopped early",
                &[
                    ("done", json!(done)),
                    ("total", json!(total)),
                    ("failed", json!(true)),
                ],
            );
        }
        if let Some(stream) = &stream {
            stream.end(RunOutcomeDto {
                status: if failed {
                    RunOutcomeStatus::Failed
                } else {
                    RunOutcomeStatus::Ok
                },
                code: None,
                message: None,
            });
        }
        self.logger.log(
            LogLevel::Debug,
            "providers",
            "Health check sweep finished",
            &[("done", json!(done)), ("at", json!(self.clock.now()))],
        );
    }
}

#[cfg(test)]
mod tests;
