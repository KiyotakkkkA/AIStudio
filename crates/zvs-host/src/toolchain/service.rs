use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zvs_contract::{
    AppError, AppErrorCode, CatalogueItemState, CatalogueSource, DownloadItemKind,
    DownloadListFilter, DownloadStatus, StartDownloadInput, ToolchainDto, ToolchainRuntime,
    ToolchainSelectInput, ToolchainSource, ToolchainStatus, VectorSourceKind,
};

use super::definitions::{
    TOOLCHAIN_DEFINITIONS, ToolchainCommand, Version, definition, node_file_key, primary_name,
    uv_asset_name,
};
use super::launch::{LaunchSpec, prepend_path};
use super::probe::{Probed, ProcessProbe, SharedProbe, primary_for, probe};
use super::process::path_dirs;
use super::releases::{ToolchainBuild, ToolchainResolver};
use crate::data::schema::DownloadRow;
use crate::downloads::{CatalogueItem, CatalogueProvider, DownloadInstaller, InstalledItem};
use crate::drivers::mcp::isolated_toolchain_env;
use crate::platform::archive::{ExtractOptions, extract_archive};
use crate::platform::logger::{LogLevel, SharedLogger};
use crate::platform::picker::SharedPicker;
use crate::runtimes::{Platform, RuntimeCatalogue, RuntimeDownloads};
use crate::services::SettingService;

pub const CONNECTED_KEY: &str = "toolchain.connected";
pub const ALREADY_DOWNLOADING: &str = "Эта среда уже загружается";
pub const NOT_MANAGED: &str = "Обновить можно только копию, загруженную приложением";
pub const NO_PICKER: &str = "Выбор файла недоступен";
pub const NO_CHECKSUM: &str = "Загрузка не содержит контрольной суммы и не будет подключена";
pub const SETTINGS_HINT: &str = "Подключите его в разделе «Настройки → Окружение».";
const STAGING_PREFIX: &str = ".staging-";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedRuntime {
    pub source: ToolchainSource,
    pub executable_path: String,
    pub version: String,
}

pub type ConnectedMap = BTreeMap<String, ConnectedRuntime>;

pub trait CommandLauncher: Send + Sync {
    fn launch_command(
        &self,
        command: &str,
        args: &[String],
    ) -> Option<Result<LaunchSpec, AppError>>;

    fn program_dirs(&self) -> Vec<PathBuf> {
        path_dirs(std::env::var_os("PATH").as_ref())
    }
}

#[derive(Clone)]
pub struct ToolchainServiceOptions {
    pub root: PathBuf,
    pub settings: SettingService,
    pub downloads: Arc<dyn RuntimeDownloads>,
    pub catalogue: Arc<dyn RuntimeCatalogue>,
    pub resolver: ToolchainResolver,
    pub probe: SharedProbe,
    pub picker: Option<SharedPicker>,
    pub path_env: Option<OsString>,
    pub logger: SharedLogger,
}

impl ToolchainServiceOptions {
    pub fn new(
        root: PathBuf,
        settings: SettingService,
        downloads: Arc<dyn RuntimeDownloads>,
        catalogue: Arc<dyn RuntimeCatalogue>,
        resolver: ToolchainResolver,
        logger: SharedLogger,
    ) -> Self {
        Self {
            root,
            settings,
            downloads,
            catalogue,
            resolver,
            probe: Arc::new(ProcessProbe::default()),
            picker: None,
            path_env: None,
            logger,
        }
    }
}

struct Inner {
    options: ToolchainServiceOptions,
    operation: tokio::sync::Mutex<()>,
}

#[derive(Clone)]
pub struct ToolchainService {
    inner: Arc<Inner>,
}

pub fn toolchain_catalogue_ref(runtime: ToolchainRuntime) -> String {
    format!("toolchain:{runtime}")
}

pub fn toolchain_archive_ref(build: &ToolchainBuild) -> String {
    format!(
        "toolchain:{}:{}:{}",
        build.runtime, build.version, build.asset_name
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainArchiveRef {
    pub runtime: ToolchainRuntime,
    pub version: Version,
    pub asset_name: String,
}

pub fn parse_toolchain_ref(item_ref: &str) -> Option<ToolchainArchiveRef> {
    let rest = item_ref.strip_prefix("toolchain:")?;
    let mut parts = rest.splitn(3, ':');
    let runtime = ToolchainRuntime::parse(parts.next()?)?;
    let version = Version::parse(parts.next()?)?;
    let asset_name = parts.next()?;
    let safe = !asset_name.is_empty()
        && !asset_name.contains(['/', '\\'])
        && asset_name != "."
        && asset_name != "..";
    safe.then(|| ToolchainArchiveRef {
        runtime,
        version,
        asset_name: asset_name.to_owned(),
    })
}

pub fn runtime_of_catalogue_ref(item_ref: &str) -> Option<ToolchainRuntime> {
    ToolchainRuntime::parse(item_ref.strip_prefix("toolchain:")?)
}

fn conflict(message: impl Into<String>) -> AppError {
    AppError::new(AppErrorCode::Conflict, message)
}

fn io_failure(error: &std::io::Error) -> AppError {
    AppError::new(AppErrorCode::Unknown, error.to_string())
}

fn find_file(root: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    if depth > 3 {
        return None;
    }
    let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(root)
        .map(|entries| entries.flatten().collect())
        .unwrap_or_default();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    let found = entries.iter().find(|entry| {
        entry.file_type().is_ok_and(|kind| kind.is_file())
            && entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
    });
    if let Some(found) = found {
        return Some(found.path());
    }
    entries
        .iter()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .find_map(|entry| find_file(&entry.path(), name, depth + 1))
}

async fn remove_tree(path: PathBuf) -> Result<(), AppError> {
    if !path.exists() {
        return Ok(());
    }
    tokio::task::spawn_blocking(move || std::fs::remove_dir_all(path))
        .await
        .map_err(|error| AppError::new(AppErrorCode::NativeError, error.to_string()))?
        .map_err(|error| io_failure(&error))
}

fn platform_supported(runtime: ToolchainRuntime, platform: Platform) -> bool {
    match runtime {
        ToolchainRuntime::Uv => uv_asset_name(platform).is_some(),
        ToolchainRuntime::Node => node_file_key(platform).is_some(),
    }
}

impl ToolchainService {
    pub fn new(options: ToolchainServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                operation: tokio::sync::Mutex::new(()),
            }),
        }
    }

    fn options(&self) -> &ToolchainServiceOptions {
        &self.inner.options
    }

    pub fn platform(&self) -> Platform {
        self.options().resolver.platform()
    }

    pub fn root(&self) -> &Path {
        &self.options().root
    }

    fn log(&self, level: LogLevel, message: &str, fields: &[(&str, Value)]) {
        self.options()
            .logger
            .log(level, "toolchain", message, fields);
    }

    fn runtime_dir(&self, runtime: ToolchainRuntime) -> PathBuf {
        self.root().join(runtime.as_str())
    }

    fn managed_dir(&self, runtime: ToolchainRuntime, version: Version) -> PathBuf {
        self.runtime_dir(runtime).join(version.to_string())
    }

    pub fn managed_versions(&self, runtime: ToolchainRuntime) -> Vec<Version> {
        let mut versions: Vec<Version> = std::fs::read_dir(self.runtime_dir(runtime))
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                    .filter_map(|entry| Version::parse(&entry.file_name().to_string_lossy()))
                    .collect()
            })
            .unwrap_or_default();
        versions.sort_by(|left, right| right.cmp(left));
        versions
    }

    pub fn connected(&self) -> ConnectedMap {
        let stored = self
            .options()
            .settings
            .get(CONNECTED_KEY)
            .ok()
            .flatten()
            .map(|setting| setting.value);
        let Some(Value::Object(entries)) = stored else {
            return ConnectedMap::new();
        };
        entries
            .into_iter()
            .filter(|(key, _)| ToolchainRuntime::parse(key).is_some())
            .filter_map(|(key, raw)| Some((key, serde_json::from_value(raw).ok()?)))
            .collect()
    }

    fn connection(&self, runtime: ToolchainRuntime) -> Option<ConnectedRuntime> {
        self.connected().remove(runtime.as_str())
    }

    fn save(&self, connected: &ConnectedMap) -> Result<(), AppError> {
        let value = serde_json::to_value(connected)
            .map_err(|error| AppError::new(AppErrorCode::Unknown, error.to_string()))?;
        self.options().settings.set(CONNECTED_KEY, &value)?;
        self.options().catalogue.invalidate();
        Ok(())
    }

    fn connect(
        &self,
        runtime: ToolchainRuntime,
        source: ToolchainSource,
        probed: &Probed,
    ) -> Result<(), AppError> {
        let mut connected = self.connected();
        connected.insert(
            runtime.as_str().to_owned(),
            ConnectedRuntime {
                source,
                executable_path: probed.executable.to_string_lossy().into_owned(),
                version: probed.version.to_string(),
            },
        );
        self.save(&connected)?;
        self.log(
            LogLevel::Info,
            "Connected a runtime",
            &[
                ("runtime", json!(runtime.as_str())),
                ("source", json!(source.as_str())),
                ("version", json!(probed.version.to_string())),
                ("executable", json!(probed.executable.to_string_lossy())),
            ],
        );
        Ok(())
    }

    async fn check(&self, runtime: ToolchainRuntime, executable: &Path) -> Result<Probed, String> {
        probe(
            self.options().probe.as_ref(),
            runtime,
            self.platform(),
            executable,
        )
        .await
    }

    fn active_refs(&self, runtime: ToolchainRuntime) -> Result<Vec<String>, AppError> {
        let mut seen = HashSet::new();
        Ok(self
            .options()
            .downloads
            .list(&DownloadListFilter {
                kinds: Some(vec![DownloadItemKind::Runtime]),
                statuses: None,
            })?
            .into_iter()
            .filter(|row| {
                matches!(
                    row.status,
                    DownloadStatus::Queued | DownloadStatus::Running | DownloadStatus::Paused
                )
            })
            .map(|row| row.item_ref)
            .filter(|item_ref| {
                parse_toolchain_ref(item_ref).is_some_and(|parsed| parsed.runtime == runtime)
            })
            .filter(|item_ref| seen.insert(item_ref.clone()))
            .collect())
    }

    fn describe(
        &self,
        runtime: ToolchainRuntime,
        latest: Option<Version>,
    ) -> Result<ToolchainDto, AppError> {
        let definition = definition(runtime);
        let record = self.connection(runtime);
        let refs = self.active_refs(runtime)?;
        let present = record
            .as_ref()
            .is_some_and(|record| Path::new(&record.executable_path).is_file());
        let status = if !refs.is_empty() {
            ToolchainStatus::Installing
        } else if record.is_none() {
            ToolchainStatus::Missing
        } else if present {
            ToolchainStatus::Ready
        } else {
            ToolchainStatus::Broken
        };
        let current = record
            .as_ref()
            .and_then(|record| Version::parse(&record.version));
        let update_available = status == ToolchainStatus::Ready
            && record
                .as_ref()
                .is_some_and(|record| record.source == ToolchainSource::Managed)
            && matches!((current, latest), (Some(current), Some(latest)) if latest > current);
        let dto = ToolchainDto {
            runtime,
            display_name: definition.display_name.to_owned(),
            status,
            source: record.as_ref().map(|record| record.source),
            version: record.as_ref().map(|record| record.version.clone()),
            executable_path: record.as_ref().map(|record| record.executable_path.clone()),
            minimum_version: definition.minimum.to_string(),
            latest_version: latest.map(|latest| latest.to_string()),
            update_available,
            managed_versions: self
                .managed_versions(runtime)
                .into_iter()
                .take(zvs_contract::toolchain::TOOLCHAIN_MANAGED_MAX)
                .map(|version| version.to_string())
                .collect(),
            problem: (status == ToolchainStatus::Broken).then(|| {
                format!(
                    "Исполняемый файл не найден: {}",
                    record
                        .as_ref()
                        .map(|record| record.executable_path.as_str())
                        .unwrap_or_default()
                )
            }),
            refs,
        };
        zvs_contract::Validate::validate(&dto)?;
        Ok(dto)
    }

    pub fn get(&self, runtime: ToolchainRuntime) -> Result<ToolchainDto, AppError> {
        self.describe(runtime, self.options().resolver.known_latest(runtime))
    }

    pub async fn list(&self, check_updates: bool) -> Result<Vec<ToolchainDto>, AppError> {
        let mut listed = Vec::new();
        for definition in &TOOLCHAIN_DEFINITIONS {
            let runtime = definition.runtime;
            let managed = self
                .connection(runtime)
                .is_some_and(|record| record.source == ToolchainSource::Managed);
            if check_updates
                && managed
                && platform_supported(runtime, self.platform())
                && let Err(error) = self.options().resolver.latest(runtime, false).await
            {
                self.log(
                    LogLevel::Warn,
                    "Could not check for a newer runtime",
                    &[
                        ("runtime", json!(runtime.as_str())),
                        ("error", json!(error.message)),
                    ],
                );
            }
            listed.push(self.get(runtime)?);
        }
        Ok(listed)
    }

    async fn connect_managed(&self, runtime: ToolchainRuntime, version: Version) -> Option<Probed> {
        let directory = self.managed_dir(runtime, version);
        let name = primary_name(runtime, self.platform());
        let executable = tokio::task::spawn_blocking(move || find_file(&directory, name, 0))
            .await
            .ok()
            .flatten()?;
        match self.check(runtime, &executable).await {
            Ok(probed) => Some(probed),
            Err(reason) => {
                self.log(
                    LogLevel::Warn,
                    "A managed runtime copy failed its probe",
                    &[
                        ("runtime", json!(runtime.as_str())),
                        ("version", json!(version.to_string())),
                        ("reason", json!(reason)),
                    ],
                );
                None
            }
        }
    }

    fn require_idle(&self, runtime: ToolchainRuntime) -> Result<(), AppError> {
        if self.active_refs(runtime)?.is_empty() {
            Ok(())
        } else {
            Err(conflict(ALREADY_DOWNLOADING).with_detail("runtime", runtime.as_str()))
        }
    }

    pub async fn install(&self, runtime: ToolchainRuntime) -> Result<ToolchainDto, AppError> {
        let _operation = self.inner.operation.lock().await;
        self.require_idle(runtime)?;
        for version in self.managed_versions(runtime) {
            if let Some(probed) = self.connect_managed(runtime, version).await {
                self.connect(runtime, ToolchainSource::Managed, &probed)?;
                return self.get(runtime);
            }
        }
        let build = self.options().resolver.latest(runtime, true).await?;
        self.queue(&build).await?;
        self.get(runtime)
    }

    pub async fn update(&self, runtime: ToolchainRuntime) -> Result<ToolchainDto, AppError> {
        let _operation = self.inner.operation.lock().await;
        let record = self
            .connection(runtime)
            .filter(|record| record.source == ToolchainSource::Managed)
            .ok_or_else(|| conflict(NOT_MANAGED).with_detail("runtime", runtime.as_str()))?;
        self.require_idle(runtime)?;
        let build = self.options().resolver.latest(runtime, true).await?;
        if Version::parse(&record.version).is_some_and(|current| current >= build.version) {
            return self.get(runtime);
        }
        if let Some(probed) = self.connect_managed(runtime, build.version).await {
            self.connect(runtime, ToolchainSource::Managed, &probed)?;
            self.prune(runtime, build.version).await;
            return self.get(runtime);
        }
        self.queue(&build).await?;
        self.get(runtime)
    }

    async fn queue(&self, build: &ToolchainBuild) -> Result<(), AppError> {
        let definition = definition(build.runtime);
        let version = build.version.to_string();
        let item = CatalogueItem {
            display_name: format!("{} {version}", definition.display_name),
            description: definition.description.to_owned(),
            version: Some(version.clone()),
            size_bytes: build.size_bytes,
            url: build.url.clone(),
            file_name: build.asset_name.clone(),
            checksum: Some(build.checksum.clone()),
            tags: vec!["окружение".to_owned(), version.clone()],
            ..CatalogueItem::new(
                toolchain_archive_ref(build),
                DownloadItemKind::Runtime,
                CatalogueSource::Github,
                format!("{}/{}", build.runtime, build.asset_name),
            )
        };
        let item_ref = item.item_ref.clone();
        self.options().catalogue.offer(item);
        self.options()
            .downloads
            .start(&StartDownloadInput::new(item_ref.clone()))
            .await?;
        self.options().catalogue.invalidate();
        self.log(
            LogLevel::Info,
            "Queued a runtime download",
            &[
                ("runtime", json!(build.runtime.as_str())),
                ("version", json!(version)),
                ("ref", json!(item_ref)),
            ],
        );
        Ok(())
    }

    fn path_candidates(&self, runtime: ToolchainRuntime) -> Vec<PathBuf> {
        let path = self
            .options()
            .path_env
            .clone()
            .or_else(|| std::env::var_os("PATH"))
            .unwrap_or_default();
        let name = primary_name(runtime, self.platform());
        let mut seen = HashSet::new();
        std::env::split_paths(&path)
            .filter(|directory| !directory.as_os_str().is_empty())
            .map(|directory| directory.join(name))
            .filter(|candidate| candidate.is_file())
            .filter(|candidate| {
                let key = candidate.to_string_lossy().to_lowercase();
                seen.insert(key)
            })
            .collect()
    }

    pub async fn scan(&self, runtime: ToolchainRuntime) -> Result<ToolchainDto, AppError> {
        let _operation = self.inner.operation.lock().await;
        let definition = definition(runtime);
        let candidates = self.path_candidates(runtime);
        if candidates.is_empty() {
            return Err(AppError::new(
                AppErrorCode::NotFound,
                format!("{} не найден в PATH", definition.display_name),
            )
            .with_detail("runtime", runtime.as_str()));
        }
        let mut refusals = Vec::new();
        for candidate in candidates {
            match self.check(runtime, &candidate).await {
                Ok(probed) => {
                    self.connect(runtime, ToolchainSource::Path, &probed)?;
                    return self.get(runtime);
                }
                Err(reason) => refusals.push(reason),
            }
        }
        Err(conflict(format!(
            "Найденная в PATH копия {} не подходит: {}",
            definition.display_name,
            refusals.first().map(String::as_str).unwrap_or_default()
        ))
        .with_detail("runtime", runtime.as_str())
        .with_detail("reasons", json!(refusals)))
    }

    pub async fn select(&self, input: &ToolchainSelectInput) -> Result<ToolchainDto, AppError> {
        zvs_contract::Validate::validate(input)?;
        let runtime = input.runtime;
        let picked = match &input.path {
            Some(path) => PathBuf::from(path),
            None => {
                let picker = self
                    .options()
                    .picker
                    .as_ref()
                    .ok_or_else(|| AppError::new(AppErrorCode::NotImplemented, NO_PICKER))?;
                match picker
                    .pick(VectorSourceKind::File)
                    .await?
                    .into_iter()
                    .next()
                {
                    Some(path) => PathBuf::from(path),
                    None => return self.get(runtime),
                }
            }
        };
        let _operation = self.inner.operation.lock().await;
        let executable = primary_for(runtime, self.platform(), &picked);
        let probed = self.check(runtime, &executable).await.map_err(|reason| {
            conflict(format!("Файл не подходит: {reason}"))
                .with_detail("runtime", runtime.as_str())
                .with_detail("path", picked.to_string_lossy().into_owned())
        })?;
        self.connect(runtime, ToolchainSource::Manual, &probed)?;
        self.get(runtime)
    }

    pub async fn remove(&self, runtime: ToolchainRuntime) -> Result<ToolchainDto, AppError> {
        let _operation = self.inner.operation.lock().await;
        self.require_idle(runtime)?;
        let mut connected = self.connected();
        let record = connected.remove(runtime.as_str());
        self.save(&connected)?;
        let copies = self.managed_versions(runtime);
        for version in &copies {
            remove_tree(self.managed_dir(runtime, *version)).await?;
        }
        self.options()
            .downloads
            .forget_by_ref(&move |item_ref: &str| {
                parse_toolchain_ref(item_ref).is_some_and(|parsed| parsed.runtime == runtime)
            })
            .await?;
        self.log(
            LogLevel::Info,
            "Disconnected a runtime",
            &[
                ("runtime", json!(runtime.as_str())),
                ("source", json!(record.map(|record| record.source.as_str()))),
                ("removedCopies", json!(copies.len())),
            ],
        );
        self.get(runtime)
    }

    async fn prune(&self, runtime: ToolchainRuntime, keep: Version) {
        for version in self.managed_versions(runtime) {
            if version == keep {
                continue;
            }
            if let Err(error) = remove_tree(self.managed_dir(runtime, version)).await {
                self.log(
                    LogLevel::Warn,
                    "Could not remove an older runtime copy",
                    &[
                        ("runtime", json!(runtime.as_str())),
                        ("version", json!(version.to_string())),
                        ("error", json!(error.message)),
                    ],
                );
            }
        }
    }

    pub async fn unpack_download(&self, row: &DownloadRow) -> Result<(), AppError> {
        let Some(parsed) = parse_toolchain_ref(&row.item_ref) else {
            return Ok(());
        };
        if row
            .checksum_value
            .as_deref()
            .is_none_or(|value| value.is_empty())
        {
            return Err(conflict(NO_CHECKSUM).with_detail("ref", row.item_ref.clone()));
        }
        let _operation = self.inner.operation.lock().await;
        let runtime = parsed.runtime;
        let staging = self
            .runtime_dir(runtime)
            .join(format!("{STAGING_PREFIX}{}", parsed.version));
        remove_tree(staging.clone()).await?;
        let outcome = self.stage(row, &parsed, &staging).await;
        let _ = std::fs::remove_file(&row.target_path);
        let probed = match outcome {
            Ok(probed) => probed,
            Err(error) => {
                let _ = remove_tree(staging).await;
                return Err(error);
            }
        };
        let target = self.managed_dir(runtime, parsed.version);
        let relative = probed
            .executable
            .strip_prefix(&staging)
            .map(Path::to_path_buf)
            .map_err(|_| AppError::new(AppErrorCode::Unknown, "executable outside staging"))?;
        let in_use = self
            .connection(runtime)
            .is_some_and(|record| Path::new(&record.executable_path).starts_with(&target));
        if in_use {
            remove_tree(staging).await?;
        } else {
            remove_tree(target.clone()).await?;
            std::fs::rename(&staging, &target).map_err(|error| io_failure(&error))?;
        }
        let installed = Probed {
            executable: target.join(relative),
            version: probed.version,
        };
        self.connect(runtime, ToolchainSource::Managed, &installed)?;
        self.prune(runtime, parsed.version).await;
        Ok(())
    }

    async fn stage(
        &self,
        row: &DownloadRow,
        parsed: &ToolchainArchiveRef,
        staging: &Path,
    ) -> Result<Probed, AppError> {
        let runtime = parsed.runtime;
        let definition = definition(runtime);
        extract_archive(
            Path::new(&row.target_path),
            staging,
            ExtractOptions::default(),
        )
        .await?;
        let name = primary_name(runtime, self.platform());
        let root = staging.to_path_buf();
        let executable = tokio::task::spawn_blocking(move || find_file(&root, name, 0))
            .await
            .ok()
            .flatten()
            .ok_or_else(|| conflict(format!("В архиве {} нет {name}", definition.display_name)))?;
        let probed = self.check(runtime, &executable).await.map_err(|reason| {
            conflict(format!(
                "Загруженная копия {} не прошла проверку: {reason}",
                definition.display_name
            ))
            .with_detail("runtime", runtime.as_str())
        })?;
        if probed.version != parsed.version {
            return Err(conflict(format!(
                "Архив содержит {} {} вместо {}",
                definition.display_name, probed.version, parsed.version
            )));
        }
        Ok(probed)
    }

    pub fn isolated_env(&self) -> BTreeMap<String, String> {
        isolated_toolchain_env(self.root())
    }

    pub fn launch(
        &self,
        command: ToolchainCommand,
        args: &[String],
    ) -> Result<LaunchSpec, AppError> {
        let runtime = command.runtime();
        let definition = definition(runtime);
        let missing = |message: String| {
            AppError::new(AppErrorCode::NotFound, message)
                .with_detail("runtime", runtime.as_str())
                .with_detail("command", command.name())
        };
        let record = self.connection(runtime).ok_or_else(|| {
            missing(format!(
                "Для команды «{}» нужен {}. {SETTINGS_HINT}",
                command.name(),
                definition.display_name
            ))
        })?;
        let primary = PathBuf::from(&record.executable_path);
        let file = primary
            .parent()
            .map(|directory| directory.join(command.file_name(self.platform())))
            .filter(|file| primary.is_file() && file.is_file())
            .ok_or_else(|| {
                missing(format!(
                    "Подключённая копия {} не найдена на диске. {SETTINGS_HINT}",
                    definition.display_name
                ))
            })?;
        let directory = primary.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut env = self.isolated_env();
        let current = self
            .options()
            .path_env
            .clone()
            .or_else(|| std::env::var_os("PATH"));
        env.insert(
            "PATH".to_owned(),
            prepend_path(&directory, current.as_ref()),
        );
        Ok(LaunchSpec {
            command: file.to_string_lossy().into_owned(),
            args: args.to_vec(),
            env,
            cwd: None,
        })
    }

    pub fn catalogue_provider(&self) -> Arc<dyn CatalogueProvider> {
        Arc::new(ToolchainCatalogueProvider {
            service: self.clone(),
        })
    }

    fn catalogue_rows(&self) -> Result<Vec<CatalogueItem>, AppError> {
        let platform = self.platform();
        let mut rows = Vec::new();
        for definition in &TOOLCHAIN_DEFINITIONS {
            if !platform_supported(definition.runtime, platform) {
                continue;
            }
            let dto = self.get(definition.runtime)?;
            let state = match dto.status {
                ToolchainStatus::Installing => CatalogueItemState::Downloading,
                ToolchainStatus::Ready if dto.update_available => CatalogueItemState::Update,
                ToolchainStatus::Ready => CatalogueItemState::Installed,
                ToolchainStatus::Missing | ToolchainStatus::Broken => CatalogueItemState::Available,
            };
            let mut tags = vec!["окружение".to_owned()];
            if let Some(source) = dto.source {
                tags.push(
                    match source {
                        ToolchainSource::Managed => "загружено приложением",
                        ToolchainSource::Path => "из PATH",
                        ToolchainSource::Manual => "указано вручную",
                    }
                    .to_owned(),
                );
            }
            rows.push(CatalogueItem {
                display_name: definition.display_name.to_owned(),
                description: definition.description.to_owned(),
                size_bytes: definition.approximate_bytes,
                url: definition.page.to_owned(),
                file_name: format!("{}.zip", definition.runtime),
                tags,
                state: Some(state),
                version: dto.latest_version.clone().or_else(|| dto.version.clone()),
                ..CatalogueItem::new(
                    toolchain_catalogue_ref(definition.runtime),
                    DownloadItemKind::Runtime,
                    CatalogueSource::Github,
                    definition.runtime.as_str(),
                )
            });
        }
        Ok(rows)
    }
}

impl CommandLauncher for ToolchainService {
    fn launch_command(
        &self,
        command: &str,
        args: &[String],
    ) -> Option<Result<LaunchSpec, AppError>> {
        ToolchainCommand::parse(command).map(|command| self.launch(command, args))
    }

    fn program_dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = TOOLCHAIN_DEFINITIONS
            .iter()
            .filter_map(|definition| self.connection(definition.runtime))
            .map(|record| PathBuf::from(record.executable_path))
            .filter(|primary| primary.is_file())
            .filter_map(|primary| primary.parent().map(Path::to_path_buf))
            .collect();
        let current = self
            .options()
            .path_env
            .clone()
            .or_else(|| std::env::var_os("PATH"));
        for dir in path_dirs(current.as_ref()) {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
        dirs
    }
}

struct ToolchainCatalogueProvider {
    service: ToolchainService,
}

#[async_trait]
impl CatalogueProvider for ToolchainCatalogueProvider {
    fn id(&self) -> CatalogueSource {
        CatalogueSource::Github
    }

    fn live(&self) -> bool {
        false
    }

    async fn list(&self) -> Result<Vec<CatalogueItem>, AppError> {
        self.service.catalogue_rows()
    }

    async fn installed(&self) -> Result<Vec<InstalledItem>, AppError> {
        Ok(self
            .service
            .connected()
            .into_iter()
            .map(|(name, record)| InstalledItem {
                kind: DownloadItemKind::Runtime,
                name,
                version: Some(record.version),
                size_bytes: 0,
                digest: None,
            })
            .collect())
    }
}

#[async_trait]
impl DownloadInstaller for ToolchainService {
    async fn install(&self, row: &DownloadRow) -> Result<(), AppError> {
        self.unpack_download(row).await
    }
}
