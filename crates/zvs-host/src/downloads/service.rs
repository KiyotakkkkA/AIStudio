use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock, TryLockError};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::validate::js_slice;
use zvs_contract::{
    AppError, AppErrorCode, CatalogueFilter, CatalogueItemDto, CatalogueItemState, ChecksumDto,
    DiskUsageDto, DownloadDto, DownloadId, DownloadItemKind, DownloadListFilter, DownloadReportDto,
    DownloadStatus, GraphNode, HostEvent, HostEventDraft, RetryPolicy, RunGraph, RunHandleDto,
    RunId, RunKind, RunStatus, StartDownloadInput, StartRunInput, Timestamp, Validate,
};

use super::catalogue::{CatalogueItem, CatalogueService, installed_key};
use super::disk::DiskService;
use crate::data::repositories::DownloadPatch;
use crate::data::schema::DownloadRow;
use crate::data::{Database, Repositories};
use crate::drivers::sidecar::{JobProgress, JobRunOptions, SharedSidecarJobs};
use crate::kernel::RunObserver;
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger};
use crate::platform::paths::download_target_path;
use crate::services::{RunService, SettingService};

pub const DOWNLOAD_JOB: &str = "job.download";
pub const PART_SUFFIX: &str = ".part";
pub const CONCURRENCY_KEY: &str = "downloads.concurrency";
pub const DEFAULT_CONCURRENCY: usize = 2;
pub const FREE_SPACE_MARGIN_BYTES: u64 = 1024 * 1024 * 1024;
pub const DEFAULT_PERSIST_INTERVAL: Duration = Duration::from_secs(1);
pub const SERVICE_STOPPED: &str = "Сервис загрузок остановлен";
pub const ALREADY_QUEUED: &str = "Этот элемент уже в очереди загрузки";
pub const NOT_RUNNING: &str = "Загрузка не выполняется";
pub const RESUME_REFUSED: &str = "Возобновить можно только приостановленную загрузку";
pub const PRIORITY_REFUSED: &str = "Приоритет меняется только у ожидающих загрузок";
pub const CANCEL_FIRST: &str = "Сначала отмените загрузку";
pub const DOWNLOAD_NOT_FOUND: &str = "Загрузка не найдена";
pub const RUN_ENDED_EMPTY: &str = "Запуск завершился без результата";
pub const UNEXPECTED_RESULT: &str = "Загрузка вернула неожиданный результат";
pub const NO_PATH: &str = "Загрузка не сообщила путь";

pub const fn priority_by_kind(kind: DownloadItemKind) -> u64 {
    match kind {
        DownloadItemKind::Mcp | DownloadItemKind::Skill => 7,
        DownloadItemKind::Runtime => 8,
        DownloadItemKind::Embedding => 6,
        DownloadItemKind::Model => 5,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DownloadProgress {
    pub done: u64,
    pub total: u64,
    pub rate: Option<f64>,
}

pub type RefMatcher = dyn for<'a> Fn(&'a str) -> bool + Send + Sync;

pub type DownloadProgressSink = Arc<dyn Fn(DownloadProgress) + Send + Sync>;

#[async_trait]
pub trait DownloadRunner: Send + Sync {
    fn start(&self, input: StartRunInput) -> Result<RunHandleDto, AppError>;

    async fn cancel(&self, id: &RunId) -> Result<(), AppError>;

    fn subscribe(&self, id: &str, observer: RunObserver) -> Result<(), AppError>;

    fn finished(&self, id: &RunId) -> bool;
}

#[async_trait]
impl DownloadRunner for RunService {
    fn start(&self, input: StartRunInput) -> Result<RunHandleDto, AppError> {
        Self::start(self, input)
    }

    async fn cancel(&self, id: &RunId) -> Result<(), AppError> {
        Self::cancel(self, id).await
    }

    fn subscribe(&self, id: &str, observer: RunObserver) -> Result<(), AppError> {
        Self::subscribe(self, id, observer).map(drop)
    }

    fn finished(&self, id: &RunId) -> bool {
        self.get(id)
            .is_ok_and(|run| RunStatus::FINISHED.contains(&run.status))
    }
}

#[async_trait]
pub trait DownloadInstaller: Send + Sync {
    async fn install(&self, row: &DownloadRow) -> Result<(), AppError>;
}

pub type SharedDownloadInstaller = Arc<dyn DownloadInstaller>;

#[async_trait]
pub trait DownloadExecutor: Send + Sync {
    async fn execute(
        &self,
        download_id: &str,
        cancel: Option<CancellationToken>,
        on_progress: Option<DownloadProgressSink>,
    ) -> Result<DownloadReportDto, AppError>;
}

pub type SharedDownloadExecutor = Arc<dyn DownloadExecutor>;

#[derive(Clone)]
pub struct DownloadServiceOptions {
    pub database: Arc<Database>,
    pub jobs: SharedSidecarJobs,
    pub disk: DiskService,
    pub catalogue: CatalogueService,
    pub settings: Option<SettingService>,
    pub downloads_dir: PathBuf,
    pub logger: SharedLogger,
    pub clock: SharedClock,
    pub concurrency: Option<usize>,
    pub persist_interval: Duration,
}

impl DownloadServiceOptions {
    pub fn new(
        database: Arc<Database>,
        jobs: SharedSidecarJobs,
        disk: DiskService,
        catalogue: CatalogueService,
        downloads_dir: PathBuf,
        logger: SharedLogger,
        clock: SharedClock,
    ) -> Self {
        Self {
            database,
            jobs,
            disk,
            catalogue,
            settings: None,
            downloads_dir,
            logger,
            clock,
            concurrency: None,
            persist_interval: DEFAULT_PERSIST_INTERVAL,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Live {
    rate: f64,
    bytes_done: u64,
    persisted_at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Intent {
    Pause,
    Cancel,
}

struct Inner {
    options: DownloadServiceOptions,
    live: Mutex<HashMap<String, Live>>,
    intents: Mutex<HashMap<String, Intent>>,
    runner: RwLock<Option<Arc<dyn DownloadRunner>>>,
    installer: RwLock<Option<SharedDownloadInstaller>>,
    closed: AtomicBool,
    pumping: Mutex<()>,
    pump_requested: AtomicBool,
}

#[derive(Clone)]
pub struct DownloadService {
    inner: Arc<Inner>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn conflict(message: &str) -> AppError {
    AppError::new(AppErrorCode::Conflict, message)
}

fn part_path(target: &str) -> PathBuf {
    PathBuf::from(format!("{target}{PART_SUFFIX}"))
}

fn partial(target: &str) -> u64 {
    std::fs::metadata(part_path(target)).map_or(0, |metadata| metadata.len())
}

fn discard(target: &str) {
    let _ = std::fs::remove_file(part_path(target));
}

fn remove_file(target: &str) {
    let _ = std::fs::remove_file(target);
}

fn to_fixed_1(value: f64) -> String {
    let tenths = (value * 10.0).round() as u64;
    format!("{}.{}", tenths / 10, tenths % 10)
}

pub fn gigabytes(bytes: u64) -> String {
    if bytes == u64::MAX {
        return "∞".to_owned();
    }
    format!("{} ГБ", to_fixed_1(bytes as f64 / 1024_f64.powi(3)))
}

fn checksum_of(row: &DownloadRow) -> Option<ChecksumDto> {
    match (row.checksum_algorithm, &row.checksum_value) {
        (Some(algorithm), Some(value)) if !value.is_empty() => Some(ChecksumDto {
            algorithm,
            value: value.clone(),
        }),
        _ => None,
    }
}

fn number(record: &serde_json::Map<String, Value>, key: &str) -> u64 {
    record
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or(0, |value| value as u64)
}

fn read_report(result: &Value) -> Result<(u64, u64, String, Option<String>), AppError> {
    let Some(record) = result.as_object() else {
        return Err(AppError::new(AppErrorCode::NativeError, UNEXPECTED_RESULT));
    };
    let path = record
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if path.is_empty() {
        return Err(AppError::new(AppErrorCode::NativeError, NO_PATH));
    }
    Ok((
        number(record, "bytes"),
        number(record, "resumedFrom"),
        path,
        record
            .get("checksum")
            .and_then(Value::as_str)
            .map(str::to_owned),
    ))
}

fn resolve_state(
    item: &CatalogueItem,
    active: Option<DownloadStatus>,
    installed: bool,
    installed_version: Option<&str>,
    digest: Option<&str>,
) -> CatalogueItemState {
    match active {
        Some(DownloadStatus::Running | DownloadStatus::Paused) => {
            return CatalogueItemState::Downloading;
        }
        Some(DownloadStatus::Queued) => return CatalogueItemState::Queued,
        _ => {}
    }
    if !installed {
        return CatalogueItemState::Available;
    }
    if let (Some(wanted), Some(present)) = (&item.digest, digest) {
        return if wanted.to_lowercase() == present.to_lowercase() {
            CatalogueItemState::Installed
        } else {
            CatalogueItemState::Update
        };
    }
    if let (Some(wanted), Some(present)) = (&item.version, installed_version) {
        return if wanted == present {
            CatalogueItemState::Installed
        } else {
            CatalogueItemState::Update
        };
    }
    CatalogueItemState::Installed
}

impl DownloadService {
    pub fn new(options: DownloadServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                live: Mutex::new(HashMap::new()),
                intents: Mutex::new(HashMap::new()),
                runner: RwLock::new(None),
                installer: RwLock::new(None),
                closed: AtomicBool::new(false),
                pumping: Mutex::new(()),
                pump_requested: AtomicBool::new(false),
            }),
        }
    }

    pub fn catalogue_source(&self) -> CatalogueService {
        self.inner.options.catalogue.clone()
    }

    pub fn downloads_dir(&self) -> &Path {
        &self.inner.options.downloads_dir
    }

    pub fn attach_installer(&self, installer: SharedDownloadInstaller) {
        *self
            .inner
            .installer
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(installer);
    }

    pub fn attach(&self, runner: Arc<dyn DownloadRunner>) {
        *self
            .inner
            .runner
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(runner);
        self.pump();
    }

    fn runner(&self) -> Option<Arc<dyn DownloadRunner>> {
        self.inner
            .runner
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn installer(&self) -> Option<SharedDownloadInstaller> {
        self.inner
            .installer
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn now(&self) -> Timestamp {
        self.inner.options.clock.now()
    }

    fn log(&self, level: LogLevel, message: &str, fields: &[(&str, Value)]) {
        self.inner
            .options
            .logger
            .log(level, "downloads", message, fields);
    }

    fn repository<T>(
        &self,
        work: impl FnOnce(&crate::data::repositories::DownloadRepository<'_>) -> rusqlite::Result<T>,
    ) -> Result<T, AppError> {
        self.inner
            .options
            .database
            .with_connection(|connection| Ok(work(&Repositories::new(connection).downloads)?))
    }

    fn update(&self, id: &str, patch: &DownloadPatch) -> Result<Option<DownloadRow>, AppError> {
        self.repository(|downloads| downloads.update(id, patch))
    }

    fn update_existing(&self, id: &str, patch: &DownloadPatch) -> Result<DownloadRow, AppError> {
        self.update(id, patch)?
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, DOWNLOAD_NOT_FOUND))
    }

    fn require(&self, id: &str) -> Result<DownloadRow, AppError> {
        self.repository(|downloads| downloads.find_by_id(id))?
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, DOWNLOAD_NOT_FOUND))
    }

    pub fn concurrency(&self) -> usize {
        let stored = self
            .inner
            .options
            .settings
            .as_ref()
            .and_then(|settings| settings.get(CONCURRENCY_KEY).ok().flatten())
            .and_then(|setting| setting.value.as_f64())
            .filter(|value| value.fract() == 0.0 && (1.0..=8.0).contains(value));
        stored.map_or_else(
            || {
                self.inner
                    .options
                    .concurrency
                    .unwrap_or(DEFAULT_CONCURRENCY)
            },
            |value| value as usize,
        )
    }

    pub fn list(&self, filter: &DownloadListFilter) -> Result<Vec<DownloadDto>, AppError> {
        self.repository(|downloads| downloads.list())?
            .into_iter()
            .filter(|row| {
                filter
                    .statuses
                    .as_ref()
                    .is_none_or(|statuses| statuses.contains(&row.status))
                    && filter
                        .kinds
                        .as_ref()
                        .is_none_or(|kinds| kinds.contains(&row.item_kind))
            })
            .map(|row| self.to_dto(&row))
            .collect()
    }

    pub fn get(&self, id: &str) -> Result<DownloadDto, AppError> {
        self.to_dto(&self.require(id)?)
    }

    pub async fn disk(&self, refresh: bool) -> DiskUsageDto {
        self.inner.options.disk.usage(refresh).await
    }

    async fn free_bytes(&self) -> u64 {
        self.inner
            .options
            .disk
            .free_bytes()
            .await
            .unwrap_or(u64::MAX)
    }

    fn precondition(
        item: &CatalogueItem,
        free_bytes: u64,
        active: bool,
        already_on_disk: u64,
    ) -> Option<String> {
        if active {
            return None;
        }
        let needed = item
            .size_bytes
            .saturating_sub(already_on_disk)
            .saturating_add(FREE_SPACE_MARGIN_BYTES);
        (free_bytes < needed).then(|| {
            format!(
                "Недостаточно места на диске: нужно {}, свободно {}",
                gigabytes(needed),
                gigabytes(free_bytes)
            )
        })
    }

    pub async fn catalogue(
        &self,
        filter: &CatalogueFilter,
    ) -> Result<Vec<CatalogueItemDto>, AppError> {
        filter.validate()?;
        let refresh = filter.refresh();
        let catalogue = &self.inner.options.catalogue;
        let (items, installed, free_bytes) = tokio::join!(
            catalogue.list(refresh),
            catalogue.installed(refresh),
            self.free_bytes()
        );
        let mut active: HashMap<String, DownloadRow> = HashMap::new();
        let mut succeeded: HashMap<String, DownloadRow> = HashMap::new();
        for row in self.repository(|downloads| downloads.list())? {
            if row.status == DownloadStatus::Succeeded {
                succeeded.entry(row.item_ref.clone()).or_insert(row);
            } else if !row.status.is_terminal() {
                active.entry(row.item_ref.clone()).or_insert(row);
            }
        }
        let query = filter.query().map(str::to_lowercase);
        let mut rows = Vec::new();
        for item in items {
            let running = active.get(&item.item_ref);
            let present = succeeded.get(&item.item_ref);
            let reported = installed.get(&installed_key(item.kind, &item.name));
            let installed_version = reported
                .and_then(|reported| reported.version.clone())
                .or_else(|| present.and_then(|present| present.version.clone()));
            let state = item.state.unwrap_or_else(|| {
                resolve_state(
                    &item,
                    running.map(|row| row.status),
                    reported.is_some() || present.is_some(),
                    installed_version.as_deref(),
                    reported.and_then(|reported| reported.digest.as_deref()),
                )
            });
            let blocked = item
                .blocked_reason
                .clone()
                .or_else(|| Self::precondition(&item, free_bytes, running.is_some(), 0));
            let dto = CatalogueItemDto {
                item_ref: item.item_ref.clone(),
                kind: item.kind,
                source: item.source,
                name: item.name.clone(),
                display_name: item.display_name.clone(),
                description: item.description.clone(),
                version: item.version.clone(),
                installed_version,
                size_bytes: item.size_bytes,
                url: item.url.clone(),
                checksum: item.checksum.clone(),
                tags: item.tags.clone(),
                dimension: item.dimension,
                query_prefix: item.query_prefix.clone(),
                document_prefix: item.document_prefix.clone(),
                state,
                downloadable: state != CatalogueItemState::Installed && blocked.is_none(),
                blocked_reason: blocked,
                download_id: running.and_then(|row| DownloadId::parse(&row.id)),
            };
            dto.validate()?;
            let wanted = filter
                .kinds
                .as_ref()
                .is_none_or(|kinds| kinds.contains(&dto.kind))
                && filter
                    .states
                    .as_ref()
                    .is_none_or(|states| states.contains(&dto.state))
                && query.as_ref().is_none_or(|query| {
                    dto.display_name.to_lowercase().contains(query)
                        || dto.name.to_lowercase().contains(query)
                });
            if wanted {
                rows.push(dto);
            }
        }
        Ok(rows)
    }

    fn require_writable(directory: &Path) -> Result<(), AppError> {
        let writable = std::fs::create_dir_all(directory)
            .and_then(|()| std::fs::metadata(directory))
            .is_ok_and(|metadata| !metadata.permissions().readonly());
        if writable {
            Ok(())
        } else {
            Err(AppError::new(
                AppErrorCode::PermissionDenied,
                format!(
                    "Каталог загрузок недоступен для записи: {}",
                    directory.display()
                ),
            ))
        }
    }

    pub async fn start(&self, input: &StartDownloadInput) -> Result<DownloadDto, AppError> {
        input.validate()?;
        if self.inner.closed.load(Ordering::SeqCst) {
            return Err(conflict(SERVICE_STOPPED));
        }
        let item = self.inner.options.catalogue.find(input.item_ref()).await?;
        if self
            .repository(|downloads| downloads.find_active_by_ref(&item.item_ref))?
            .is_some()
        {
            return Err(conflict(ALREADY_QUEUED));
        }
        let target_path = download_target_path(
            &self.inner.options.downloads_dir,
            item.kind.as_str(),
            &item.file_name,
        )
        .map_err(|error| AppError::new(AppErrorCode::Unknown, error.to_string()))?;
        if let Some(parent) = target_path.parent() {
            Self::require_writable(parent)?;
        }
        let target = target_path.to_string_lossy().into_owned();
        let on_disk = partial(&target);
        if let Some(blocked) = Self::precondition(&item, self.free_bytes().await, false, on_disk) {
            return Err(conflict(&blocked));
        }
        let at = self.now();
        let draft = DownloadRow {
            id: new_id().hyphenated().to_string(),
            item_kind: item.kind,
            item_ref: item.item_ref.clone(),
            display_name: item.display_name.clone(),
            version: item.version.clone(),
            size_bytes: item.size_bytes,
            bytes_done: partial(&target),
            status: DownloadStatus::Queued,
            priority: input
                .priority
                .unwrap_or_else(|| priority_by_kind(item.kind)),
            target_path: target,
            url: item.url.clone(),
            checksum_algorithm: item.checksum.as_ref().map(|checksum| checksum.algorithm),
            checksum_value: item
                .checksum
                .as_ref()
                .map(|checksum| checksum.value().to_owned()),
            error: None,
            run_id: None,
            created_at: at,
            updated_at: at,
            started_at: None,
            finished_at: None,
        };
        let row = self.repository(|downloads| downloads.create(&draft))?;
        self.log(
            LogLevel::Info,
            "Queued a download",
            &[
                ("id", json!(row.id)),
                ("ref", json!(item.item_ref)),
                ("sizeBytes", json!(item.size_bytes)),
                ("priority", json!(row.priority)),
            ],
        );
        self.pump();
        let current = self
            .repository(|downloads| downloads.find_by_id(&row.id))?
            .unwrap_or(row);
        self.to_dto(&current)
    }

    fn set_intent(&self, id: &str, intent: Intent) {
        lock(&self.inner.intents).insert(id.to_owned(), intent);
    }

    fn intent(&self, id: &str) -> Option<Intent> {
        lock(&self.inner.intents).get(id).copied()
    }

    fn forget(&self, id: &str) {
        lock(&self.inner.live).remove(id);
        lock(&self.inner.intents).remove(id);
    }

    pub async fn pause(&self, id: &str) -> Result<DownloadDto, AppError> {
        let row = self.require(id)?;
        if row.status == DownloadStatus::Queued {
            return self.settle(id, DownloadStatus::Paused);
        }
        if row.status != DownloadStatus::Running {
            return Err(conflict(NOT_RUNNING));
        }
        self.set_intent(id, Intent::Pause);
        self.abort(&row).await?;
        self.get(id)
    }

    pub fn resume(&self, id: &str) -> Result<DownloadDto, AppError> {
        let row = self.require(id)?;
        if !matches!(row.status, DownloadStatus::Paused | DownloadStatus::Failed) {
            return Err(conflict(RESUME_REFUSED));
        }
        let updated = self.update_existing(
            id,
            &DownloadPatch {
                status: Some(DownloadStatus::Queued),
                error: Some(None),
                updated_at: Some(self.now()),
                ..DownloadPatch::default()
            },
        )?;
        self.pump();
        self.to_dto(&updated)
    }

    pub fn prioritise(&self, id: &str, priority: u64) -> Result<DownloadDto, AppError> {
        let row = self.require(id)?;
        if row.status != DownloadStatus::Queued {
            return Err(conflict(PRIORITY_REFUSED));
        }
        let updated = self.update_existing(
            id,
            &DownloadPatch {
                priority: Some(priority),
                updated_at: Some(self.now()),
                ..DownloadPatch::default()
            },
        )?;
        self.pump();
        self.to_dto(&updated)
    }

    pub async fn cancel(&self, id: &str) -> Result<DownloadDto, AppError> {
        let row = self.require(id)?;
        if row.status.is_terminal() {
            return self.to_dto(&row);
        }
        if row.status == DownloadStatus::Running {
            self.set_intent(id, Intent::Cancel);
            self.abort(&row).await?;
            return self.get(id);
        }
        discard(&row.target_path);
        let cancelled = self.settle(id, DownloadStatus::Cancelled)?;
        self.pump();
        Ok(cancelled)
    }

    pub async fn remove(&self, id: &str) -> Result<(), AppError> {
        let row = self.require(id)?;
        if !row.status.is_terminal() && row.status != DownloadStatus::Paused {
            return Err(conflict(CANCEL_FIRST));
        }
        discard(&row.target_path);
        if row.status == DownloadStatus::Succeeded {
            remove_file(&row.target_path);
        }
        self.repository(|downloads| downloads.remove(id))?;
        self.forget(id);
        self.pump();
        Ok(())
    }

    pub async fn forget_by_ref(&self, matches: &RefMatcher) -> Result<usize, AppError> {
        let rows: Vec<DownloadRow> = self
            .repository(|downloads| downloads.list())?
            .into_iter()
            .filter(|row| matches(&row.item_ref))
            .collect();
        for row in &rows {
            if !row.status.is_terminal() && row.status != DownloadStatus::Paused {
                self.cancel(&row.id).await?;
            }
            discard(&row.target_path);
            remove_file(&row.target_path);
            self.repository(|downloads| downloads.remove(&row.id))?;
            self.forget(&row.id);
        }
        self.inner.options.catalogue.invalidate();
        self.pump();
        Ok(rows.len())
    }

    pub async fn execute(
        &self,
        download_id: &str,
        cancel: Option<CancellationToken>,
        on_progress: Option<DownloadProgressSink>,
    ) -> Result<DownloadReportDto, AppError> {
        let row = self.require(download_id)?;
        lock(&self.inner.live).insert(
            download_id.to_owned(),
            Live {
                rate: 0.0,
                bytes_done: row.bytes_done,
                persisted_at: 0,
            },
        );
        let outcome = self.transfer(&row, cancel, on_progress).await;
        if let Err(error) = &outcome {
            self.fail(&row, error);
        }
        self.forget(download_id);
        self.inner.options.catalogue.invalidate();
        self.pump();
        outcome
    }

    async fn transfer(
        &self,
        row: &DownloadRow,
        cancel: Option<CancellationToken>,
        on_progress: Option<DownloadProgressSink>,
    ) -> Result<DownloadReportDto, AppError> {
        let checksum = checksum_of(row);
        let mut params = json!({
            "url": row.url,
            "targetPath": row.target_path,
            "sizeBytes": if row.size_bytes > 0 { json!(row.size_bytes) } else { Value::Null },
        });
        if let Some(checksum) = &checksum {
            params["checksum"] = json!({
                "algorithm": checksum.algorithm.as_str(),
                "value": checksum.value,
            });
        }
        let service = self.clone();
        let id = row.id.clone();
        let result = self
            .inner
            .options
            .jobs
            .run(
                DOWNLOAD_JOB,
                params,
                JobRunOptions {
                    cancel,
                    on_progress: Some(Arc::new(move |progress: JobProgress| {
                        service.track(&id, progress.done, progress.rate.unwrap_or(0.0));
                        if let Some(sink) = &on_progress {
                            sink(DownloadProgress {
                                done: progress.done,
                                total: progress.total,
                                rate: progress.rate,
                            });
                        }
                    })),
                },
            )
            .await?;
        let (bytes, resumed_from, path, reported_checksum) = read_report(&result)?;
        if let Some(installer) = self.installer() {
            installer.install(row).await?;
        }
        let at = self.now();
        self.update(
            &row.id,
            &DownloadPatch {
                status: Some(DownloadStatus::Succeeded),
                bytes_done: Some(bytes),
                size_bytes: Some(if bytes > 0 { bytes } else { row.size_bytes }),
                error: Some(None),
                finished_at: Some(Some(at)),
                updated_at: Some(at),
                ..DownloadPatch::default()
            },
        )?;
        self.log(
            LogLevel::Info,
            "A download landed",
            &[
                ("id", json!(row.id)),
                ("ref", json!(row.item_ref)),
                ("bytes", json!(bytes)),
                ("resumedFrom", json!(resumed_from)),
                ("verified", json!(checksum.is_some())),
            ],
        );
        let report = DownloadReportDto {
            id: DownloadId::parse(&row.id)
                .ok_or_else(|| AppError::new(AppErrorCode::Unknown, DOWNLOAD_NOT_FOUND))?,
            bytes,
            resumed_from,
            path,
            checksum: reported_checksum,
        };
        report.validate()?;
        Ok(report)
    }

    pub fn recover(&self) -> Result<usize, AppError> {
        let at = self.now();
        let interrupted = self.repository(|downloads| downloads.running())?;
        for row in &interrupted {
            self.update(
                &row.id,
                &DownloadPatch {
                    status: Some(DownloadStatus::Queued),
                    run_id: Some(None),
                    updated_at: Some(at),
                    ..DownloadPatch::default()
                },
            )?;
        }
        if !interrupted.is_empty() {
            self.log(
                LogLevel::Info,
                "Requeued downloads after a restart",
                &[("count", json!(interrupted.len()))],
            );
        }
        self.pump();
        Ok(interrupted.len())
    }

    pub fn dispose(&self) {
        self.inner.closed.store(true, Ordering::SeqCst);
        lock(&self.inner.live).clear();
        lock(&self.inner.intents).clear();
    }

    pub fn pump(&self) {
        loop {
            if self.inner.closed.load(Ordering::SeqCst) || self.runner().is_none() {
                return;
            }
            self.inner.pump_requested.store(true, Ordering::SeqCst);
            {
                let _pumping = match self.inner.pumping.try_lock() {
                    Ok(guard) => guard,
                    Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                    Err(TryLockError::WouldBlock) => return,
                };
                while self.inner.pump_requested.swap(false, Ordering::SeqCst) {
                    if let Err(error) = self.pump_once() {
                        self.log(
                            LogLevel::Error,
                            "Could not fill the download queue",
                            &[("error", json!(error.message))],
                        );
                    }
                }
            }
            if !self.inner.pump_requested.load(Ordering::SeqCst) {
                return;
            }
        }
    }

    fn pump_once(&self) -> Result<(), AppError> {
        if self.inner.closed.load(Ordering::SeqCst) {
            return Ok(());
        }
        let Some(runner) = self.runner() else {
            return Ok(());
        };
        let running = self.repository(|downloads| downloads.running())?.len();
        let free = self.concurrency().saturating_sub(running);
        if free == 0 {
            return Ok(());
        }
        for row in self.repository(|downloads| downloads.next_queued(free))? {
            self.launch(runner.as_ref(), &row)?;
        }
        Ok(())
    }

    fn launch(&self, runner: &dyn DownloadRunner, row: &DownloadRow) -> Result<(), AppError> {
        let at = self.now();
        self.update(
            &row.id,
            &DownloadPatch {
                status: Some(DownloadStatus::Running),
                started_at: Some(at),
                updated_at: Some(at),
                error: Some(None),
                ..DownloadPatch::default()
            },
        )?;
        let mut node = GraphNode::new("fetch", "download.fetch");
        node.dependencies = Some(Vec::new());
        node.bindings = Some(std::collections::BTreeMap::new());
        node.input = Some(json!({ "downloadId": row.id }));
        node.retry = Some(RetryPolicy::default());
        let mut input = StartRunInput::new(
            RunKind::Download,
            RunGraph {
                nodes: vec![node],
                permission_scopes: None,
                allowed_tools: None,
            },
        );
        input.subject_id = Some(row.id.clone());
        input.title = Some(format!("Загрузка «{}»", row.display_name));
        input.input = Some(Value::Null);
        input.concurrency = Some(1);
        let handle = match runner.start(input) {
            Ok(handle) => handle,
            Err(error) => {
                let at = self.now();
                self.update(
                    &row.id,
                    &DownloadPatch {
                        status: Some(DownloadStatus::Failed),
                        error: Some(Some(error.message)),
                        finished_at: Some(Some(at)),
                        updated_at: Some(at),
                        ..DownloadPatch::default()
                    },
                )?;
                return Ok(());
            }
        };
        self.update(
            &row.id,
            &DownloadPatch {
                run_id: Some(Some(handle.id.to_string())),
                updated_at: Some(self.now()),
                ..DownloadPatch::default()
            },
        )?;
        let service = self.clone();
        let id = row.id.clone();
        let observer: RunObserver = Arc::new(move |event: &HostEvent| {
            if let HostEventDraft::End { outcome } = &event.body {
                service.run_ended(&id, outcome.message.clone());
            }
        });
        runner.subscribe(&handle.id.to_string(), observer)?;
        if runner.finished(&handle.id) {
            self.mark_ended(&row.id, None)?;
        }
        Ok(())
    }

    fn run_ended(&self, id: &str, message: Option<String>) {
        match self.mark_ended(id, message) {
            Ok(true) => self.pump(),
            Ok(false) => {}
            Err(error) => self.log(
                LogLevel::Error,
                "Could not settle a download after its run ended",
                &[("id", json!(id)), ("error", json!(error.message))],
            ),
        }
    }

    fn mark_ended(&self, id: &str, message: Option<String>) -> Result<bool, AppError> {
        let current = self.repository(|downloads| downloads.find_by_id(id))?;
        if current.is_none_or(|row| row.status != DownloadStatus::Running) {
            return Ok(false);
        }
        let at = self.now();
        self.update(
            id,
            &DownloadPatch {
                status: Some(DownloadStatus::Failed),
                error: Some(Some(message.unwrap_or_else(|| RUN_ENDED_EMPTY.to_owned()))),
                finished_at: Some(Some(at)),
                updated_at: Some(at),
                ..DownloadPatch::default()
            },
        )?;
        Ok(true)
    }

    async fn abort(&self, row: &DownloadRow) -> Result<(), AppError> {
        let run_id = row.run_id.as_deref().and_then(RunId::parse);
        match (run_id, self.runner()) {
            (Some(run_id), Some(runner)) => runner.cancel(&run_id).await,
            _ => {
                let status = if self.intent(&row.id) == Some(Intent::Cancel) {
                    DownloadStatus::Cancelled
                } else {
                    DownloadStatus::Paused
                };
                self.settle(&row.id, status).map(drop)
            }
        }
    }

    fn fail(&self, row: &DownloadRow, error: &AppError) {
        let at = self.now();
        let written = if error.code == AppErrorCode::RunCancelled {
            let intent = self.intent(&row.id);
            if intent == Some(Intent::Cancel) {
                discard(&row.target_path);
            }
            let cancelled = intent == Some(Intent::Cancel);
            self.update(
                &row.id,
                &DownloadPatch {
                    status: Some(if cancelled {
                        DownloadStatus::Cancelled
                    } else {
                        DownloadStatus::Paused
                    }),
                    bytes_done: Some(partial(&row.target_path)),
                    finished_at: Some(cancelled.then_some(at)),
                    updated_at: Some(at),
                    ..DownloadPatch::default()
                },
            )
        } else {
            let written = self.update(
                &row.id,
                &DownloadPatch {
                    status: Some(DownloadStatus::Failed),
                    error: Some(Some(js_slice(&error.message, 2000).to_owned())),
                    bytes_done: Some(partial(&row.target_path)),
                    finished_at: Some(Some(at)),
                    updated_at: Some(at),
                    ..DownloadPatch::default()
                },
            );
            self.log(
                LogLevel::Warn,
                "A download failed",
                &[
                    ("id", json!(row.id)),
                    ("ref", json!(row.item_ref)),
                    ("error", json!(error.message)),
                ],
            );
            written
        };
        if let Err(failure) = written {
            self.log(
                LogLevel::Error,
                "Could not record a download outcome",
                &[("id", json!(row.id)), ("error", json!(failure.message))],
            );
        }
    }

    fn track(&self, id: &str, done: u64, rate: f64) {
        let at = self.now();
        let interval =
            i64::try_from(self.inner.options.persist_interval.as_millis()).unwrap_or(i64::MAX);
        let persist = {
            let mut live = lock(&self.inner.live);
            let entry = live.entry(id.to_owned()).or_insert(Live {
                rate: 0.0,
                bytes_done: done,
                persisted_at: 0,
            });
            entry.rate = rate;
            entry.bytes_done = done;
            let due = at - entry.persisted_at >= interval;
            if due {
                entry.persisted_at = at;
            }
            due
        };
        if persist {
            let _ = self.update(
                id,
                &DownloadPatch {
                    bytes_done: Some(done),
                    updated_at: Some(at),
                    ..DownloadPatch::default()
                },
            );
        }
    }

    fn settle(&self, id: &str, status: DownloadStatus) -> Result<DownloadDto, AppError> {
        let at = self.now();
        let row = self.update_existing(
            id,
            &DownloadPatch {
                status: Some(status),
                updated_at: Some(at),
                finished_at: status.is_terminal().then_some(Some(at)),
                ..DownloadPatch::default()
            },
        )?;
        self.to_dto(&row)
    }

    fn to_dto(&self, row: &DownloadRow) -> Result<DownloadDto, AppError> {
        let live = lock(&self.inner.live).get(&row.id).copied();
        let bytes_done = live.map_or(row.bytes_done, |live| live.bytes_done);
        let rate = if row.status == DownloadStatus::Running {
            live.map_or(0.0, |live| live.rate)
        } else {
            0.0
        };
        let remaining = row.size_bytes.saturating_sub(bytes_done);
        let malformed = || AppError::new(AppErrorCode::Unknown, "Запись загрузки повреждена");
        let dto = DownloadDto {
            id: DownloadId::parse(&row.id).ok_or_else(malformed)?,
            item_kind: row.item_kind,
            item_ref: row.item_ref.clone(),
            display_name: row.display_name.clone(),
            size_bytes: row.size_bytes,
            bytes_done,
            status: row.status,
            priority: row.priority,
            target_path: row.target_path.clone(),
            url: row.url.clone(),
            checksum: checksum_of(row),
            version: row.version.clone(),
            error: row.error.clone(),
            run_id: row.run_id.as_deref().and_then(RunId::parse),
            rate_bytes_per_second: rate,
            eta_ms: (rate > 0.0 && remaining > 0)
                .then(|| (remaining as f64 / rate * 1000.0).round() as u64),
            created_at: row.created_at,
            updated_at: row.updated_at,
            started_at: row.started_at,
            finished_at: row.finished_at,
        };
        dto.validate()?;
        Ok(dto)
    }
}

#[async_trait]
impl DownloadExecutor for DownloadService {
    async fn execute(
        &self,
        download_id: &str,
        cancel: Option<CancellationToken>,
        on_progress: Option<DownloadProgressSink>,
    ) -> Result<DownloadReportDto, AppError> {
        Self::execute(self, download_id, cancel, on_progress).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_written_like_to_fixed_one() {
        assert_eq!(gigabytes(u64::MAX), "∞");
        assert_eq!(gigabytes(12 * 1024 * 1024 * 1024), "12.0 ГБ");
        assert_eq!(gigabytes(1 << 28), "0.3 ГБ");
        assert_eq!(gigabytes(0), "0.0 ГБ");
        assert_eq!(gigabytes(381 * (1 << 30) + (1 << 29)), "381.5 ГБ");
    }

    #[test]
    fn small_artefacts_jump_the_queue() {
        assert!(
            priority_by_kind(DownloadItemKind::Runtime) > priority_by_kind(DownloadItemKind::Mcp)
        );
        assert!(
            priority_by_kind(DownloadItemKind::Mcp) > priority_by_kind(DownloadItemKind::Embedding)
        );
        assert!(
            priority_by_kind(DownloadItemKind::Embedding)
                > priority_by_kind(DownloadItemKind::Model)
        );
    }
}
