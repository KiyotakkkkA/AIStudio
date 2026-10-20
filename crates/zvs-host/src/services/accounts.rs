use std::collections::HashMap;
use std::future::{Future, ready};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use futures::FutureExt;
use futures::future::{BoxFuture, Shared, join_all};
use serde_json::{Map, Value, json};
use tokio::runtime::Handle;
use tokio::time::{Instant, MissedTickBehavior, interval_at, sleep, sleep_until};
use tokio_util::sync::CancellationToken;
use url::Url;
use zvs_contract::{
    AccountDto, AccountFamily, AccountId, AccountLinkResult, AccountStatus,
    AccountsCancelLinkOutput, AccountsUnlinkOutput, AppError, AppErrorCode, HostEventDraft,
    RunOutcomeDto, RunOutcomeStatus, SecretScope, Timestamp,
};

use super::crypto::Plaintext;
use super::secrets::{SecretCreateInput, SecretService, SecretUpdateInput};
use crate::browser::{
    BROWSER_PARTITION, BrowserLifecycle, BrowserLifecycleEvent, LifecycleSubscription,
};
use crate::data::repositories::{AccountDraft, AccountPatch};
use crate::data::schema::AccountRow;
pub use crate::data::schema::SESSION_EXPIRED_DETAIL;
use crate::data::{DataError, Database, Repositories};
use crate::drivers::ai::errors::is_session_expired;
use crate::drivers::ai::identity::{
    ACCOUNT_TOKEN_SECRET_TYPE, ProbeFactory, ProbeResult, SharedIdentityProbe, account_session,
    confirm_renewable, identity_probes, probe_renewing,
};
use crate::drivers::ai::registry::SessionFactory;
use crate::drivers::ai::stream_to_events::ok_outcome;
use crate::platform::clock::{SharedClock, SystemClock};
use crate::platform::events::EventBus;
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger, silent_logger};
use crate::platform::navigation::{SharedNavigator, WorkspaceRoute};

pub const LINK_TIMEOUT: Duration = Duration::from_secs(180);
pub const POLL_INTERVAL: Duration = Duration::from_secs(2);
pub const AUTO_REFRESH_TICK: Duration = Duration::from_secs(60);
pub const AUTO_REFRESH_SOON_MS: Timestamp = 60_000;
pub const AUTO_REFRESH_IDLE_MS: Timestamp = 15 * 60_000;
pub const EXPIRY_WINDOW_MS: Timestamp = 5 * 60_000;

pub const SIGN_IN_IN_PROGRESS: &str = "Account sign-in is in progress";
pub const SERVICE_CLOSED: &str = "Account service is closed";
pub const ACCOUNT_NOT_FOUND: &str = "Account not found";
pub const VENDOR_UNCHECKED: &str = "Could not check the vendor session";
pub const OPERATION_CANCELLED: &str = "Account operation cancelled";
pub const NO_SIGN_IN: &str = "No sign-in detected";
pub const IDENTITY_REPLACED: &str = "Previous identity replaced; only one browser session per vendor can be active. Its providers now use the new account.";
pub const DIFFERENT_ACCOUNT: &str = "Browser is signed in to a different account";
pub const COOKIES_CLEARED_DETAIL: &str = "Browser cookies cleared — sign in again";
pub const LINKING_FAILED: &str = "Account linking failed";

#[async_trait]
pub trait AccountBrowser: Send + Sync {
    async fn open_tab(&self, url: &str) -> Result<(), AppError>;
}

pub type SharedAccountBrowser = Arc<dyn AccountBrowser>;
pub type RandomSource = Arc<dyn Fn() -> f64 + Send + Sync>;

pub fn system_random() -> RandomSource {
    Arc::new(|| {
        let bits = new_id().as_u128() & ((1_u128 << 53) - 1);
        bits as f64 / (1_u64 << 53) as f64
    })
}

pub struct AccountServiceOptions {
    pub database: Arc<Database>,
    pub secrets: Arc<SecretService>,
    pub probes: ProbeFactory,
    pub sessions: SessionFactory,
    pub browser: SharedAccountBrowser,
    pub lifecycle: BrowserLifecycle,
    pub events: EventBus,
    pub navigator: SharedNavigator,
    pub clock: SharedClock,
    pub random: RandomSource,
    pub logger: SharedLogger,
    pub timeout: Duration,
    pub interval: Duration,
    pub runtime: Option<Handle>,
}

impl AccountServiceOptions {
    pub fn new(
        database: Arc<Database>,
        secrets: Arc<SecretService>,
        sessions: SessionFactory,
        browser: SharedAccountBrowser,
        lifecycle: BrowserLifecycle,
        events: EventBus,
        navigator: SharedNavigator,
    ) -> Self {
        Self {
            database,
            secrets,
            probes: identity_probes(),
            sessions,
            browser,
            lifecycle,
            events,
            navigator,
            clock: Arc::new(SystemClock),
            random: system_random(),
            logger: silent_logger(),
            timeout: LINK_TIMEOUT,
            interval: POLL_INTERVAL,
            runtime: None,
        }
    }
}

type PendingFuture<T> = Shared<BoxFuture<'static, Result<T, AppError>>>;

struct Pending<T: Clone> {
    id: u64,
    cancel: CancellationToken,
    result: PendingFuture<T>,
}

struct Inner {
    database: Arc<Database>,
    secrets: Arc<SecretService>,
    probes: ProbeFactory,
    sessions: SessionFactory,
    browser: SharedAccountBrowser,
    lifecycle: BrowserLifecycle,
    events: EventBus,
    navigator: SharedNavigator,
    clock: SharedClock,
    random: RandomSource,
    logger: SharedLogger,
    timeout: Duration,
    interval: Duration,
    runtime: Option<Handle>,
    links: Mutex<HashMap<AccountFamily, Pending<AccountLinkResult>>>,
    refreshes: Mutex<HashMap<String, Pending<AccountDto>>>,
    attempts: Mutex<HashMap<String, Timestamp>>,
    auto: Mutex<Option<CancellationToken>>,
    subscription: Mutex<Option<LifecycleSubscription>>,
    disposed: AtomicBool,
    next: AtomicU64,
}

#[derive(Clone)]
pub struct AccountService {
    inner: Arc<Inner>,
}

enum Stop {
    Aborted,
    Failed(AppError),
}

impl From<AppError> for Stop {
    fn from(error: AppError) -> Self {
        Self::Failed(error)
    }
}

struct Deadline<'a> {
    at: Instant,
    cancel: &'a CancellationToken,
    expired: AtomicBool,
}

impl Deadline<'_> {
    async fn guard<T>(&self, work: impl Future<Output = T>) -> Result<T, Stop> {
        tokio::select! {
            biased;
            () = self.cancel.cancelled() => Err(Stop::Aborted),
            () = sleep_until(self.at) => {
                self.expired.store(true, Ordering::SeqCst);
                self.cancel.cancel();
                Err(Stop::Aborted)
            }
            value = work => Ok(value),
        }
    }

    fn expired(&self) -> bool {
        self.expired.load(Ordering::SeqCst)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn closed() -> AppError {
    AppError::new(AppErrorCode::Conflict, SERVICE_CLOSED)
}

fn not_found() -> AppError {
    AppError::new(AppErrorCode::NotFound, ACCOUNT_NOT_FOUND)
}

fn operation_cancelled() -> AppError {
    AppError::new(AppErrorCode::Conflict, OPERATION_CANCELLED)
}

fn unchecked() -> AppError {
    AppError::new(AppErrorCode::ProviderUnreachable, VENDOR_UNCHECKED)
}

fn corrupt(id: &str) -> AppError {
    AppError::new(
        AppErrorCode::DbError,
        "Аккаунт хранится в повреждённом виде",
    )
    .with_detail("accountId", id)
}

pub fn mask_email(value: Option<&str>) -> Option<String> {
    let value = value.filter(|value| !value.is_empty())?;
    let Some(at) = value.rfind('@').filter(|at| *at >= 1) else {
        return Some("***".to_owned());
    };
    let first = value.chars().next().unwrap_or_default();
    Some(format!("{first}***{}", &value[at..]))
}

fn step(fields: impl IntoIterator<Item = (&'static str, Value)>) -> HostEventDraft {
    HostEventDraft::Step {
        step: fields
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect::<Map<String, Value>>(),
    }
}

fn host_of(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
}

impl AccountService {
    pub fn new(options: AccountServiceOptions) -> Self {
        let inner = Arc::new(Inner {
            database: options.database,
            secrets: options.secrets,
            probes: options.probes,
            sessions: options.sessions,
            browser: options.browser,
            lifecycle: options.lifecycle,
            events: options.events,
            navigator: options.navigator,
            clock: options.clock,
            random: options.random,
            logger: options.logger,
            timeout: options.timeout,
            interval: options.interval,
            runtime: options.runtime,
            links: Mutex::default(),
            refreshes: Mutex::default(),
            attempts: Mutex::default(),
            auto: Mutex::default(),
            subscription: Mutex::default(),
            disposed: AtomicBool::new(false),
            next: AtomicU64::new(0),
        });
        let weak = Arc::downgrade(&inner);
        let subscription = inner.lifecycle.subscribe(move |event| {
            if let Some(inner) = weak.upgrade() {
                Self { inner }.browser_event(event);
            }
        });
        *lock(&inner.subscription) = Some(subscription);
        Self { inner }
    }

    pub fn list(&self) -> Result<Vec<AccountDto>, AppError> {
        let rows = self.inner.database.with_connection(|connection| {
            let repositories = Repositories::new(connection);
            let mut rows = Vec::new();
            for family in AccountFamily::ALL {
                for row in repositories.accounts.list_by_adapter(*family)? {
                    let count = repositories.providers.count_by_account(&row.id)?;
                    rows.push((row, count));
                }
            }
            Ok(rows)
        })?;
        rows.iter()
            .map(|(row, count)| self.inner.dto_with(row, *count))
            .collect()
    }

    pub fn start_auto_refresh(&self) -> Result<(), AppError> {
        self.inner.require_active()?;
        let mut auto = lock(&self.inner.auto);
        if auto.is_some() {
            return Ok(());
        }
        let runtime = self.inner.runtime()?;
        let stop = CancellationToken::new();
        *auto = Some(stop.clone());
        drop(auto);
        self.auto_refresh();
        let weak = Arc::downgrade(&self.inner);
        runtime.spawn(async move {
            let mut ticker = interval_at(Instant::now() + AUTO_REFRESH_TICK, AUTO_REFRESH_TICK);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    () = stop.cancelled() => return,
                    _ = ticker.tick() => {
                        let Some(inner) = weak.upgrade() else {
                            return;
                        };
                        Self { inner }.auto_refresh();
                    }
                }
            }
        });
        Ok(())
    }

    fn auto_refresh(&self) {
        let now = self.inner.clock.now();
        let Ok(accounts) = self.list() else {
            return;
        };
        for account in accounts {
            let id = account.id.to_string();
            let lapsed = account.status == AccountStatus::NeedsRelink
                && account.detail.as_deref() == Some(SESSION_EXPIRED_DETAIL);
            if (account.status != AccountStatus::Linked && !lapsed)
                || lock(&self.inner.links).contains_key(&account.adapter)
                || lock(&self.inner.refreshes).contains_key(&id)
            {
                continue;
            }
            let attempted = lock(&self.inner.attempts).get(&id).copied().unwrap_or(0);
            let checked = if lapsed {
                attempted
            } else {
                account.last_checked_at.unwrap_or(0).max(attempted)
            };
            let due = if lapsed {
                AUTO_REFRESH_IDLE_MS
            } else if account
                .expires_at
                .is_some_and(|expires_at| expires_at <= now + EXPIRY_WINDOW_MS)
            {
                AUTO_REFRESH_SOON_MS
            } else {
                AUTO_REFRESH_IDLE_MS
            };
            if now - checked < due {
                continue;
            }
            lock(&self.inner.attempts).insert(id.clone(), now);
            let Ok(runtime) = self.inner.runtime() else {
                return;
            };
            let refreshed = self.refresh(&id);
            let logger = self.inner.logger.clone();
            let adapter = account.adapter;
            runtime.spawn(async move {
                if refreshed.await.is_err() {
                    logger.log(
                        LogLevel::Debug,
                        "accounts",
                        "Automatic session refresh deferred",
                        &[("adapter", json!(adapter))],
                    );
                }
            });
        }
    }

    pub fn link(
        &self,
        adapter: AccountFamily,
    ) -> BoxFuture<'static, Result<AccountLinkResult, AppError>> {
        if let Err(error) = self.inner.require_active() {
            return ready(Err(error)).boxed();
        }
        let ids = match self.inner.rows_of(adapter) {
            Ok(rows) => rows.into_iter().map(|row| row.id).collect::<Vec<_>>(),
            Err(error) => return ready(Err(error)).boxed(),
        };
        let runtime = match self.inner.runtime() {
            Ok(runtime) => runtime,
            Err(error) => return ready(Err(error)).boxed(),
        };
        let mut links = lock(&self.inner.links);
        if let Some(pending) = links.get(&adapter) {
            return pending.result.clone().boxed();
        }
        {
            let refreshes = lock(&self.inner.refreshes);
            for id in &ids {
                if let Some(pending) = refreshes.get(id) {
                    pending.cancel.cancel();
                }
            }
        }
        let cancel = CancellationToken::new();
        let id = self.inner.next.fetch_add(1, Ordering::Relaxed);
        let inner = Arc::clone(&self.inner);
        let token = cancel.clone();
        let task = runtime.spawn(async move {
            let result = inner.poll(adapter, &token).await;
            let mut links = lock(&inner.links);
            if links.get(&adapter).is_some_and(|pending| pending.id == id) {
                links.remove(&adapter);
            }
            result
        });
        let result = async move { task.await.unwrap_or_else(|_| Err(operation_cancelled())) }
            .boxed()
            .shared();
        links.insert(
            adapter,
            Pending {
                id,
                cancel,
                result: result.clone(),
            },
        );
        drop(links);
        result.boxed()
    }

    pub fn cancel_link(&self, adapter: AccountFamily) -> AccountsCancelLinkOutput {
        let links = lock(&self.inner.links);
        let pending = links.get(&adapter);
        if let Some(pending) = pending {
            pending.cancel.cancel();
        }
        AccountsCancelLinkOutput {
            cancelled: pending.is_some(),
        }
    }

    pub fn unlink(&self, id: &str) -> Result<AccountsUnlinkOutput, AppError> {
        let row = self.inner.require(id)?;
        if let Some(family) = row.family() {
            self.cancel_link(family);
        }
        if let Some(pending) = lock(&self.inner.refreshes).get(id) {
            pending.cancel.cancel();
        }
        lock(&self.inner.attempts).remove(id);
        let now = self.inner.clock.now();
        self.inner.database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            let removal = repositories.accounts.remove(id, now)?;
            if let Some(secret_id) = &removal.freed_secret_id {
                self.inner.secrets.remove_in(&repositories, secret_id)?;
            }
            Ok(())
        })?;
        Ok(AccountsUnlinkOutput {
            id: AccountId::parse(id).ok_or_else(|| corrupt(id))?,
            removed: true,
            browser_session_preserved: true,
        })
    }

    pub fn refresh(&self, id: &str) -> BoxFuture<'static, Result<AccountDto, AppError>> {
        if let Err(error) = self.inner.require_active() {
            return ready(Err(error)).boxed();
        }
        let row = match self.inner.require(id) {
            Ok(row) => row,
            Err(error) => return ready(Err(error)).boxed(),
        };
        if row
            .family()
            .is_some_and(|family| lock(&self.inner.links).contains_key(&family))
        {
            return ready(Err(AppError::new(
                AppErrorCode::Conflict,
                SIGN_IN_IN_PROGRESS,
            )))
            .boxed();
        }
        let runtime = match self.inner.runtime() {
            Ok(runtime) => runtime,
            Err(error) => return ready(Err(error)).boxed(),
        };
        let mut refreshes = lock(&self.inner.refreshes);
        if let Some(pending) = refreshes.get(id) {
            return pending.result.clone().boxed();
        }
        let cancel = CancellationToken::new();
        let sequence = self.inner.next.fetch_add(1, Ordering::Relaxed);
        let inner = Arc::clone(&self.inner);
        let token = cancel.clone();
        let key = id.to_owned();
        let task = runtime.spawn(async move {
            let result = inner.refresh_row(row, &token).await;
            let mut refreshes = lock(&inner.refreshes);
            if refreshes
                .get(&key)
                .is_some_and(|pending| pending.id == sequence)
            {
                refreshes.remove(&key);
            }
            result
        });
        let result = async move { task.await.unwrap_or_else(|_| Err(operation_cancelled())) }
            .boxed()
            .shared();
        refreshes.insert(
            id.to_owned(),
            Pending {
                id: sequence,
                cancel,
                result: result.clone(),
            },
        );
        drop(refreshes);
        result.boxed()
    }

    pub fn is_linking(&self, adapter: AccountFamily) -> bool {
        lock(&self.inner.links).contains_key(&adapter)
    }

    pub async fn dispose(&self) {
        self.inner.disposed.store(true, Ordering::SeqCst);
        if let Some(stop) = lock(&self.inner.auto).take() {
            stop.cancel();
        }
        lock(&self.inner.attempts).clear();
        drop(lock(&self.inner.subscription).take());
        let mut pending: Vec<BoxFuture<'static, ()>> = Vec::new();
        for link in lock(&self.inner.links).values() {
            link.cancel.cancel();
            pending.push(link.result.clone().map(|_| ()).boxed());
        }
        for refresh in lock(&self.inner.refreshes).values() {
            refresh.cancel.cancel();
            pending.push(refresh.result.clone().map(|_| ()).boxed());
        }
        join_all(pending).await;
    }

    fn browser_event(&self, event: &BrowserLifecycleEvent) {
        match event {
            BrowserLifecycleEvent::WorkspaceClosed => {
                for pending in lock(&self.inner.links).values() {
                    pending.cancel.cancel();
                }
            }
            BrowserLifecycleEvent::LinkTabClosed { url } => {
                for family in AccountFamily::ALL {
                    if self.inner.probe(*family).login_url() == url {
                        self.cancel_link(*family);
                    }
                }
            }
            BrowserLifecycleEvent::CookiesCleared { domain } => self.cookies_cleared(domain),
            BrowserLifecycleEvent::LinkFinished { .. } => {}
        }
    }

    fn cookies_cleared(&self, domain: &str) {
        let domain = domain.strip_prefix('.').unwrap_or(domain).to_lowercase();
        for family in AccountFamily::ALL {
            let Some(host) = host_of(self.inner.probe(*family).login_url()) else {
                continue;
            };
            if domain != host && !host.ends_with(&format!(".{domain}")) {
                continue;
            }
            self.cancel_link(*family);
            let Ok(rows) = self.inner.rows_of(*family) else {
                continue;
            };
            for row in rows {
                if let Some(pending) = lock(&self.inner.refreshes).get(&row.id) {
                    pending.cancel.cancel();
                }
                let _ = self.inner.set_status(
                    &row.id,
                    AccountStatus::NeedsRelink,
                    COOKIES_CLEARED_DETAIL,
                );
            }
        }
    }
}

impl Inner {
    fn require_active(&self) -> Result<(), AppError> {
        if self.disposed.load(Ordering::SeqCst) {
            Err(closed())
        } else {
            Ok(())
        }
    }

    fn runtime(&self) -> Result<Handle, AppError> {
        self.runtime
            .clone()
            .or_else(|| Handle::try_current().ok())
            .ok_or_else(closed)
    }

    fn probe(&self, family: AccountFamily) -> SharedIdentityProbe {
        (self.probes)(family)
    }

    fn rows_of(&self, family: AccountFamily) -> Result<Vec<AccountRow>, AppError> {
        self.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .accounts
                .list_by_adapter(family)?)
        })
    }

    fn require(&self, id: &str) -> Result<AccountRow, AppError> {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).accounts.find_by_id(id)?)
            })?
            .ok_or_else(not_found)
    }

    fn set_status(&self, id: &str, status: AccountStatus, detail: &str) -> Result<(), AppError> {
        let now = self.clock.now();
        self.database.transaction(|transaction| {
            Repositories::new(transaction).accounts.update_status(
                id,
                status,
                Some(detail.to_owned()),
                now,
            )?;
            Ok(())
        })
    }

    fn touch(&self, id: &str) -> Result<(), AppError> {
        let now = self.clock.now();
        self.database.transaction(|transaction| {
            Repositories::new(transaction).accounts.update(
                id,
                &AccountPatch {
                    last_checked_at: Some(Some(now)),
                    updated_at: Some(now),
                    ..AccountPatch::default()
                },
                now,
            )?;
            Ok(())
        })
    }

    fn dto(&self, row: &AccountRow) -> Result<AccountDto, AppError> {
        let count = self.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .providers
                .count_by_account(&row.id)?)
        })?;
        self.dto_with(row, count)
    }

    fn dto_with(&self, row: &AccountRow, count: u64) -> Result<AccountDto, AppError> {
        Ok(AccountDto {
            id: AccountId::parse(&row.id).ok_or_else(|| corrupt(&row.id))?,
            adapter: row.family().ok_or_else(|| corrupt(&row.id))?,
            email_masked: mask_email(row.email_masked.as_deref()),
            display_name: row.display_name.clone(),
            avatar_url: row.avatar_url.clone(),
            status: row.account_status().ok_or_else(|| corrupt(&row.id))?,
            detail: row.status_detail.clone(),
            expires_at: row
                .token_expires_at
                .map(|seconds| seconds.saturating_mul(1000)),
            last_checked_at: row.last_checked_at,
            linked_providers_count: count,
        })
    }

    async fn poll(
        &self,
        adapter: AccountFamily,
        cancel: &CancellationToken,
    ) -> Result<AccountLinkResult, AppError> {
        let probe = self.probe(adapter);
        let outcome = self.poll_with(adapter, &probe, cancel).await;
        self.lifecycle.emit(&BrowserLifecycleEvent::LinkFinished {
            url: probe.login_url().to_owned(),
        });
        outcome
    }

    async fn poll_with(
        &self,
        adapter: AccountFamily,
        probe: &SharedIdentityProbe,
        cancel: &CancellationToken,
    ) -> Result<AccountLinkResult, AppError> {
        let stream = self.events.open_stream()?;
        let deadline = Deadline {
            at: Instant::now() + self.timeout,
            cancel,
            expired: AtomicBool::new(false),
        };
        let total = u64::try_from(self.timeout.as_millis()).unwrap_or(u64::MAX);
        let start = self.clock.now();
        let run = async {
            if cancel.is_cancelled() {
                return Err(Stop::Aborted);
            }
            let session = (self.sessions)(BROWSER_PARTITION);
            deadline
                .guard(self.browser.open_tab(probe.login_url()))
                .await??;
            loop {
                if cancel.is_cancelled() {
                    return Err(Stop::Aborted);
                }
                stream.emit(step([
                    ("domain", json!("accounts")),
                    ("adapter", json!(adapter)),
                    ("phase", json!("waiting-for-sign-in")),
                ]));
                let elapsed = u64::try_from((self.clock.now() - start).max(0)).unwrap_or(0);
                stream.emit(HostEventDraft::Progress {
                    done: elapsed.min(total),
                    total,
                });
                let checked = match deadline
                    .guard(probe.probe(session.as_ref(), cancel))
                    .await?
                {
                    Ok(result) => deadline
                        .guard(confirm_renewable(probe.as_ref(), session.as_ref(), cancel))
                        .await?
                        .map(|()| result),
                    Err(error) => Err(error),
                };
                match checked {
                    Err(error) => {
                        if !is_session_expired(&error)
                            && error.code != AppErrorCode::ProviderUnreachable
                        {
                            return Err(Stop::Failed(unchecked()));
                        }
                        let jitter = 0.9 + (self.random)() * 0.2;
                        deadline.guard(sleep(self.interval.mul_f64(jitter))).await?;
                    }
                    Ok(result) => {
                        if cancel.is_cancelled() {
                            return Err(Stop::Aborted);
                        }
                        let linked = self.save(adapter, result)?;
                        stream.emit(step([
                            ("domain", json!("accounts")),
                            ("adapter", json!(adapter)),
                            ("phase", json!("done")),
                            ("result", json!(linked)),
                        ]));
                        stream.end(ok_outcome());
                        self.navigator.navigate(WorkspaceRoute::Providers);
                        return Ok(linked);
                    }
                }
            }
        };
        match run.await {
            Ok(linked) => Ok(linked),
            Err(Stop::Failed(error)) if !cancel.is_cancelled() => {
                stream.end(RunOutcomeDto {
                    status: RunOutcomeStatus::Failed,
                    code: None,
                    message: Some(LINKING_FAILED.to_owned()),
                });
                self.logger.log(
                    LogLevel::Warn,
                    "accounts",
                    LINKING_FAILED,
                    &[("adapter", json!(adapter))],
                );
                Err(error)
            }
            Err(_) => {
                let result = if deadline.expired() {
                    AccountLinkResult::Timeout {
                        detail: NO_SIGN_IN.to_owned(),
                    }
                } else {
                    AccountLinkResult::Cancelled {
                        detail: NO_SIGN_IN.to_owned(),
                    }
                };
                stream.emit(step([
                    ("domain", json!("accounts")),
                    ("adapter", json!(adapter)),
                    ("phase", json!("done")),
                    ("result", json!(result)),
                ]));
                stream.end(RunOutcomeDto {
                    status: RunOutcomeStatus::Cancelled,
                    code: None,
                    message: None,
                });
                Ok(result)
            }
        }
    }

    async fn refresh_row(
        &self,
        row: AccountRow,
        cancel: &CancellationToken,
    ) -> Result<AccountDto, AppError> {
        let deadline = Deadline {
            at: Instant::now() + self.timeout,
            cancel,
            expired: AtomicBool::new(false),
        };
        let checked = async {
            let family = row.family().ok_or_else(|| corrupt(&row.id))?;
            let session =
                account_session(&row, (self.sessions)(&row.partition), self.secrets.as_ref())?;
            if cancel.is_cancelled() {
                return Err(Stop::Aborted);
            }
            let probe = self.probe(family);
            let result = deadline
                .guard(probe_renewing(probe.as_ref(), &session, cancel))
                .await??;
            deadline
                .guard(confirm_renewable(probe.as_ref(), &session, cancel))
                .await??;
            if cancel.is_cancelled() {
                return Err(Stop::Aborted);
            }
            if result.identity.external_id == row.external_id {
                self.save(family, result)?;
            } else {
                self.set_status(&row.id, AccountStatus::NeedsRelink, DIFFERENT_ACCOUNT)?;
            }
            Ok(())
        };
        match checked.await {
            Ok(()) => {}
            Err(_) if cancel.is_cancelled() => return Err(operation_cancelled()),
            Err(Stop::Aborted) => return Err(operation_cancelled()),
            Err(Stop::Failed(error)) => {
                self.touch(&row.id)?;
                if !is_session_expired(&error) {
                    return Err(unchecked());
                }
                self.set_status(&row.id, AccountStatus::NeedsRelink, SESSION_EXPIRED_DETAIL)?;
            }
        }
        self.dto(&self.require(&row.id)?)
    }

    fn save(
        &self,
        adapter: AccountFamily,
        result: ProbeResult,
    ) -> Result<AccountLinkResult, AppError> {
        let ProbeResult {
            identity,
            credential,
        } = result;
        let (row, replaced) = self.database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            let previous = repositories.accounts.list_by_adapter(adapter)?;
            let matching = repositories
                .accounts
                .find_by_adapter_and_external_id(adapter, &identity.external_id)?;
            let replaced = previous
                .iter()
                .any(|row| row.external_id != identity.external_id);
            for row in &previous {
                if matching.as_ref().is_some_and(|kept| kept.id == row.id) {
                    continue;
                }
                let removal = repositories.accounts.remove(&row.id, self.clock.now())?;
                if let Some(secret_id) = &removal.freed_secret_id {
                    self.secrets.remove_in(&repositories, secret_id)?;
                }
            }
            let now = self.clock.now();
            let previous_secret = matching
                .as_ref()
                .and_then(|row| row.token_secret_id.clone());
            let token_secret_id = match &credential {
                None => None,
                Some(credential) => Some(match &previous_secret {
                    Some(secret_id) => {
                        self.secrets.update_in(
                            &repositories,
                            secret_id,
                            SecretUpdateInput {
                                value: Some(Plaintext::new(credential.token.clone())),
                                ..SecretUpdateInput::default()
                            },
                        )?;
                        secret_id.clone()
                    }
                    None => {
                        let mut input = SecretCreateInput::new(
                            ACCOUNT_TOKEN_SECRET_TYPE,
                            format!("{adapter} account token"),
                            SecretScope::Personal,
                        );
                        input.value = Some(Plaintext::new(credential.token.clone()));
                        self.secrets.create_in(&repositories, input)?.id
                    }
                }),
            };
            let email = mask_email(identity.email_masked.as_deref());
            let row = match &matching {
                Some(kept) => repositories
                    .accounts
                    .update(
                        &kept.id,
                        &AccountPatch {
                            external_id: Some(identity.external_id.clone()),
                            email_masked: Some(email),
                            display_name: Some(identity.display_name.clone()),
                            avatar_url: Some(identity.avatar_url.clone()),
                            token_secret_id: Some(token_secret_id.clone()),
                            token_expires_at: Some(identity.expires_at),
                            status: Some(AccountStatus::Linked),
                            status_detail: Some(None),
                            last_checked_at: Some(Some(now)),
                            updated_at: Some(now),
                            ..AccountPatch::default()
                        },
                        now,
                    )?
                    .ok_or_else(|| DataError::from(not_found()))?,
                None => repositories.accounts.create(&AccountDraft {
                    email_masked: email,
                    display_name: identity.display_name.clone(),
                    avatar_url: identity.avatar_url.clone(),
                    token_secret_id: token_secret_id.clone(),
                    token_expires_at: identity.expires_at,
                    last_checked_at: Some(now),
                    ..AccountDraft::new(
                        adapter,
                        BROWSER_PARTITION,
                        identity.external_id.clone(),
                        now,
                    )
                })?,
            };
            if let Some(secret_id) = &previous_secret
                && token_secret_id.is_none()
            {
                self.secrets.remove_in(&repositories, secret_id)?;
            }
            // Providers left without an account (by a replaced identity or earlier) follow it.
            let attached = repositories
                .accounts
                .attach_orphans(adapter, &row.id, now)?;
            if !attached.is_empty() {
                self.logger.log(
                    LogLevel::Info,
                    "accounts",
                    "Attached providers to the linked account",
                    &[("accountId", json!(row.id)), ("providers", json!(attached))],
                );
            }
            Ok((row, replaced))
        })?;
        Ok(AccountLinkResult::Linked {
            account: self.dto(&row)?,
            replaced_previous_identity: replaced,
            detail: replaced.then(|| IDENTITY_REPLACED.to_owned()),
        })
    }
}

#[cfg(test)]
mod tests;
