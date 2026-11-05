mod runner;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zvs_contract::artifacts::{
    DASHBOARD_ERROR_MAX, DASHBOARD_INTERVAL_MAX, DASHBOARD_INTERVAL_MIN,
    DASHBOARD_SCRIPT_SOURCE_MAX, DASHBOARD_SCRIPTS_MAX,
};
use zvs_contract::{AppError, AppErrorCode, DashboardScriptDto, DashboardStatusDto};

pub use runner::ProcessDashboardRunner;

pub const LIVE_DIR: &str = ".zvs";
pub const SETTINGS_FILE: &str = "settings.json";
pub const NOT_LIVE: &str = "Дашборд собран без сохранённой спецификации и не подлежит обновлению.";
pub const NEEDS_APPROVAL: &str = "Для обновления в реальном времени подтверждите примемение скриптов сбора данных со страницы «Галерея»";
pub const FETCH_FAILED: &str = "Получение данных завершилось с ошибкой";
pub const BUILD_FAILED: &str = "Не удалось обновить дашборд";
pub const STALE_APPROVAL: &str =
    "Правила обновления дашборда изменились, обновите их на странице «Галерея»";
pub const SETTINGS_FAILED: &str = "Не удалось сохранить настройки дашборда";

#[async_trait]
pub trait DashboardRunner: Send + Sync {
    async fn fetch(&self, script: &Path, output: &Path, cwd: &Path) -> Result<(), AppError>;
    async fn build(&self, library: &Path, stem: &str) -> Result<(), AppError>;
}

pub type SharedDashboardRunner = Arc<dyn DashboardRunner>;

#[derive(Debug, Clone, Deserialize)]
struct FetchStep {
    script: String,
    output: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Meta {
    #[serde(default)]
    interval: u32,
    #[serde(default)]
    fetch: Vec<FetchStep>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Preference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    interval: Option<u32>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    approved: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Settings {
    #[serde(default)]
    dashboards: BTreeMap<String, Preference>,
}

#[derive(Debug, Clone, Default)]
struct Runtime {
    refreshing: bool,
    error: Option<String>,
    finished: Option<Instant>,
}

struct Script {
    name: String,
    path: PathBuf,
    output: PathBuf,
    sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageState {
    pub interval: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refreshed_at: Option<i64>,
    pub refreshing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub needs_approval: bool,
}

struct Inner {
    library: PathBuf,
    runner: SharedDashboardRunner,
    settings: Mutex<()>,
    runtime: Mutex<HashMap<String, Runtime>>,
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

#[derive(Clone)]
pub struct DashboardService {
    inner: Arc<Inner>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sha256(path: &Path) -> Result<String, AppError> {
    let bytes = std::fs::read(path).map_err(|_| AppError::new(AppErrorCode::NotFound, NOT_LIVE))?;
    Ok(hex(&Sha256::digest(&bytes)))
}

fn millis(time: SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
}

fn clipped(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn contained(base: &Path, relative: &str) -> Option<PathBuf> {
    let path = Path::new(relative);
    let plain = path
        .components()
        .all(|component| matches!(component, Component::Normal(_)));
    (plain && !relative.is_empty()).then(|| base.join(path))
}

fn stem_of(name: &str) -> Option<&str> {
    let stem = name
        .strip_suffix(".html")
        .or_else(|| name.strip_suffix(".HTML"))?;
    let plain = !stem.is_empty()
        && stem != "."
        && stem != ".."
        && !stem.contains(['/', '\\'])
        && stem != LIVE_DIR;
    plain.then_some(stem)
}

pub fn effective_interval(spec: u32, chosen: Option<u32>) -> u32 {
    chosen.unwrap_or(spec)
}

impl DashboardService {
    pub fn new(library: PathBuf, runner: SharedDashboardRunner) -> Self {
        Self {
            inner: Arc::new(Inner {
                library,
                runner,
                settings: Mutex::new(()),
                runtime: Mutex::new(HashMap::new()),
                locks: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn library(&self) -> &Path {
        &self.inner.library
    }

    fn folder(&self, stem: &str) -> PathBuf {
        self.inner.library.join(LIVE_DIR).join(stem)
    }

    fn settings_path(&self) -> PathBuf {
        self.inner.library.join(LIVE_DIR).join(SETTINGS_FILE)
    }

    pub fn is_live(&self, name: &str) -> bool {
        stem_of(name).is_some_and(|stem| self.folder(stem).join("meta.json").is_file())
    }

    fn meta(&self, stem: &str) -> Option<Meta> {
        let text = std::fs::read_to_string(self.folder(stem).join("meta.json")).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn read_settings(&self) -> Settings {
        std::fs::read_to_string(self.settings_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn update_settings(
        &self,
        stem: &str,
        change: impl FnOnce(&mut Preference),
    ) -> Result<(), AppError> {
        let _guard = self
            .inner
            .settings
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut settings = self.read_settings();
        let entry = settings.dashboards.entry(stem.to_owned()).or_default();
        change(entry);
        if entry.interval.is_none() && entry.approved.is_empty() {
            settings.dashboards.remove(stem);
        }
        let path = self.settings_path();
        let failed = |cause: std::io::Error| {
            AppError::new(AppErrorCode::Unknown, SETTINGS_FAILED)
                .with_detail("raw", cause.to_string())
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(failed)?;
        }
        let text = serde_json::to_string_pretty(&settings).unwrap_or_default();
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, text).map_err(failed)?;
        std::fs::rename(&temporary, &path).map_err(failed)
    }

    fn scripts(&self, stem: &str, meta: &Meta) -> Result<Vec<Script>, AppError> {
        let fetch = self.folder(stem);
        meta.fetch
            .iter()
            .take(DASHBOARD_SCRIPTS_MAX)
            .map(|step| {
                let path = contained(&fetch, &step.script)
                    .ok_or_else(|| AppError::new(AppErrorCode::ValidationFailed, NOT_LIVE))?;
                let output = contained(&fetch, &step.output)
                    .ok_or_else(|| AppError::new(AppErrorCode::ValidationFailed, NOT_LIVE))?;
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                Ok(Script {
                    sha256: sha256(&path)?,
                    name,
                    path,
                    output,
                })
            })
            .collect()
    }

    fn runtime(&self, stem: &str) -> Runtime {
        self.inner
            .runtime
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(stem)
            .cloned()
            .unwrap_or_default()
    }

    fn set_runtime(&self, stem: &str, change: impl FnOnce(&mut Runtime)) {
        let mut runtime = self
            .inner
            .runtime
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        change(runtime.entry(stem.to_owned()).or_default());
    }

    fn lock(&self, stem: &str) -> Arc<tokio::sync::Mutex<()>> {
        Arc::clone(
            self.inner
                .locks
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(stem.to_owned())
                .or_default(),
        )
    }

    fn refreshed_at(&self, stem: &str) -> Option<i64> {
        std::fs::metadata(self.folder(stem).join("data.json"))
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(millis)
    }

    fn stem(name: &str) -> Result<&str, AppError> {
        stem_of(name).ok_or_else(|| AppError::new(AppErrorCode::ValidationFailed, NOT_LIVE))
    }

    pub fn status(&self, id: &str, name: &str) -> Result<DashboardStatusDto, AppError> {
        let stem = Self::stem(name)?;
        let preference = self
            .read_settings()
            .dashboards
            .get(stem)
            .cloned()
            .unwrap_or_default();
        let runtime = self.runtime(stem);
        let Some(meta) = self.meta(stem) else {
            return Ok(DashboardStatusDto {
                id: id.to_owned(),
                live: false,
                interval: 0,
                spec_interval: 0,
                overridden: false,
                refreshed_at: None,
                refreshing: false,
                error: None,
                scripts: Vec::new(),
            });
        };
        let scripts = self
            .scripts(stem, &meta)?
            .into_iter()
            .map(|script| {
                let source = std::fs::read_to_string(&script.path).unwrap_or_default();
                let truncated = source.chars().count() > DASHBOARD_SCRIPT_SOURCE_MAX;
                DashboardScriptDto {
                    approved: preference.approved.contains(&script.sha256),
                    name: clipped(&script.name, zvs_contract::artifacts::ARTIFACT_NAME_MAX),
                    sha256: script.sha256,
                    source: clipped(&source, DASHBOARD_SCRIPT_SOURCE_MAX),
                    truncated,
                }
            })
            .collect();
        let spec_interval = Self::bounded(meta.interval);
        Ok(DashboardStatusDto {
            id: id.to_owned(),
            live: true,
            interval: effective_interval(spec_interval, preference.interval.map(Self::bounded)),
            spec_interval,
            overridden: preference.interval.is_some(),
            refreshed_at: self.refreshed_at(stem),
            refreshing: runtime.refreshing,
            error: runtime
                .error
                .map(|error| clipped(&error, DASHBOARD_ERROR_MAX)),
            scripts,
        })
    }

    fn bounded(interval: u32) -> u32 {
        if interval == 0 {
            0
        } else {
            interval.clamp(DASHBOARD_INTERVAL_MIN, DASHBOARD_INTERVAL_MAX)
        }
    }

    pub fn page_state(&self, id: &str, name: &str) -> Result<PageState, AppError> {
        let status = self.status(id, name)?;
        Ok(PageState {
            interval: status.interval,
            refreshed_at: status.refreshed_at,
            refreshing: status.refreshing,
            error: status.error,
            needs_approval: status.scripts.iter().any(|script| !script.approved),
        })
    }

    pub fn data(&self, name: &str) -> Result<Vec<u8>, AppError> {
        let stem = Self::stem(name)?;
        std::fs::read(self.folder(stem).join("data.json"))
            .map_err(|_| AppError::new(AppErrorCode::NotFound, NOT_LIVE))
    }

    pub async fn refresh(
        &self,
        id: &str,
        name: &str,
        fresh_within: Duration,
    ) -> Result<DashboardStatusDto, AppError> {
        let stem = Self::stem(name)?.to_owned();
        let lock = self.lock(&stem);
        let waited = lock.try_lock().is_err();
        let _guard = lock.lock().await;
        let recent = self
            .runtime(&stem)
            .finished
            .is_some_and(|finished| finished.elapsed() < fresh_within);
        if waited || recent {
            return self.status(id, name);
        }
        self.set_runtime(&stem, |runtime| runtime.refreshing = true);
        let outcome = self.run(&stem).await;
        self.set_runtime(&stem, |runtime| {
            runtime.refreshing = false;
            runtime.finished = Some(Instant::now());
            runtime.error = outcome.as_ref().err().map(|error| error.message.clone());
        });
        outcome?;
        self.status(id, name)
    }

    async fn run(&self, stem: &str) -> Result<(), AppError> {
        let meta = self
            .meta(stem)
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, NOT_LIVE))?;
        let approved = self
            .read_settings()
            .dashboards
            .get(stem)
            .map(|preference| preference.approved.clone())
            .unwrap_or_default();
        let scripts = self.scripts(stem, &meta)?;
        if scripts
            .iter()
            .any(|script| !approved.contains(&script.sha256))
        {
            return Err(AppError::new(
                AppErrorCode::PermissionDenied,
                NEEDS_APPROVAL,
            ));
        }
        let folder = self.folder(stem);
        for script in &scripts {
            let cwd = script.path.parent().unwrap_or(&folder).to_path_buf();
            self.inner
                .runner
                .fetch(&script.path, &script.output, &cwd)
                .await?;
        }
        self.inner.runner.build(&self.inner.library, stem).await
    }

    pub fn approve(
        &self,
        id: &str,
        name: &str,
        hashes: &[String],
    ) -> Result<DashboardStatusDto, AppError> {
        let stem = Self::stem(name)?;
        let meta = self
            .meta(stem)
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, NOT_LIVE))?;
        let current: BTreeSet<String> = self
            .scripts(stem, &meta)?
            .into_iter()
            .map(|script| script.sha256)
            .collect();
        let asked: BTreeSet<String> = hashes.iter().cloned().collect();
        if asked != current {
            return Err(AppError::new(AppErrorCode::Conflict, STALE_APPROVAL));
        }
        self.update_settings(stem, |preference| preference.approved = current)?;
        self.set_runtime(stem, |runtime| {
            if runtime.error.as_deref() == Some(NEEDS_APPROVAL) {
                runtime.error = None;
            }
        });
        self.status(id, name)
    }

    pub fn set_interval(
        &self,
        id: &str,
        name: &str,
        interval: Option<u32>,
    ) -> Result<DashboardStatusDto, AppError> {
        let stem = Self::stem(name)?;
        if self.meta(stem).is_none() {
            return Err(AppError::new(AppErrorCode::NotFound, NOT_LIVE));
        }
        if let Some(value) = interval
            && value != 0
            && !(DASHBOARD_INTERVAL_MIN..=DASHBOARD_INTERVAL_MAX).contains(&value)
        {
            return Err(zvs_contract::validate::invalid("interval", "range"));
        }
        self.update_settings(stem, |preference| preference.interval = interval)?;
        self.status(id, name)
    }

    pub fn forget(&self, name: &str) -> Result<(), AppError> {
        let stem = Self::stem(name)?;
        let folder = self.folder(stem);
        if folder.is_dir() {
            std::fs::remove_dir_all(&folder).map_err(|cause| {
                AppError::new(AppErrorCode::Unknown, SETTINGS_FAILED)
                    .with_detail("raw", cause.to_string())
            })?;
        }
        self.update_settings(stem, |preference| *preference = Preference::default())?;
        self.inner
            .runtime
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(stem);
        Ok(())
    }
}
