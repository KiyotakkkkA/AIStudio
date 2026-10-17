use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use zvs_contract::{
    AppError, AppErrorCode, CatalogueFilter, CatalogueItemState, CatalogueSource,
    ChecksumAlgorithm, ChecksumDto, DiskCategory, DownloadDto, DownloadItemKind,
    DownloadListFilter, DownloadStatus, RunKind, RunListFilter, StartDownloadInput,
};

use super::service::FREE_SPACE_MARGIN_BYTES;
use super::*;
use crate::data::Repositories;
use crate::data::schema::DownloadRow;
use crate::kernel::{KernelServices, NodeRegistry, register_download_nodes};
use crate::platform::clock::{Clock, FakeClock, SharedClock};
use crate::platform::disk::FreeSpace;
use crate::platform::events::EventBus;
use crate::platform::logger::silent_logger;
use crate::platform::paths::download_target_path;
use crate::services::{RunService, RunServiceOptions};
use crate::testing::{
    FakeDownloadJobs, TemporaryDatabase, TemporaryDirectory, sha256_hex, temporary_database,
    temporary_directory,
};

const GIB: u64 = 1024 * 1024 * 1024;
const BODY: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

struct FakeProbe {
    free: AtomicU64,
}

#[async_trait]
impl DiskProbe for FakeProbe {
    async fn free(&self, _path: &Path) -> Result<FreeSpace, AppError> {
        Ok(FreeSpace {
            total_bytes: 1000 * GIB,
            free_bytes: self.free.load(Ordering::SeqCst),
        })
    }

    async fn size(&self, path: &Path) -> u64 {
        super::disk::directory_size(path)
    }
}

#[derive(Default)]
struct FakeProvider {
    items: Mutex<Vec<CatalogueItem>>,
    present: Mutex<Vec<InstalledItem>>,
}

#[async_trait]
impl CatalogueProvider for FakeProvider {
    fn id(&self) -> CatalogueSource {
        CatalogueSource::Curated
    }

    fn live(&self) -> bool {
        false
    }

    async fn list(&self) -> Result<Vec<CatalogueItem>, AppError> {
        Ok(self
            .items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone())
    }

    async fn installed(&self) -> Result<Vec<InstalledItem>, AppError> {
        Ok(self
            .present
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone())
    }
}

fn item(item_ref: &str, name: &str) -> CatalogueItem {
    CatalogueItem {
        size_bytes: BODY.len() as u64,
        url: format!("https://example.test/{name}"),
        file_name: format!("{name}.bin"),
        ..CatalogueItem::new(
            item_ref,
            DownloadItemKind::Model,
            CatalogueSource::Curated,
            name,
        )
    }
}

struct Fixture {
    downloads: DownloadService,
    runs: RunService,
    jobs: Arc<FakeDownloadJobs>,
    provider: Arc<FakeProvider>,
    probe: Arc<FakeProbe>,
    clock: Arc<FakeClock>,
    downloads_dir: PathBuf,
    vectors_dir: PathBuf,
    workspace: TemporaryDirectory,
    database: TemporaryDatabase,
}

impl Fixture {
    fn new(concurrency: usize) -> Self {
        let database = temporary_database();
        let workspace = temporary_directory("studio-downloads-");
        let downloads_dir = workspace.join("downloads");
        let vectors_dir = workspace.join("vectors");
        std::fs::create_dir_all(&vectors_dir).expect("creates the vectors folder");
        let clock = Arc::new(FakeClock::default());
        let shared_clock: SharedClock = clock.clone();
        let jobs = FakeDownloadJobs::shared();
        let probe = Arc::new(FakeProbe {
            free: AtomicU64::new(100 * GIB),
        });
        let provider = Arc::new(FakeProvider::default());
        provider
            .items
            .lock()
            .expect("not poisoned")
            .push(item("curated:model:alpha", "alpha"));
        let disk = DiskService::new(DiskServiceOptions {
            probe: probe.clone(),
            ttl: Duration::ZERO,
            ..DiskServiceOptions::new(
                downloads_dir.clone(),
                vectors_dir.clone(),
                workspace.path().to_path_buf(),
                shared_clock.clone(),
            )
        });
        let catalogue = CatalogueService::new(CatalogueServiceOptions {
            providers: vec![provider.clone()],
            ttl: Duration::ZERO,
            ..CatalogueServiceOptions::new(shared_clock.clone(), silent_logger())
        });
        let downloads = DownloadService::new(DownloadServiceOptions {
            concurrency: Some(concurrency),
            persist_interval: Duration::ZERO,
            ..DownloadServiceOptions::new(
                database.shared(),
                jobs.clone(),
                disk,
                catalogue,
                downloads_dir.clone(),
                silent_logger(),
                shared_clock.clone(),
            )
        });
        let nodes = Arc::new(NodeRegistry::new());
        register_download_nodes(&nodes).expect("registers");
        let runs = RunService::new(RunServiceOptions {
            services: KernelServices {
                downloads: Some(Arc::new(downloads.clone())),
                ..KernelServices::default()
            },
            clock: shared_clock,
            ..RunServiceOptions::new(database.shared(), EventBus::default(), nodes)
        })
        .expect("a run service");
        downloads.attach(Arc::new(runs.clone()));
        Self {
            downloads,
            runs,
            jobs,
            provider,
            probe,
            clock,
            downloads_dir,
            vectors_dir,
            workspace,
            database,
        }
    }

    fn set_items(&self, items: Vec<CatalogueItem>) {
        *self.provider.items.lock().expect("not poisoned") = items;
    }

    fn push_item(&self, entry: CatalogueItem) {
        self.provider
            .items
            .lock()
            .expect("not poisoned")
            .push(entry);
    }

    fn set_free(&self, bytes: u64) {
        self.probe.free.store(bytes, Ordering::SeqCst);
    }

    fn target_of(&self, entry: &CatalogueItem) -> PathBuf {
        download_target_path(&self.downloads_dir, entry.kind.as_str(), &entry.file_name)
            .expect("a safe target")
    }

    fn alpha(&self) -> CatalogueItem {
        self.provider.items.lock().expect("not poisoned")[0].clone()
    }

    async fn start(&self, item_ref: &str) -> Result<DownloadDto, AppError> {
        self.downloads
            .start(&StartDownloadInput::new(item_ref))
            .await
    }

    async fn settle(&self, id: &str) -> DownloadDto {
        for _ in 0..600 {
            let row = self.downloads.get(id).expect("the download");
            if !matches!(row.status, DownloadStatus::Queued | DownloadStatus::Running) {
                return row;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!(
            "download never settled: {:?}",
            self.downloads.get(id).map(|row| row.status)
        );
    }

    async fn dispose(self) {
        self.downloads.dispose();
        self.runs.dispose().await;
    }
}

fn part(target: &Path) -> PathBuf {
    PathBuf::from(format!("{}.part", target.display()))
}

fn id(download: &DownloadDto) -> String {
    download.id.to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_lands_at_its_target_and_shows_up_as_a_run_on_the_tasks_page() {
    let fixture = Fixture::new(2);
    fixture.jobs.serve("https://example.test/alpha", BODY, None);

    let started = fixture.start("curated:model:alpha").await.expect("starts");
    assert_eq!(started.status, DownloadStatus::Running);

    let finished = fixture.settle(&id(&started)).await;
    assert_eq!(finished.status, DownloadStatus::Succeeded);
    assert_eq!(finished.bytes_done, BODY.len() as u64);
    let target = fixture.target_of(&fixture.alpha());
    assert_eq!(std::fs::read(&target).expect("landed"), BODY);
    assert!(!part(&target).exists());

    let page = fixture
        .runs
        .list(&RunListFilter {
            live: Some(false),
            limit: Some(50),
            ..RunListFilter::default()
        })
        .expect("lists");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].kind, RunKind::Download);
    assert_eq!(
        page.items[0].subject_id.as_deref(),
        Some(id(&started).as_str())
    );
    assert_eq!(page.counts.by_kind.get(&RunKind::Download), Some(&1));
    fixture.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_partial_file_resumes_from_where_it_stopped_instead_of_restarting() {
    let fixture = Fixture::new(2);
    fixture.jobs.serve("https://example.test/alpha", BODY, None);
    let target = fixture.target_of(&fixture.alpha());
    std::fs::create_dir_all(target.parent().expect("a folder")).expect("creates");
    std::fs::write(part(&target), &BODY[..12]).expect("writes");

    let started = fixture.start("curated:model:alpha").await.expect("starts");
    assert_eq!(started.bytes_done, 12);
    assert_eq!(
        fixture.settle(&id(&started)).await.status,
        DownloadStatus::Succeeded
    );

    let attempts = fixture.jobs.attempts();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].resumed_from, 12);
    assert_eq!(std::fs::read(&target).expect("landed"), BODY);
    fixture.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_checksum_mismatch_fails_the_download_and_installs_nothing() {
    let fixture = Fixture::new(2);
    fixture.set_items(vec![CatalogueItem {
        checksum: Some(ChecksumDto {
            algorithm: ChecksumAlgorithm::Sha256,
            value: sha256_hex(b"something else"),
        }),
        ..item("curated:model:alpha", "alpha")
    }]);
    fixture.jobs.serve("https://example.test/alpha", BODY, None);

    let started = fixture.start("curated:model:alpha").await.expect("starts");
    let failed = fixture.settle(&id(&started)).await;

    assert_eq!(failed.status, DownloadStatus::Failed);
    assert!(
        failed
            .error
            .as_deref()
            .is_some_and(|error| error.contains("Checksum mismatch"))
    );
    let target = fixture.target_of(&fixture.alpha());
    assert!(!target.exists());
    assert!(!part(&target).exists());
    fixture.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_deletes_the_partial_file_and_pausing_keeps_it() {
    let fixture = Fixture::new(2);
    fixture
        .jobs
        .serve("https://example.test/alpha", BODY, Some(6));
    fixture.jobs.hold("https://example.test/alpha");
    let started = fixture.start("curated:model:alpha").await.expect("starts");
    fixture.jobs.started(1).await;

    let paused = fixture
        .downloads
        .pause(&id(&started))
        .await
        .expect("pauses");
    fixture.jobs.release("https://example.test/alpha");
    assert_eq!(paused.status, DownloadStatus::Paused);
    assert!(part(Path::new(&started.target_path)).exists());

    fixture.downloads.resume(&id(&started)).expect("resumes");
    assert_eq!(
        fixture.settle(&id(&started)).await.status,
        DownloadStatus::Succeeded
    );
    let attempts = fixture.jobs.attempts();
    assert_eq!(attempts.len(), 2);
    assert!(attempts[1].resumed_from > 0);

    fixture
        .jobs
        .serve("https://example.test/beta", BODY, Some(6));
    fixture.push_item(item("curated:model:beta", "beta"));
    fixture.jobs.hold("https://example.test/beta");
    let second = fixture.start("curated:model:beta").await.expect("starts");
    fixture.jobs.started(3).await;
    let cancelled = fixture
        .downloads
        .cancel(&id(&second))
        .await
        .expect("cancels");
    fixture.jobs.release("https://example.test/beta");

    assert_eq!(cancelled.status, DownloadStatus::Cancelled);
    assert!(!part(Path::new(&second.target_path)).exists());
    assert!(!Path::new(&second.target_path).exists());
    fixture.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_queue_respects_the_concurrency_cap_and_lets_a_small_package_overtake_a_big_model() {
    let fixture = Fixture::new(1);
    fixture.set_free(500 * GIB);
    fixture.set_items(vec![
        item("curated:model:alpha", "alpha"),
        CatalogueItem {
            size_bytes: 240 * GIB,
            ..item("curated:model:big", "big")
        },
        CatalogueItem {
            kind: DownloadItemKind::Mcp,
            size_bytes: 1024,
            ..item("curated:mcp:small", "small")
        },
    ]);
    for name in ["alpha", "big", "small"] {
        fixture
            .jobs
            .serve(&format!("https://example.test/{name}"), BODY, Some(6));
    }
    fixture.jobs.hold("https://example.test/alpha");

    let blocking = fixture.start("curated:model:alpha").await.expect("starts");
    fixture.jobs.started(1).await;
    let big = fixture.start("curated:model:big").await.expect("queues");
    let small = fixture.start("curated:mcp:small").await.expect("queues");

    assert_eq!(fixture.jobs.attempts().len(), 1);
    let big_now = fixture.downloads.get(&id(&big)).expect("big");
    let small_now = fixture.downloads.get(&id(&small)).expect("small");
    assert_eq!(big_now.status, DownloadStatus::Queued);
    assert_eq!(small_now.status, DownloadStatus::Queued);
    assert!(small_now.priority > big_now.priority);

    fixture.jobs.release("https://example.test/alpha");
    fixture.settle(&id(&blocking)).await;
    fixture.settle(&id(&small)).await;
    fixture.settle(&id(&big)).await;

    let urls: Vec<String> = fixture
        .jobs
        .attempts()
        .into_iter()
        .map(|attempt| attempt.url)
        .collect();
    assert_eq!(
        urls,
        [
            "https://example.test/alpha",
            "https://example.test/small",
            "https://example.test/big",
        ]
    );
    fixture.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raising_the_priority_of_a_waiting_item_moves_it_ahead_of_the_queue() {
    let fixture = Fixture::new(1);
    fixture.set_free(500 * GIB);
    fixture.set_items(vec![
        item("curated:model:alpha", "alpha"),
        item("curated:model:first", "first"),
        item("curated:model:second", "second"),
    ]);
    for name in ["alpha", "first", "second"] {
        fixture
            .jobs
            .serve(&format!("https://example.test/{name}"), BODY, Some(6));
    }
    fixture.jobs.hold("https://example.test/alpha");

    let blocking = fixture.start("curated:model:alpha").await.expect("starts");
    fixture.jobs.started(1).await;
    let first = fixture.start("curated:model:first").await.expect("queues");
    let second = fixture.start("curated:model:second").await.expect("queues");

    let raised = fixture
        .downloads
        .prioritise(&id(&second), second.priority + 1)
        .expect("raises");
    assert_eq!(raised.priority, second.priority + 1);
    assert_eq!(
        fixture
            .downloads
            .prioritise(&id(&blocking), 9)
            .expect_err("refuses")
            .code,
        AppErrorCode::Conflict
    );

    fixture.jobs.release("https://example.test/alpha");
    fixture.settle(&id(&blocking)).await;
    fixture.settle(&id(&second)).await;
    fixture.settle(&id(&first)).await;

    let urls: Vec<String> = fixture
        .jobs
        .attempts()
        .into_iter()
        .map(|attempt| attempt.url)
        .collect();
    assert_eq!(
        urls,
        [
            "https://example.test/alpha",
            "https://example.test/second",
            "https://example.test/first",
        ]
    );
    fixture.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_with_nowhere_to_put_it_is_rejected_before_a_byte_moves() {
    let fixture = Fixture::new(2);
    fixture.set_items(vec![CatalogueItem {
        size_bytes: 380 * GIB,
        ..item("curated:model:huge", "huge")
    }]);
    fixture.jobs.serve("https://example.test/huge", BODY, None);
    fixture.set_free(12 * GIB);

    let refused = fixture
        .start("curated:model:huge")
        .await
        .expect_err("refuses");
    assert_eq!(refused.code, AppErrorCode::Conflict);
    assert!(fixture.jobs.attempts().is_empty());
    assert!(
        fixture
            .downloads
            .list(&DownloadListFilter::default())
            .expect("lists")
            .is_empty()
    );

    let rows = fixture
        .downloads
        .catalogue(&CatalogueFilter::default())
        .await
        .expect("lists");
    assert_eq!(rows[0].state, CatalogueItemState::Available);
    assert!(!rows[0].downloadable);
    assert!(
        rows[0]
            .blocked_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("Недостаточно места"))
    );

    fixture.set_free(380 * GIB + FREE_SPACE_MARGIN_BYTES);
    let freed = fixture
        .downloads
        .catalogue(&CatalogueFilter::default())
        .await
        .expect("lists");
    assert!(freed[0].downloadable);
    fixture.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_catalogue_marks_installed_items_and_flags_a_newer_version_as_an_update() {
    let fixture = Fixture::new(2);
    fixture.set_items(vec![
        CatalogueItem {
            version: Some("q4_K_M".to_owned()),
            digest: Some("aa11".to_owned()),
            ..item("curated:model:alpha", "alpha")
        },
        CatalogueItem {
            version: Some("v2".to_owned()),
            ..item("curated:model:beta", "beta")
        },
    ]);
    *fixture.provider.present.lock().expect("not poisoned") = vec![
        InstalledItem {
            kind: DownloadItemKind::Model,
            name: "alpha".to_owned(),
            version: None,
            size_bytes: 10,
            digest: Some("bb22".to_owned()),
        },
        InstalledItem {
            kind: DownloadItemKind::Model,
            name: "beta".to_owned(),
            version: Some("v2".to_owned()),
            size_bytes: 10,
            digest: None,
        },
    ];

    let rows = fixture
        .downloads
        .catalogue(&CatalogueFilter {
            refresh: Some(true),
            ..CatalogueFilter::default()
        })
        .await
        .expect("lists");
    let by_ref = |item_ref: &str| {
        rows.iter()
            .find(|row| row.item_ref == item_ref)
            .expect("a row")
            .clone()
    };
    assert_eq!(
        by_ref("curated:model:alpha").state,
        CatalogueItemState::Update
    );
    assert_eq!(
        by_ref("curated:model:beta").state,
        CatalogueItemState::Installed
    );
    assert!(!by_ref("curated:model:beta").downloadable);
    fixture.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_catalogue_reports_a_queued_item_as_in_flight() {
    let fixture = Fixture::new(2);
    fixture
        .jobs
        .serve("https://example.test/alpha", BODY, Some(6));
    fixture.jobs.hold("https://example.test/alpha");
    let started = fixture.start("curated:model:alpha").await.expect("starts");
    fixture.jobs.started(1).await;

    let rows = fixture
        .downloads
        .catalogue(&CatalogueFilter::default())
        .await
        .expect("lists");
    assert_eq!(rows[0].state, CatalogueItemState::Downloading);
    assert_eq!(rows[0].download_id, Some(started.id));

    let queried = fixture
        .downloads
        .catalogue(&CatalogueFilter {
            query: Some("ALPH".to_owned()),
            states: Some(vec![CatalogueItemState::Downloading]),
            ..CatalogueFilter::default()
        })
        .await
        .expect("lists");
    assert_eq!(queried.len(), 1);
    let none = fixture
        .downloads
        .catalogue(&CatalogueFilter {
            kinds: Some(vec![DownloadItemKind::Mcp]),
            ..CatalogueFilter::default()
        })
        .await
        .expect("lists");
    assert!(none.is_empty());

    fixture.jobs.release("https://example.test/alpha");
    fixture.settle(&id(&started)).await;
    fixture.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disk_accounting_totals_each_category_and_leaves_the_rest_as_other() {
    let fixture = Fixture::new(2);
    let models = fixture.downloads_dir.join("models");
    let mcp = fixture.downloads_dir.join("mcp");
    std::fs::create_dir_all(&models).expect("creates");
    std::fs::create_dir_all(&mcp).expect("creates");
    std::fs::write(models.join("a.bin"), "x".repeat(300)).expect("writes");
    std::fs::write(mcp.join("b.tgz"), "y".repeat(50)).expect("writes");
    std::fs::write(fixture.vectors_dir.join("index.lance"), "z".repeat(120)).expect("writes");
    std::fs::write(fixture.workspace.join("studio.sqlite"), "q".repeat(70)).expect("writes");

    let usage = fixture.downloads.disk(true).await;
    let bytes = |category: DiskCategory| {
        usage
            .categories
            .iter()
            .find(|entry| entry.category == category)
            .map(|entry| entry.bytes)
    };
    assert_eq!(bytes(DiskCategory::Models), Some(300));
    assert_eq!(bytes(DiskCategory::Mcp), Some(50));
    assert_eq!(bytes(DiskCategory::Embeddings), Some(0));
    assert_eq!(bytes(DiskCategory::Vectors), Some(120));
    assert_eq!(bytes(DiskCategory::Other), Some(70));
    assert_eq!(usage.free_bytes, 100 * GIB);
    assert_eq!(usage.used_bytes, 900 * GIB);
    assert_eq!(usage.root, fixture.downloads_dir.to_string_lossy());

    std::fs::write(models.join("c.bin"), "x".repeat(10)).expect("writes");
    let cached = fixture.downloads.disk(false).await;
    assert_eq!(cached.measured_at, usage.measured_at);
    fixture.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_interrupted_by_a_restart_is_requeued_and_resumes_from_its_partial() {
    let fixture = Fixture::new(2);
    fixture.jobs.serve("https://example.test/alpha", BODY, None);
    let target = fixture.target_of(&fixture.alpha());
    std::fs::create_dir_all(target.parent().expect("a folder")).expect("creates");
    std::fs::write(part(&target), &BODY[..20]).expect("writes");
    let at = fixture.clock.now();
    let row_id = "0193c3c0-0000-7000-8000-000000000001";
    fixture
        .database
        .with_connection(|connection| {
            Repositories::new(connection)
                .downloads
                .create(&DownloadRow {
                    id: row_id.to_owned(),
                    item_kind: DownloadItemKind::Model,
                    item_ref: "curated:model:alpha".to_owned(),
                    display_name: "alpha".to_owned(),
                    version: None,
                    size_bytes: BODY.len() as u64,
                    bytes_done: 20,
                    status: DownloadStatus::Running,
                    priority: 5,
                    target_path: target.to_string_lossy().into_owned(),
                    url: "https://example.test/alpha".to_owned(),
                    checksum_algorithm: None,
                    checksum_value: None,
                    error: None,
                    run_id: None,
                    created_at: at,
                    updated_at: at,
                    started_at: None,
                    finished_at: None,
                })?;
            Ok(())
        })
        .expect("seeds");

    assert_eq!(fixture.downloads.recover().expect("recovers"), 1);
    let finished = fixture.settle(row_id).await;

    assert_eq!(finished.status, DownloadStatus::Succeeded);
    assert_eq!(fixture.jobs.attempts()[0].resumed_from, 20);
    assert_eq!(std::fs::read(&target).expect("landed"), BODY);
    fixture.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn removing_a_finished_download_deletes_the_artefact_and_an_active_one_refuses() {
    let fixture = Fixture::new(2);
    fixture
        .jobs
        .serve("https://example.test/alpha", BODY, Some(6));
    fixture.jobs.hold("https://example.test/alpha");
    let started = fixture.start("curated:model:alpha").await.expect("starts");
    fixture.jobs.started(1).await;

    assert_eq!(
        fixture
            .downloads
            .remove(&id(&started))
            .await
            .expect_err("refuses")
            .code,
        AppErrorCode::Conflict
    );

    fixture.jobs.release("https://example.test/alpha");
    fixture.settle(&id(&started)).await;
    assert!(Path::new(&started.target_path).exists());

    fixture
        .downloads
        .remove(&id(&started))
        .await
        .expect("removes");
    assert!(!Path::new(&started.target_path).exists());
    assert!(
        fixture
            .downloads
            .list(&DownloadListFilter::default())
            .expect("lists")
            .is_empty()
    );
    fixture.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_same_item_cannot_be_queued_twice() {
    let fixture = Fixture::new(2);
    fixture
        .jobs
        .serve("https://example.test/alpha", BODY, Some(6));
    fixture.jobs.hold("https://example.test/alpha");
    let started = fixture.start("curated:model:alpha").await.expect("starts");
    fixture.jobs.started(1).await;

    assert_eq!(
        fixture
            .start("curated:model:alpha")
            .await
            .expect_err("refuses")
            .code,
        AppErrorCode::Conflict
    );
    assert_eq!(
        fixture
            .start("curated:model:nope")
            .await
            .expect_err("refuses")
            .code,
        AppErrorCode::NotFound
    );

    fixture.jobs.release("https://example.test/alpha");
    fixture.settle(&id(&started)).await;
    fixture.dispose().await;
}

struct ScriptedRunner {
    refuse: bool,
}

#[async_trait]
impl DownloadRunner for ScriptedRunner {
    fn start(
        &self,
        _input: zvs_contract::StartRunInput,
    ) -> Result<zvs_contract::RunHandleDto, AppError> {
        if self.refuse {
            return Err(AppError::new(
                AppErrorCode::Conflict,
                "Сервис выполнения остановлен",
            ));
        }
        Ok(zvs_contract::RunHandleDto {
            id: zvs_contract::RunId::new(crate::platform::ids::new_id()),
            stream_id: zvs_contract::StreamId::new(crate::platform::ids::new_id()),
        })
    }

    async fn cancel(&self, _id: &zvs_contract::RunId) -> Result<(), AppError> {
        Ok(())
    }

    fn subscribe(&self, _id: &str, _observer: crate::kernel::RunObserver) -> Result<(), AppError> {
        Ok(())
    }

    fn finished(&self, _id: &zvs_contract::RunId) -> bool {
        true
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_that_cannot_start_or_dies_before_its_node_fails_the_row() {
    let fixture = Fixture::new(1);
    fixture
        .downloads
        .attach(Arc::new(ScriptedRunner { refuse: true }));
    let refused = fixture.start("curated:model:alpha").await.expect("queues");
    assert_eq!(refused.status, DownloadStatus::Failed);
    assert_eq!(
        refused.error.as_deref(),
        Some("Сервис выполнения остановлен")
    );
    assert!(fixture.jobs.attempts().is_empty());

    fixture
        .downloads
        .attach(Arc::new(ScriptedRunner { refuse: false }));
    fixture.downloads.resume(&id(&refused)).expect("resumes");
    let resumed = fixture.downloads.get(&id(&refused)).expect("the row");
    assert_eq!(resumed.status, DownloadStatus::Failed);
    assert_eq!(
        resumed.error.as_deref(),
        Some(super::service::RUN_ENDED_EMPTY)
    );
    assert!(resumed.run_id.is_some());
    fixture.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_closed_service_refuses_new_downloads_and_stops_pumping() {
    let fixture = Fixture::new(2);
    fixture.downloads.dispose();
    assert_eq!(
        fixture
            .start("curated:model:alpha")
            .await
            .expect_err("refuses")
            .message,
        super::service::SERVICE_STOPPED
    );
    fixture.runs.dispose().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_shutdown_mid_transfer_keeps_the_partial_and_leaves_the_row_paused() {
    let fixture = Fixture::new(2);
    fixture
        .jobs
        .serve("https://example.test/alpha", BODY, Some(6));
    fixture.jobs.hold("https://example.test/alpha");
    let started = fixture.start("curated:model:alpha").await.expect("starts");
    fixture.jobs.started(1).await;
    let partial = part(Path::new(&started.target_path));
    for _ in 0..600 {
        if partial.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    fixture.downloads.dispose();
    fixture.runs.dispose().await;

    let row = fixture.downloads.get(&id(&started)).expect("the row");
    assert_eq!(row.status, DownloadStatus::Paused);
    assert!(row.bytes_done > 0);
    assert!(partial.exists());
}
