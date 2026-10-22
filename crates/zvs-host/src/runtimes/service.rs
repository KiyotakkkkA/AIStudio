use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    Accelerator, AppError, AppErrorCode, CatalogueItemState, CatalogueSource, DownloadDto,
    DownloadItemKind, DownloadListFilter, DownloadStatus, InstallRuntimeInput, ModelFormat,
    RuntimeDto, RuntimeOverviewDto, RuntimePlanDto, RuntimeState, StartDownloadInput, Validate,
    VectorOcrLanguage,
};

use super::definitions::{
    AcceleratorPlan, Platform, RUNTIME_DEFINITIONS, RuntimeDefinition, definition_of, format_of,
    plan_accelerator,
};
use super::llama_server::{LlamaServer, LlamaServerOptions, LlamaServerRole, SharedEngineServer};
use super::releases::{LLAMA_RELEASES_PAGE, ReleaseResolver, ResolvedBuild};
use crate::data::schema::DownloadRow;
use crate::downloads::{
    CatalogueItem, CatalogueProvider, CatalogueService, DownloadInstaller, DownloadService,
    InstalledItem, RefMatcher,
};
use crate::drivers::ai::HttpClient;
use crate::drivers::ai::adapters::LocalEmbeddingEngine;
use crate::platform::archive::{ExtractOptions, extract_archive};
use crate::platform::device::DeviceProbe;
use crate::platform::logger::{LogLevel, SharedLogger};
use crate::platform::paths::{DownloadKind, runtime_install_path};
use crate::services::SettingService;
use crate::services::rerank::LocalRerankEngine;

pub const INSTALLED_KEY: &str = "runtimes.installed";
pub const RECOMMENDED_MODEL_REFS: &[&str] = &["curated:embedding:bge-m3-f16"];
pub const NO_SUCH_ENGINE: &str = "Такого движка нет";
pub const NOT_INSTALLED: &str = "Этот движок не установлен";
pub const ALREADY_INSTALLING: &str = "Этот движок уже устанавливается";
pub const NO_ENGINE: &str =
    "Локальный движок не установлен. Откройте «Загрузки» и установите llama.cpp.";
pub const NO_ONNX_ENGINE: &str =
    "Для файлов .onnx движок ещё не подключён: используйте модель в формате GGUF";
pub const NO_PROJECTOR: &str = "Рядом с моделью нет файла mmproj: скачайте проектор в «Загрузках», без него распознавание не запустится";
pub const UNSUPPORTED_PLATFORM: &str = "Для этой операционной системы такой сборки не существует";
const SERVER_NAMES: [&str; 2] = ["llama-server", "llama-server.exe"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledRuntime {
    pub tag: String,
    pub install_path: String,
    pub executable_path: String,
    pub size_bytes: u64,
}

pub type InstalledMap = BTreeMap<String, InstalledRuntime>;

#[async_trait]
pub trait RuntimeDownloads: Send + Sync {
    async fn start(&self, input: &StartDownloadInput) -> Result<DownloadDto, AppError>;

    fn list(&self, filter: &DownloadListFilter) -> Result<Vec<DownloadDto>, AppError>;

    async fn forget_by_ref(&self, matches: &RefMatcher) -> Result<usize, AppError>;
}

#[async_trait]
impl RuntimeDownloads for DownloadService {
    async fn start(&self, input: &StartDownloadInput) -> Result<DownloadDto, AppError> {
        Self::start(self, input).await
    }

    fn list(&self, filter: &DownloadListFilter) -> Result<Vec<DownloadDto>, AppError> {
        Self::list(self, filter)
    }

    async fn forget_by_ref(&self, matches: &RefMatcher) -> Result<usize, AppError> {
        Self::forget_by_ref(self, matches).await
    }
}

#[async_trait]
pub trait RuntimeCatalogue: Send + Sync {
    fn offer(&self, item: CatalogueItem) -> CatalogueItem;

    async fn find(&self, item_ref: &str) -> Result<CatalogueItem, AppError>;

    fn invalidate(&self) {}
}

#[async_trait]
impl RuntimeCatalogue for CatalogueService {
    fn offer(&self, item: CatalogueItem) -> CatalogueItem {
        Self::offer(self, item)
    }

    fn invalidate(&self) {
        Self::invalidate(self);
    }

    async fn find(&self, item_ref: &str) -> Result<CatalogueItem, AppError> {
        Self::find(self, item_ref).await
    }
}

#[async_trait]
pub trait RuntimeDevice: Send + Sync {
    async fn profile(&self) -> Option<zvs_contract::DeviceProfileDto>;
}

#[async_trait]
impl RuntimeDevice for DeviceProbe {
    async fn profile(&self) -> Option<zvs_contract::DeviceProfileDto> {
        Self::profile(self, false).await.ok()
    }
}

pub type ServerFactory = Arc<dyn Fn(LlamaServerOptions) -> SharedEngineServer + Send + Sync>;

#[derive(Clone)]
pub struct RuntimeServiceOptions {
    pub downloads: Arc<dyn RuntimeDownloads>,
    pub catalogue: Arc<dyn RuntimeCatalogue>,
    pub settings: SettingService,
    pub downloads_dir: PathBuf,
    pub device: Option<Arc<dyn RuntimeDevice>>,
    pub resolver: ReleaseResolver,
    pub http: HttpClient,
    pub logger: SharedLogger,
    pub create_server: Option<ServerFactory>,
    pub context_size: Option<u32>,
}

struct Inner {
    options: RuntimeServiceOptions,
    servers: Mutex<BTreeMap<String, SharedEngineServer>>,
    installing: Mutex<HashSet<String>>,
    recommended: Mutex<Accelerator>,
    stopping: Mutex<Vec<SharedEngineServer>>,
}

#[derive(Clone)]
pub struct RuntimeService {
    inner: Arc<Inner>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn runtime_ref(runtime_id: &str, tag: &str, asset_name: &str) -> String {
    format!("runtime:{runtime_id}:{tag}:{asset_name}")
}

pub fn runtime_catalogue_ref(runtime_id: &str) -> String {
    format!("runtime:{runtime_id}")
}

fn catalogue_ref_pattern() -> Regex {
    Regex::new(r"^runtime:([a-z0-9-]+)$").expect("a valid pattern")
}

fn archive_ref_pattern() -> Regex {
    Regex::new(r"^runtime:([a-z0-9-]+):(b\d+):(.+)$").expect("a valid pattern")
}

pub fn runtime_id_of_catalogue_ref(item_ref: &str) -> Option<String> {
    catalogue_ref_pattern()
        .captures(item_ref)
        .and_then(|captures| captures.get(1))
        .map(|id| id.as_str().to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeArchiveRef {
    pub runtime_id: String,
    pub tag: String,
    pub asset_name: String,
}

pub fn parse_runtime_ref(item_ref: &str) -> Option<RuntimeArchiveRef> {
    let captures = archive_ref_pattern().captures(item_ref)?;
    Some(RuntimeArchiveRef {
        runtime_id: captures.get(1)?.as_str().to_owned(),
        tag: captures.get(2)?.as_str().to_owned(),
        asset_name: captures.get(3)?.as_str().to_owned(),
    })
}

pub fn ocr_prompt(language: VectorOcrLanguage) -> String {
    let hint = match language {
        VectorOcrLanguage::Auto => "",
        VectorOcrLanguage::Rus => " Текст на русском языке.",
        VectorOcrLanguage::Eng => " The text is in English.",
        VectorOcrLanguage::RusEng => " В тексте есть и русский, и английский.",
    };
    format!(
        "Прочитай весь текст на изображении и выпиши его целиком, сохраняя порядок строк, \
         абзацы и структуру таблиц. Не переводи, не пересказывай и не добавляй комментариев. \
         Если текста нет, ответь пустой строкой.{hint}"
    )
}

fn overlap(candidate: &str, stem: &str) -> usize {
    let name = candidate.to_lowercase();
    stem.split(['-', '_', '.'])
        .filter(|part| part.chars().count() > 2 && name.contains(part))
        .count()
}

const fn rank(accelerator: Accelerator) -> u8 {
    match accelerator {
        Accelerator::Cpu => 0,
        _ => 1,
    }
}

fn file_name(path: &str) -> String {
    Path::new(path).file_name().map_or_else(
        || path.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

fn sorted_entries(directory: &Path) -> Vec<std::fs::DirEntry> {
    let mut entries: Vec<std::fs::DirEntry> = std::fs::read_dir(directory)
        .map(|entries| entries.flatten().collect())
        .unwrap_or_default();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    entries
}

pub fn find_executable(root: &Path, depth: usize) -> Option<PathBuf> {
    if depth > 4 {
        return None;
    }
    let entries = sorted_entries(root);
    let found = entries.iter().find(|entry| {
        entry.file_type().is_ok_and(|kind| kind.is_file())
            && SERVER_NAMES.contains(&entry.file_name().to_string_lossy().to_lowercase().as_str())
    });
    if let Some(found) = found {
        return Some(found.path());
    }
    entries
        .iter()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .find_map(|entry| find_executable(&entry.path(), depth + 1))
}

fn unsupported_format(name: &str) -> AppError {
    AppError::new(
        AppErrorCode::UnsupportedFormat,
        format!(
            "Формат файла «{}» не поддерживается локальным движком",
            file_name(name)
        ),
    )
}

fn path_failure(error: &crate::platform::paths::PathError) -> AppError {
    AppError::new(AppErrorCode::Unknown, error.to_string())
}

impl RuntimeService {
    pub fn new(options: RuntimeServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                servers: Mutex::new(BTreeMap::new()),
                installing: Mutex::new(HashSet::new()),
                recommended: Mutex::new(Accelerator::Cpu),
                stopping: Mutex::new(Vec::new()),
            }),
        }
    }

    fn platform(&self) -> Platform {
        self.inner.options.resolver.platform()
    }

    fn log(&self, level: LogLevel, message: &str, fields: &[(&str, Value)]) {
        self.inner
            .options
            .logger
            .log(level, "runtimes", message, fields);
    }

    async fn device(&self) -> Option<zvs_contract::DeviceProfileDto> {
        match &self.inner.options.device {
            Some(device) => device.profile().await,
            None => None,
        }
    }

    async fn refresh_recommendation(
        &self,
    ) -> (AcceleratorPlan, Option<zvs_contract::DeviceProfileDto>) {
        let device = self.device().await;
        let plan = plan_accelerator(device.as_ref(), self.platform());
        *lock(&self.inner.recommended) = plan.accelerator;
        (plan, device)
    }

    pub fn installed(&self) -> InstalledMap {
        let stored = self
            .inner
            .options
            .settings
            .get(INSTALLED_KEY)
            .ok()
            .flatten()
            .map(|setting| setting.value);
        let Some(Value::Object(entries)) = stored else {
            return InstalledMap::new();
        };
        entries
            .into_iter()
            .filter_map(|(id, raw)| {
                let tag = raw.get("tag")?.as_str()?.to_owned();
                let install_path = raw.get("installPath")?.as_str()?.to_owned();
                Some((
                    id,
                    InstalledRuntime {
                        tag,
                        install_path,
                        executable_path: raw
                            .get("executablePath")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        size_bytes: raw
                            .get("sizeBytes")
                            .and_then(Value::as_f64)
                            .filter(|size| size.is_finite() && *size >= 0.0)
                            .map_or(0, |size| size as u64),
                    },
                ))
            })
            .collect()
    }

    fn save_installed(&self, installed: &InstalledMap) -> Result<(), AppError> {
        let value = serde_json::to_value(installed)
            .map_err(|error| AppError::new(AppErrorCode::Unknown, error.to_string()))?;
        self.inner.options.settings.set(INSTALLED_KEY, &value)?;
        Ok(())
    }

    fn remember(&self, runtime_id: &str, record: InstalledRuntime) -> Result<(), AppError> {
        let mut installed = self.installed();
        installed.insert(runtime_id.to_owned(), record);
        self.save_installed(&installed)
    }

    fn active_downloads(&self) -> Result<Vec<String>, AppError> {
        let mut seen = HashSet::new();
        Ok(self
            .inner
            .options
            .downloads
            .list(&DownloadListFilter {
                kinds: Some(vec![DownloadItemKind::Runtime]),
                statuses: None,
            })?
            .into_iter()
            .filter(|row| matches!(row.status, DownloadStatus::Queued | DownloadStatus::Running))
            .map(|row| row.item_ref)
            .filter(|item_ref| seen.insert(item_ref.clone()))
            .collect())
    }

    fn refs_of(active: &[String], runtime_id: &str) -> Vec<String> {
        active
            .iter()
            .filter(|item_ref| {
                parse_runtime_ref(item_ref).is_some_and(|parsed| parsed.runtime_id == runtime_id)
            })
            .cloned()
            .collect()
    }

    pub async fn overview(&self) -> Result<RuntimeOverviewDto, AppError> {
        let (plan, device) = self.refresh_recommendation().await;
        let installed = self.installed();
        let active = self.active_downloads()?;
        let runtimes = RUNTIME_DEFINITIONS
            .iter()
            .map(|definition| self.present(definition, plan.accelerator, &installed, &active))
            .collect();
        let overview = RuntimeOverviewDto {
            plan: RuntimePlanDto {
                accelerator: plan.accelerator,
                reason: plan.reason,
                gpu: device.and_then(|device| device.gpu),
                ready: !installed.is_empty(),
            },
            runtimes,
            recommended_models: RECOMMENDED_MODEL_REFS
                .iter()
                .map(|item_ref| (*item_ref).to_owned())
                .collect(),
        };
        overview.validate()?;
        Ok(overview)
    }

    fn present(
        &self,
        definition: &RuntimeDefinition,
        recommended: Accelerator,
        installed: &InstalledMap,
        active: &[String],
    ) -> RuntimeDto {
        let supported = definition.supported(self.platform());
        let record = installed.get(definition.id);
        let running = lock(&self.inner.servers)
            .get(&format!(
                "{}:{}",
                definition.id,
                LlamaServerRole::Embedding.as_str()
            ))
            .is_some_and(|server| server.running());
        let refs = Self::refs_of(active, definition.id);
        let state = if !supported {
            RuntimeState::Unsupported
        } else if !refs.is_empty() {
            RuntimeState::Installing
        } else if record.is_none_or(|record| record.executable_path.is_empty()) {
            RuntimeState::Available
        } else if running {
            RuntimeState::Ready
        } else {
            RuntimeState::Installed
        };
        RuntimeDto {
            id: definition.id.to_owned(),
            engine: definition.engine,
            accelerator: definition.accelerator,
            display_name: definition.display_name.to_owned(),
            description: definition.description.to_owned(),
            formats: definition.formats.to_vec(),
            state,
            installed_version: record.map(|record| record.tag.clone()),
            install_path: record.map(|record| record.install_path.clone()),
            executable_path: record
                .filter(|record| !record.executable_path.is_empty())
                .map(|record| record.executable_path.clone()),
            size_bytes: record.map_or(definition.approximate_bytes, |record| record.size_bytes),
            recommended: supported && definition.accelerator == recommended,
            blocked_reason: (!supported).then(|| UNSUPPORTED_PLATFORM.to_owned()),
            error: None,
            refs,
        }
    }

    pub fn catalogue_provider(&self) -> Arc<dyn CatalogueProvider> {
        Arc::new(RuntimeCatalogueProvider {
            service: self.clone(),
        })
    }

    fn catalogue_rows(&self) -> Result<Vec<CatalogueItem>, AppError> {
        let installed = self.installed();
        let active = self.active_downloads()?;
        let recommended = *lock(&self.inner.recommended);
        let platform = self.platform();
        Ok(RUNTIME_DEFINITIONS
            .iter()
            .filter(|definition| definition.supported(platform))
            .map(|definition| {
                let record = installed.get(definition.id);
                let installing = !Self::refs_of(&active, definition.id).is_empty();
                let ready = record.is_some_and(|record| !record.executable_path.is_empty());
                let mut tags = vec![
                    "llama.cpp".to_owned(),
                    definition.accelerator.as_str().to_owned(),
                ];
                tags.extend(
                    definition
                        .formats
                        .iter()
                        .map(|format| format.as_str().to_owned()),
                );
                if definition.accelerator == recommended {
                    tags.push("рекомендуется".to_owned());
                }
                CatalogueItem {
                    display_name: definition.display_name.to_owned(),
                    description: definition.description.to_owned(),
                    size_bytes: record
                        .map_or(definition.approximate_bytes, |record| record.size_bytes),
                    url: LLAMA_RELEASES_PAGE.to_owned(),
                    file_name: format!("{}.zip", definition.id),
                    tags,
                    state: if installing {
                        Some(CatalogueItemState::Downloading)
                    } else if ready {
                        Some(CatalogueItemState::Installed)
                    } else {
                        None
                    },
                    version: record.map(|record| record.tag.clone()),
                    ..CatalogueItem::new(
                        runtime_catalogue_ref(definition.id),
                        DownloadItemKind::Runtime,
                        CatalogueSource::Github,
                        definition.id,
                    )
                }
            })
            .collect())
    }

    pub async fn uninstall(&self, runtime_id: &str) -> Result<(), AppError> {
        if definition_of(runtime_id).is_none() {
            return Err(AppError::new(AppErrorCode::NotFound, NO_SUCH_ENGINE)
                .with_detail("runtimeId", runtime_id));
        }
        let mut installed = self.installed();
        let Some(record) = installed.remove(runtime_id) else {
            return Err(AppError::new(AppErrorCode::Conflict, NOT_INSTALLED));
        };
        self.stop(runtime_id);
        self.wait_stopped().await;
        let target = runtime_install_path(&self.inner.options.downloads_dir, runtime_id)
            .map_err(|error| path_failure(&error))?;
        if target.exists() {
            let removal = target.clone();
            tokio::task::spawn_blocking(move || std::fs::remove_dir_all(removal))
                .await
                .map_err(|error| AppError::new(AppErrorCode::NativeError, error.to_string()))?
                .map_err(|error| AppError::new(AppErrorCode::Unknown, error.to_string()))?;
        }
        self.save_installed(&installed)?;
        let id = runtime_id.to_owned();
        self.inner
            .options
            .downloads
            .forget_by_ref(&move |item_ref: &str| {
                parse_runtime_ref(item_ref).is_some_and(|parsed| parsed.runtime_id == id)
            })
            .await?;
        self.log(
            LogLevel::Info,
            "Removed an engine",
            &[("runtimeId", json!(runtime_id)), ("tag", json!(record.tag))],
        );
        Ok(())
    }

    async fn definition_for(
        &self,
        id: Option<&str>,
    ) -> Result<&'static RuntimeDefinition, AppError> {
        if let Some(id) = id {
            return definition_of(id).ok_or_else(|| {
                AppError::new(AppErrorCode::NotFound, NO_SUCH_ENGINE).with_detail("id", id)
            });
        }
        let (plan, _) = self.refresh_recommendation().await;
        let platform = self.platform();
        Ok(RUNTIME_DEFINITIONS
            .iter()
            .find(|definition| {
                definition.accelerator == plan.accelerator && definition.supported(platform)
            })
            .or_else(|| {
                RUNTIME_DEFINITIONS
                    .iter()
                    .find(|definition| definition.supported(platform))
            })
            .unwrap_or(&RUNTIME_DEFINITIONS[0]))
    }

    fn catalogue_items(
        definition: &RuntimeDefinition,
        build: &ResolvedBuild,
    ) -> Vec<CatalogueItem> {
        build
            .assets
            .iter()
            .map(|asset| CatalogueItem {
                display_name: format!("{} · {}", definition.display_name, build.tag),
                description: definition.description.to_owned(),
                version: Some(build.tag.clone()),
                size_bytes: asset.size_bytes,
                url: asset.url.clone(),
                file_name: asset.name.clone(),
                checksum: asset.checksum.clone(),
                digest: Some(build.tag.clone()),
                tags: vec![
                    "llama.cpp".to_owned(),
                    definition.accelerator.as_str().to_owned(),
                    build.tag.clone(),
                ],
                ..CatalogueItem::new(
                    runtime_ref(definition.id, &build.tag, &asset.name),
                    DownloadItemKind::Runtime,
                    CatalogueSource::Github,
                    format!("{}/{}", definition.id, asset.name),
                )
            })
            .collect()
    }

    pub async fn install(&self, input: &InstallRuntimeInput) -> Result<Vec<DownloadDto>, AppError> {
        input.validate()?;
        let definition = self.definition_for(input.id.as_deref()).await?;
        if lock(&self.inner.installing).contains(definition.id) {
            return Err(AppError::new(AppErrorCode::Conflict, ALREADY_INSTALLING));
        }
        let platform = self.platform();
        let Some(build) = self
            .inner
            .options
            .resolver
            .resolve(definition, true)
            .await?
        else {
            return Err(AppError::new(
                AppErrorCode::UnsupportedFormat,
                format!("Для этой системы нет сборки «{}»", definition.display_name),
            )
            .with_detail("runtimeId", definition.id)
            .with_detail("platform", platform.os)
            .with_detail("arch", platform.arch));
        };
        if !lock(&self.inner.installing).insert(definition.id.to_owned()) {
            return Err(AppError::new(AppErrorCode::Conflict, ALREADY_INSTALLING));
        }
        let outcome = self.queue_build(definition, &build).await;
        lock(&self.inner.installing).remove(definition.id);
        let started = outcome?;
        self.log(
            LogLevel::Info,
            "Queued an engine install",
            &[
                ("runtimeId", json!(definition.id)),
                ("tag", json!(build.tag)),
                ("archives", json!(started.len())),
            ],
        );
        Ok(started)
    }

    async fn queue_build(
        &self,
        definition: &RuntimeDefinition,
        build: &ResolvedBuild,
    ) -> Result<Vec<DownloadDto>, AppError> {
        let mut started = Vec::new();
        for item in Self::catalogue_items(definition, build) {
            let item_ref = item.item_ref.clone();
            self.inner.options.catalogue.offer(item);
            started.push(
                self.inner
                    .options
                    .downloads
                    .start(&StartDownloadInput::new(item_ref))
                    .await?,
            );
        }
        Ok(started)
    }

    pub async fn unpack_download(&self, row: &DownloadRow) -> Result<(), AppError> {
        if row.item_kind != DownloadItemKind::Runtime {
            return Ok(());
        }
        let Some(parsed) = parse_runtime_ref(&row.item_ref) else {
            return Ok(());
        };
        let Some(definition) = definition_of(&parsed.runtime_id) else {
            return Ok(());
        };
        let target = runtime_install_path(&self.inner.options.downloads_dir, definition.id)
            .map_err(|error| path_failure(&error))?;
        let report = extract_archive(
            Path::new(&row.target_path),
            &target,
            ExtractOptions::default(),
        )
        .await?;
        let _ = std::fs::remove_file(&row.target_path);
        let root = target.clone();
        let executable = tokio::task::spawn_blocking(move || find_executable(&root, 0))
            .await
            .ok()
            .flatten();
        let previous = self.installed().remove(definition.id);
        let record = InstalledRuntime {
            tag: parsed.tag.clone(),
            install_path: target.to_string_lossy().into_owned(),
            executable_path: executable
                .map(|path| path.to_string_lossy().into_owned())
                .or_else(|| {
                    previous
                        .as_ref()
                        .map(|previous| previous.executable_path.clone())
                })
                .unwrap_or_default(),
            size_bytes: previous
                .as_ref()
                .filter(|previous| previous.tag == parsed.tag)
                .map_or(0, |previous| previous.size_bytes)
                + report.bytes,
        };
        let executable =
            (!record.executable_path.is_empty()).then(|| record.executable_path.clone());
        self.remember(definition.id, record)?;
        self.log(
            LogLevel::Info,
            "Unpacked an engine archive",
            &[
                ("runtimeId", json!(definition.id)),
                ("tag", json!(parsed.tag)),
                ("files", json!(report.files)),
                ("executable", json!(executable)),
            ],
        );
        self.stop(definition.id);
        Ok(())
    }

    fn engine_for(
        &self,
        format: ModelFormat,
    ) -> Result<(&'static RuntimeDefinition, InstalledRuntime), AppError> {
        let installed = self.installed();
        let mut candidates: Vec<&'static RuntimeDefinition> = RUNTIME_DEFINITIONS
            .iter()
            .filter(|definition| {
                definition.formats.contains(&format)
                    && installed
                        .get(definition.id)
                        .is_some_and(|record| !record.executable_path.is_empty())
            })
            .collect();
        candidates.sort_by_key(|definition| std::cmp::Reverse(rank(definition.accelerator)));
        let Some(definition) = candidates.first() else {
            return Err(AppError::new(
                AppErrorCode::Conflict,
                if format == ModelFormat::Onnx {
                    NO_ONNX_ENGINE
                } else {
                    NO_ENGINE
                },
            )
            .with_detail("format", format.as_str()));
        };
        let record = installed
            .get(definition.id)
            .cloned()
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, NO_ENGINE))?;
        Ok((definition, record))
    }

    pub fn supports(&self, format: ModelFormat) -> bool {
        self.engine_for(format).is_ok()
    }

    fn model_path(&self, model_name: &str) -> Result<String, AppError> {
        let safe = file_name(model_name);
        for kind in [DownloadKind::Embedding, DownloadKind::Model] {
            let candidate = self
                .inner
                .options
                .downloads_dir
                .join(kind.folder())
                .join(&safe);
            if candidate.is_file() {
                return Ok(candidate.to_string_lossy().into_owned());
            }
        }
        Err(AppError::new(
            AppErrorCode::NotFound,
            format!("Файл модели «{safe}» не найден на диске"),
        )
        .with_detail("modelName", model_name))
    }

    fn projector_for(model_path: &str) -> Result<String, AppError> {
        let directory = Path::new(model_path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let projector = Regex::new(r"(?i)^mmproj.*\.gguf$").expect("a valid pattern");
        let mut projectors: Vec<String> = sorted_entries(&directory)
            .into_iter()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| projector.is_match(name))
            .collect();
        if projectors.is_empty() {
            return Err(AppError::new(AppErrorCode::NotFound, NO_PROJECTOR)
                .with_detail("directory", directory.to_string_lossy().into_owned()));
        }
        let lower = file_name(model_path).to_lowercase();
        let stem = lower.strip_suffix(".gguf").unwrap_or(&lower).to_owned();
        projectors.sort_by_key(|name| std::cmp::Reverse(overlap(name, &stem)));
        Ok(directory
            .join(&projectors[0])
            .to_string_lossy()
            .into_owned())
    }

    fn server_for(
        &self,
        definition: &RuntimeDefinition,
        record: &InstalledRuntime,
        model_path: &str,
        role: LlamaServerRole,
        mmproj_path: Option<String>,
    ) -> SharedEngineServer {
        let key = format!("{}:{}", definition.id, role.as_str());
        let mut servers = lock(&self.inner.servers);
        if let Some(existing) = servers.get(&key) {
            if existing.model_path() == model_path {
                return Arc::clone(existing);
            }
            existing.stop();
        }
        let options = &self.inner.options;
        let server_options = LlamaServerOptions {
            mmproj_path,
            context_size: options.context_size,
            ..LlamaServerOptions::new(
                PathBuf::from(&record.executable_path),
                model_path,
                role,
                if definition.accelerator == Accelerator::Cpu {
                    0
                } else {
                    999
                },
                options.http.clone(),
                options.logger.clone(),
            )
        };
        let server: SharedEngineServer = match &options.create_server {
            Some(create) => create(server_options),
            None => Arc::new(LlamaServer::new(server_options)),
        };
        servers.insert(key, Arc::clone(&server));
        server
    }

    pub async fn embed(
        &self,
        model_name: &str,
        texts: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        let format = format_of(model_name).ok_or_else(|| unsupported_format(model_name))?;
        let (definition, record) = self.engine_for(format)?;
        let model_path = self.model_path(model_name)?;
        let server = self.server_for(
            definition,
            &record,
            &model_path,
            LlamaServerRole::Embedding,
            None,
        );
        server.embed(texts, cancel).await
    }

    pub async fn read_image(
        &self,
        model_ref: &str,
        media_type: &str,
        bytes: &[u8],
        language: VectorOcrLanguage,
        cancel: &CancellationToken,
    ) -> Result<String, AppError> {
        let item = self.inner.options.catalogue.find(model_ref).await?;
        let (definition, record) = self.engine_for(ModelFormat::Gguf)?;
        let model_path = self.model_path(&item.file_name)?;
        let mmproj_path = Self::projector_for(&model_path)?;
        let server = self.server_for(
            definition,
            &record,
            &model_path,
            LlamaServerRole::Vision,
            Some(mmproj_path),
        );
        let text = server
            .describe_image(media_type, bytes, &ocr_prompt(language), cancel)
            .await?;
        Ok(text.trim().to_owned())
    }

    pub async fn rerank(
        &self,
        model_ref: &str,
        query: &str,
        passages: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<f64>, AppError> {
        let item = self.inner.options.catalogue.find(model_ref).await?;
        if format_of(&item.file_name) != Some(ModelFormat::Gguf) {
            return Err(unsupported_format(&item.file_name));
        }
        let (definition, record) = self.engine_for(ModelFormat::Gguf)?;
        let model_path = self.model_path(&item.file_name)?;
        let server = self.server_for(
            definition,
            &record,
            &model_path,
            LlamaServerRole::Rerank,
            None,
        );
        server.rerank(query, passages, cancel).await
    }

    pub fn stop(&self, runtime_id: &str) -> bool {
        let prefix = format!("{runtime_id}:");
        let mut servers = lock(&self.inner.servers);
        let keys: Vec<String> = servers
            .keys()
            .filter(|key| key.starts_with(&prefix))
            .cloned()
            .collect();
        let mut stopped = false;
        let mut stopping = lock(&self.inner.stopping);
        for key in keys {
            if let Some(server) = servers.remove(&key) {
                stopped = stopped || server.running();
                server.stop();
                stopping.push(server);
            }
        }
        stopped
    }

    async fn wait_stopped(&self) {
        let pending: Vec<SharedEngineServer> = std::mem::take(&mut *lock(&self.inner.stopping));
        for server in pending {
            server.stopped().await;
        }
    }

    pub async fn dispose(&self) {
        let servers: Vec<SharedEngineServer> = std::mem::take(&mut *lock(&self.inner.servers))
            .into_values()
            .collect();
        for server in &servers {
            server.stop();
        }
        for server in servers {
            server.stopped().await;
        }
        self.wait_stopped().await;
    }
}

struct RuntimeCatalogueProvider {
    service: RuntimeService,
}

#[async_trait]
impl CatalogueProvider for RuntimeCatalogueProvider {
    fn id(&self) -> CatalogueSource {
        CatalogueSource::Github
    }

    fn live(&self) -> bool {
        false
    }

    async fn list(&self) -> Result<Vec<CatalogueItem>, AppError> {
        self.service.refresh_recommendation().await;
        self.service.catalogue_rows()
    }

    async fn installed(&self) -> Result<Vec<InstalledItem>, AppError> {
        Ok(self
            .service
            .installed()
            .into_iter()
            .map(|(id, record)| InstalledItem {
                kind: DownloadItemKind::Runtime,
                name: id,
                version: Some(record.tag),
                size_bytes: record.size_bytes,
                digest: None,
            })
            .collect())
    }
}

#[async_trait]
impl DownloadInstaller for RuntimeService {
    async fn install(&self, row: &DownloadRow) -> Result<(), AppError> {
        self.unpack_download(row).await
    }
}

#[async_trait]
impl LocalEmbeddingEngine for RuntimeService {
    async fn embed(
        &self,
        model_name: &str,
        texts: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        Self::embed(self, model_name, texts, cancel).await
    }
}

#[async_trait]
impl LocalRerankEngine for RuntimeService {
    async fn rerank(
        &self,
        model_ref: &str,
        query: &str,
        passages: &[String],
        cancel: &CancellationToken,
    ) -> Result<Vec<f64>, AppError> {
        Self::rerank(self, model_ref, query, passages, cancel).await
    }
}

#[cfg(test)]
mod tests;
