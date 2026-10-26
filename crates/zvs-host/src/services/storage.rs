use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use zvs_contract::{
    AppError, AppErrorCode, StorageCategory, StorageCategoryUsage, StorageClearCacheOutput,
    StoragePart, StoragePartUsage, StorageUsageDto,
};

use crate::artifacts::DASHBOARDS;
use crate::browser::PROFILE_DIR;
use crate::data::{Database, Repositories};
use crate::downloads::disk::directory_size;
use crate::drivers::mcp::TOOLCHAIN_DIR;
use crate::platform::clock::SharedClock;
use crate::platform::paths::{DownloadKind, StudioPaths};
use crate::skills::SKILLS_DIR;

pub const CACHE_BUSY: &str = "Кэш нельзя очистить, пока выполняются задачи";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageRoot {
    pub category: StorageCategory,
    pub paths: Vec<PathBuf>,
    pub parts: Vec<(StoragePart, Vec<PathBuf>)>,
}

impl StorageRoot {
    fn new(category: StorageCategory, paths: Vec<PathBuf>) -> Self {
        Self {
            category,
            paths,
            parts: Vec::new(),
        }
    }

    fn with_parts(mut self, parts: Vec<(StoragePart, Vec<PathBuf>)>) -> Self {
        self.parts = parts;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageRoots {
    pub categories: Vec<StorageRoot>,
    pub cache_dir: PathBuf,
}

const fn download_part(kind: DownloadKind) -> StoragePart {
    match kind {
        DownloadKind::Model => StoragePart::Models,
        DownloadKind::Embedding => StoragePart::Embeddings,
        DownloadKind::Runtime => StoragePart::Runtimes,
        DownloadKind::Mcp => StoragePart::Mcp,
        DownloadKind::Skill => StoragePart::Skills,
    }
}

fn toolchain_parts(root: &Path) -> Vec<(StoragePart, Vec<PathBuf>)> {
    let uv = root.join("uv");
    vec![
        (StoragePart::Node, vec![root.join("node")]),
        (
            StoragePart::Npm,
            vec![root.join("npm-cache"), root.join("npm-global")],
        ),
        (StoragePart::Uv, vec![uv.clone()]),
        (StoragePart::Python, vec![uv.join("python")]),
        (
            StoragePart::PythonPackages,
            vec![
                uv.join("tools"),
                uv.join("bin"),
                uv.join("cache"),
                root.join("pip-cache"),
            ],
        ),
    ]
}

impl StorageRoots {
    pub fn resolve(paths: &StudioPaths) -> Self {
        let data = &paths.data_dir;
        let database = paths.db_path.to_string_lossy().into_owned();
        let toolchain = data.join(TOOLCHAIN_DIR);
        Self {
            categories: vec![
                StorageRoot::new(
                    StorageCategory::Conversations,
                    vec![paths.conversations_dir.clone()],
                ),
                StorageRoot::new(StorageCategory::Dashboards, vec![data.join(DASHBOARDS)]),
                StorageRoot::new(StorageCategory::Skills, vec![data.join(SKILLS_DIR)]),
                StorageRoot::new(
                    StorageCategory::Vectors,
                    vec![paths.vector_stores_dir.clone()],
                ),
                StorageRoot::new(
                    StorageCategory::Downloads,
                    vec![paths.downloads_dir.clone()],
                )
                .with_parts(
                    DownloadKind::ALL
                        .iter()
                        .map(|kind| (download_part(*kind), vec![paths.download_dir(*kind)]))
                        .collect(),
                ),
                StorageRoot::new(StorageCategory::Toolchain, vec![toolchain.clone()])
                    .with_parts(toolchain_parts(&toolchain)),
                StorageRoot::new(StorageCategory::Browser, vec![data.join(PROFILE_DIR[0])]),
                StorageRoot::new(StorageCategory::Cache, vec![paths.cache_dir.clone()]),
                StorageRoot::new(StorageCategory::Logs, vec![paths.logs_dir.clone()]),
                StorageRoot::new(StorageCategory::Backups, vec![paths.backups_dir.clone()]),
                StorageRoot::new(
                    StorageCategory::Database,
                    ["", "-wal", "-shm"]
                        .iter()
                        .map(|suffix| PathBuf::from(format!("{database}{suffix}")))
                        .collect(),
                ),
            ],
            cache_dir: paths.cache_dir.clone(),
        }
    }
}

fn path_size(path: &Path) -> u64 {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => directory_size(path),
        Ok(metadata) if metadata.is_file() => metadata.len(),
        _ => 0,
    }
}

fn measure_parts(root: &StorageRoot, bytes: u64) -> Vec<StoragePartUsage> {
    let nested: Vec<&PathBuf> = root.parts.iter().flat_map(|(_, paths)| paths).collect();
    let own_size = |path: &PathBuf| {
        let inner: u64 = nested
            .iter()
            .filter(|other| **other != path && other.starts_with(path))
            .map(|other| path_size(other))
            .sum();
        path_size(path).saturating_sub(inner)
    };
    let mut parts: Vec<StoragePartUsage> = root
        .parts
        .iter()
        .map(|(part, paths)| StoragePartUsage {
            part: *part,
            bytes: paths.iter().map(own_size).sum(),
        })
        .collect();
    let accounted: u64 = parts.iter().map(|usage| usage.bytes).sum();
    parts.push(StoragePartUsage {
        part: StoragePart::Other,
        bytes: bytes.saturating_sub(accounted),
    });
    parts.retain(|usage| usage.bytes > 0);
    parts
}

fn measure(roots: &StorageRoots, measured_at: i64) -> StorageUsageDto {
    let categories: Vec<StorageCategoryUsage> = roots
        .categories
        .iter()
        .map(|root| {
            let bytes = root.paths.iter().map(|path| path_size(path)).sum();
            let parts = if root.parts.is_empty() {
                Vec::new()
            } else {
                measure_parts(root, bytes)
            };
            StorageCategoryUsage {
                category: root.category,
                bytes,
                parts,
            }
        })
        .collect();
    StorageUsageDto {
        total_bytes: categories.iter().map(|usage| usage.bytes).sum(),
        categories,
        measured_at,
    }
}

fn clear_directory(directory: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let size = path_size(&path);
        let cleared = match entry.file_type() {
            Ok(kind) if kind.is_dir() => std::fs::remove_dir_all(&path),
            _ => std::fs::remove_file(&path),
        };
        if cleared.is_ok() {
            removed += size;
        }
    }
    removed
}

fn native(error: impl std::fmt::Display) -> AppError {
    AppError::new(AppErrorCode::NativeError, error.to_string())
}

struct Inner {
    roots: StorageRoots,
    database: Arc<Database>,
    clock: SharedClock,
    cached: Mutex<Option<StorageUsageDto>>,
}

#[derive(Clone)]
pub struct StorageService {
    inner: Arc<Inner>,
}

impl StorageService {
    pub fn new(roots: StorageRoots, database: Arc<Database>, clock: SharedClock) -> Self {
        Self {
            inner: Arc::new(Inner {
                roots,
                database,
                clock,
                cached: Mutex::new(None),
            }),
        }
    }

    fn cached(&self) -> std::sync::MutexGuard<'_, Option<StorageUsageDto>> {
        self.inner
            .cached
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub async fn usage(&self, refresh: bool) -> Result<StorageUsageDto, AppError> {
        if !refresh && let Some(usage) = self.cached().clone() {
            return Ok(usage);
        }
        let inner = Arc::clone(&self.inner);
        let usage = tokio::task::spawn_blocking(move || measure(&inner.roots, inner.clock.now()))
            .await
            .map_err(native)?;
        *self.cached() = Some(usage.clone());
        Ok(usage)
    }

    pub async fn clear_cache(&self) -> Result<StorageClearCacheOutput, AppError> {
        let busy = self.inner.database.with_connection(|connection| {
            Ok(!Repositories::new(connection).runs.unfinished()?.is_empty())
        })?;
        if busy {
            return Err(AppError::new(AppErrorCode::Conflict, CACHE_BUSY));
        }
        let directory = self.inner.roots.cache_dir.clone();
        let removed_bytes = tokio::task::spawn_blocking(move || clear_directory(&directory))
            .await
            .map_err(native)?;
        *self.cached() = None;
        Ok(StorageClearCacheOutput { removed_bytes })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::Value;
    use zvs_contract::{GraphNode, RunGraph, RunKind, RunStatus, Validate};

    use super::*;
    use crate::data::repositories::RunDraft;
    use crate::platform::clock::FakeClock;
    use crate::platform::paths::PathEnvironment;
    use crate::testing::{TemporaryDatabase, temporary_database};

    struct Fixture {
        _root: tempfile::TempDir,
        paths: StudioPaths,
        database: TemporaryDatabase,
        clock: Arc<FakeClock>,
        service: StorageService,
    }

    fn fixture() -> Fixture {
        let root = tempfile::tempdir().expect("a data folder");
        let paths = StudioPaths::resolve(&PathEnvironment {
            data_dir: root.path().join("data"),
            resource_dir: root.path().join("resources"),
            executable_dir: root.path().join("bin"),
            packaged: false,
        });
        let database = temporary_database();
        let clock = Arc::new(FakeClock::new(1_760_000_000_000));
        let service = StorageService::new(
            StorageRoots::resolve(&paths),
            database.shared(),
            clock.clone(),
        );
        Fixture {
            _root: root,
            paths,
            database,
            clock,
            service,
        }
    }

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().expect("a parent")).expect("creates the folder");
        fs::write(path, vec![b'x'; bytes]).expect("writes");
    }

    fn bytes(usage: &StorageUsageDto, category: StorageCategory) -> u64 {
        usage
            .categories
            .iter()
            .find(|item| item.category == category)
            .map_or(0, |item| item.bytes)
    }

    #[tokio::test]
    async fn usage_sums_every_category_and_is_cached_until_a_refresh() {
        let test = fixture();
        let data = &test.paths.data_dir;
        write(&test.paths.conversations_dir.join("c1/work/report.md"), 100);
        write(&test.paths.conversations_dir.join("c1/snapshots/r1/a"), 20);
        write(&data.join("dashboards/sales.html"), 7);
        write(&data.join("skills/dashboard/SKILL.md"), 5);
        write(&test.paths.vector_stores_dir.join("s1/data"), 11);
        write(&test.paths.downloads_dir.join("models/m.gguf"), 13);
        write(&data.join("mcp-toolchain/node/node.exe"), 17);
        write(&data.join("browser/work/Cookies"), 19);
        write(&test.paths.cache_dir.join("code-runs/r1/out.txt"), 23);
        write(&test.paths.logs_dir.join("studio.jsonl"), 29);
        write(&test.paths.backups_dir.join("db-1.sqlite"), 31);
        write(&test.paths.db_path, 37);
        write(
            &PathBuf::from(format!("{}-wal", test.paths.db_path.display())),
            3,
        );

        let usage = test.service.usage(false).await.expect("measures");

        usage.validate().expect("a valid usage");
        assert_eq!(usage.categories.len(), StorageCategory::ALL.len());
        assert_eq!(bytes(&usage, StorageCategory::Conversations), 120);
        assert_eq!(bytes(&usage, StorageCategory::Dashboards), 7);
        assert_eq!(bytes(&usage, StorageCategory::Skills), 5);
        assert_eq!(bytes(&usage, StorageCategory::Vectors), 11);
        assert_eq!(bytes(&usage, StorageCategory::Downloads), 13);
        assert_eq!(bytes(&usage, StorageCategory::Toolchain), 17);
        assert_eq!(bytes(&usage, StorageCategory::Browser), 19);
        assert_eq!(bytes(&usage, StorageCategory::Cache), 23);
        assert_eq!(bytes(&usage, StorageCategory::Logs), 29);
        assert_eq!(bytes(&usage, StorageCategory::Backups), 31);
        assert_eq!(bytes(&usage, StorageCategory::Database), 40);
        assert_eq!(usage.total_bytes, 315);
        assert_eq!(usage.measured_at, 1_760_000_000_000);
        assert_eq!(
            parts(&usage, StorageCategory::Downloads),
            vec![(StoragePart::Models, 13)]
        );
        assert_eq!(
            parts(&usage, StorageCategory::Toolchain),
            vec![(StoragePart::Node, 17)]
        );
        assert!(parts(&usage, StorageCategory::Cache).is_empty());

        write(&data.join("dashboards/more.html"), 1000);
        test.clock.advance(5);
        assert_eq!(test.service.usage(false).await.expect("cached"), usage);
        let refreshed = test.service.usage(true).await.expect("measures");
        assert_eq!(bytes(&refreshed, StorageCategory::Dashboards), 1007);
        assert_eq!(refreshed.measured_at, 1_760_000_000_005);
    }

    fn parts(usage: &StorageUsageDto, category: StorageCategory) -> Vec<(StoragePart, u64)> {
        usage
            .categories
            .iter()
            .find(|item| item.category == category)
            .map(|item| {
                item.parts
                    .iter()
                    .map(|part| (part.part, part.bytes))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn downloads_and_components_are_split_into_parts_that_add_up() {
        let test = fixture();
        let downloads = &test.paths.downloads_dir;
        let toolchain = test.paths.data_dir.join(TOOLCHAIN_DIR);
        write(&downloads.join("models/m.gguf"), 13);
        write(&downloads.join("embeddings/e.gguf"), 5);
        write(&downloads.join("runtimes/node.zip"), 7);
        write(&downloads.join("stray.part"), 2);
        write(&toolchain.join("node/22.0.0/node.exe"), 40);
        write(&toolchain.join("npm-cache/a"), 3);
        write(&toolchain.join("npm-global/b"), 4);
        write(&toolchain.join("uv/0.8.0/uv.exe"), 9);
        write(&toolchain.join("uv/python/3.13/python.exe"), 30);
        write(&toolchain.join("uv/tools/ruff/r"), 6);
        write(&toolchain.join("uv/cache/c"), 1);
        write(&toolchain.join("pip-cache/p"), 2);
        write(&toolchain.join("state.json"), 1);

        let usage = test.service.usage(false).await.expect("measures");

        usage.validate().expect("parts add up to their category");
        assert_eq!(
            parts(&usage, StorageCategory::Downloads),
            vec![
                (StoragePart::Models, 13),
                (StoragePart::Embeddings, 5),
                (StoragePart::Runtimes, 7),
                (StoragePart::Other, 2),
            ]
        );
        assert_eq!(
            parts(&usage, StorageCategory::Toolchain),
            vec![
                (StoragePart::Node, 40),
                (StoragePart::Npm, 7),
                (StoragePart::Uv, 9),
                (StoragePart::Python, 30),
                (StoragePart::PythonPackages, 9),
                (StoragePart::Other, 1),
            ]
        );
    }

    #[tokio::test]
    async fn clearing_the_cache_empties_only_the_cache_and_drops_the_cached_usage() {
        let test = fixture();
        write(&test.paths.cache_dir.join("code-runs/r1/out.txt"), 23);
        write(&test.paths.cache_dir.join("documents/d.bin"), 7);
        write(&test.paths.logs_dir.join("studio.jsonl"), 29);
        let before = test.service.usage(false).await.expect("measures");
        assert_eq!(bytes(&before, StorageCategory::Cache), 30);

        let cleared = test.service.clear_cache().await.expect("clears");

        assert_eq!(cleared.removed_bytes, 30);
        assert!(test.paths.cache_dir.is_dir());
        assert_eq!(
            fs::read_dir(&test.paths.cache_dir).expect("lists").count(),
            0
        );
        let after = test.service.usage(false).await.expect("measures");
        assert_eq!(bytes(&after, StorageCategory::Cache), 0);
        assert_eq!(bytes(&after, StorageCategory::Logs), 29);
    }

    #[tokio::test]
    async fn the_cache_is_kept_while_a_run_is_unfinished() {
        let test = fixture();
        write(&test.paths.cache_dir.join("code-runs/r1/out.txt"), 23);
        test.database
            .shared()
            .with_connection(|connection| {
                Repositories::new(connection).runs.create(&RunDraft {
                    id: "r1".to_owned(),
                    kind: RunKind::Job,
                    subject_id: None,
                    title: None,
                    retry_of_id: None,
                    status: RunStatus::Running,
                    graph: RunGraph::new(vec![GraphNode::new("a", "work").normalized()]),
                    input: Value::Null,
                    stream_id: "s".to_owned(),
                    concurrency: 1,
                    created_at: 1,
                    started_at: None,
                })?;
                Ok(())
            })
            .expect("creates a run");

        let error = test.service.clear_cache().await.expect_err("refuses");

        assert_eq!(error.code, AppErrorCode::Conflict);
        assert_eq!(error.message, CACHE_BUSY);
        assert!(test.paths.cache_dir.join("code-runs/r1/out.txt").is_file());
    }
}
