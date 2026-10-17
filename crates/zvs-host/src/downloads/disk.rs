use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use zvs_contract::{
    AppError, AppErrorCode, DiskCategory, DiskCategoryUsage, DiskUsageDto, DownloadItemKind,
};

use crate::platform::clock::SharedClock;
use crate::platform::disk::{FreeSpace, disk_space};
use crate::platform::paths::DownloadKind;

pub const DEFAULT_DISK_TTL: Duration = Duration::from_secs(30);

#[async_trait]
pub trait DiskProbe: Send + Sync {
    async fn free(&self, path: &Path) -> Result<FreeSpace, AppError>;

    async fn size(&self, path: &Path) -> u64;
}

pub type SharedDiskProbe = Arc<dyn DiskProbe>;

pub struct SystemDiskProbe;

#[async_trait]
impl DiskProbe for SystemDiskProbe {
    async fn free(&self, path: &Path) -> Result<FreeSpace, AppError> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || disk_space(&path))
            .await
            .map_err(|error| AppError::new(AppErrorCode::NativeError, error.to_string()))?
            .map_err(|message| AppError::new(AppErrorCode::NotFound, message))
    }

    async fn size(&self, path: &Path) -> u64 {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || directory_size(&path))
            .await
            .unwrap_or(0)
    }
}

pub fn directory_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0;
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            total += directory_size(&entry.path());
        } else if kind.is_file() {
            total += entry.metadata().map_or(0, |metadata| metadata.len());
        }
    }
    total
}

pub const fn category_of(kind: DownloadItemKind) -> DiskCategory {
    match kind {
        DownloadItemKind::Model => DiskCategory::Models,
        DownloadItemKind::Embedding => DiskCategory::Embeddings,
        DownloadItemKind::Runtime => DiskCategory::Runtimes,
        DownloadItemKind::Mcp => DiskCategory::Mcp,
        DownloadItemKind::Skill => DiskCategory::Skills,
    }
}

pub const fn download_kind(kind: DownloadItemKind) -> DownloadKind {
    match kind {
        DownloadItemKind::Model => DownloadKind::Model,
        DownloadItemKind::Embedding => DownloadKind::Embedding,
        DownloadItemKind::Runtime => DownloadKind::Runtime,
        DownloadItemKind::Mcp => DownloadKind::Mcp,
        DownloadItemKind::Skill => DownloadKind::Skill,
    }
}

#[derive(Clone)]
pub struct DiskServiceOptions {
    pub downloads_dir: PathBuf,
    pub vector_stores_dir: PathBuf,
    pub user_data_dir: PathBuf,
    pub probe: SharedDiskProbe,
    pub ttl: Duration,
    pub clock: SharedClock,
}

impl DiskServiceOptions {
    pub fn new(
        downloads_dir: PathBuf,
        vector_stores_dir: PathBuf,
        user_data_dir: PathBuf,
        clock: SharedClock,
    ) -> Self {
        Self {
            downloads_dir,
            vector_stores_dir,
            user_data_dir,
            probe: Arc::new(SystemDiskProbe),
            ttl: DEFAULT_DISK_TTL,
            clock,
        }
    }
}

#[derive(Default)]
struct Cache {
    usage: Option<DiskUsageDto>,
    generation: u64,
}

struct Inner {
    options: DiskServiceOptions,
    cache: Mutex<Cache>,
    measuring: tokio::sync::Mutex<()>,
}

#[derive(Clone)]
pub struct DiskService {
    inner: Arc<Inner>,
}

impl DiskService {
    pub fn new(options: DiskServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                cache: Mutex::new(Cache::default()),
                measuring: tokio::sync::Mutex::new(()),
            }),
        }
    }

    pub fn downloads_dir(&self) -> &Path {
        &self.inner.options.downloads_dir
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, Cache> {
        self.inner
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn fresh(&self) -> Option<DiskUsageDto> {
        let ttl = i64::try_from(self.inner.options.ttl.as_millis()).unwrap_or(i64::MAX);
        let now = self.inner.options.clock.now();
        self.cache()
            .usage
            .clone()
            .filter(|usage| now - usage.measured_at < ttl)
    }

    pub async fn usage(&self, refresh: bool) -> DiskUsageDto {
        if !refresh && let Some(usage) = self.fresh() {
            return usage;
        }
        let seen = self.cache().generation;
        let _measuring = self.inner.measuring.lock().await;
        {
            let cache = self.cache();
            if cache.generation != seen
                && let Some(usage) = &cache.usage
            {
                return usage.clone();
            }
        }
        let usage = self.measure().await;
        let mut cache = self.cache();
        cache.usage = Some(usage.clone());
        cache.generation += 1;
        usage
    }

    pub async fn free_bytes(&self) -> Result<u64, AppError> {
        Ok(self
            .inner
            .options
            .probe
            .free(&self.inner.options.downloads_dir)
            .await?
            .free_bytes)
    }

    async fn measure(&self) -> DiskUsageDto {
        let options = &self.inner.options;
        let probe = &options.probe;
        let space = match probe.free(&options.downloads_dir).await {
            Ok(space) => space,
            Err(_) => probe.free(&options.user_data_dir).await.unwrap_or_default(),
        };
        let mut categories = Vec::new();
        let mut accounted = 0_u64;
        for kind in DownloadItemKind::ALL {
            let bytes = probe
                .size(&options.downloads_dir.join(download_kind(*kind).folder()))
                .await;
            accounted += bytes;
            categories.push(DiskCategoryUsage {
                category: category_of(*kind),
                bytes,
            });
        }
        let vectors = probe.size(&options.vector_stores_dir).await;
        accounted += vectors;
        categories.push(DiskCategoryUsage {
            category: DiskCategory::Vectors,
            bytes: vectors,
        });
        let everything = probe.size(&options.user_data_dir).await;
        categories.push(DiskCategoryUsage {
            category: DiskCategory::Other,
            bytes: everything.saturating_sub(accounted),
        });
        DiskUsageDto {
            root: options.downloads_dir.to_string_lossy().into_owned(),
            total_bytes: space.total_bytes,
            free_bytes: space.free_bytes,
            used_bytes: space.total_bytes.saturating_sub(space.free_bytes),
            categories,
            measured_at: options.clock.now(),
        }
    }
}
