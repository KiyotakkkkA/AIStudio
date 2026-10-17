use std::sync::Mutex;

use zvs_contract::{DeviceGpuDto, DeviceProfileDto, DeviceTier, DownloadId};

use super::*;
use crate::drivers::ai::mock::{MockServer, Reply, direct_http};
use crate::platform::clock::FakeClock;
use crate::platform::logger::silent_logger;
use crate::runtimes::llama_server::EngineServer;
use crate::runtimes::releases::ReleaseResolverOptions;
use crate::testing::{
    TemporaryDatabase, TemporaryDirectory, temporary_database, temporary_directory,
};

const TAG: &str = "b10970";

fn release_payload() -> Value {
    let asset = |name: &str, size: u64| {
        json!({
            "name": name,
            "size": size,
            "state": "uploaded",
            "browser_download_url": format!("https://github.test/{name}"),
            "digest": format!("sha256:{}", "a".repeat(64)),
        })
    };
    json!([
        { "tag_name": "v0.4.1", "draft": false, "prerelease": false, "assets": [asset("nightly-tag.txt", 7)] },
        {
            "tag_name": TAG,
            "draft": false,
            "prerelease": true,
            "assets": [
                asset(&format!("llama-{TAG}-bin-win-cpu-x64.zip"), 18_428_751),
                asset(&format!("llama-{TAG}-bin-win-vulkan-x64.zip"), 31_675_940),
                asset(&format!("llama-{TAG}-bin-win-cuda-13.3-x64.zip"), 149_722_826),
                asset("cudart-llama-bin-win-cuda-13.3-x64.zip", 390_970_417),
            ],
        },
    ])
}

fn device(gpu: Option<DeviceGpuDto>) -> DeviceProfileDto {
    DeviceProfileDto {
        platform: "win32".to_owned(),
        arch: "x64".to_owned(),
        cpu_model: "Test CPU".to_owned(),
        cpu_cores: 16,
        total_memory_bytes: 32 << 30,
        free_memory_bytes: 16 << 30,
        free_disk_bytes: 500 << 30,
        gpu,
        tier: DeviceTier::High,
        measured_at: 1,
    }
}

fn gpu(vendor: &str, model: &str) -> Option<DeviceGpuDto> {
    Some(DeviceGpuDto {
        vendor: vendor.to_owned(),
        model: model.to_owned(),
        vram_bytes: None,
        discrete: true,
    })
}

struct FakeDevice(Option<DeviceGpuDto>);

#[async_trait]
impl RuntimeDevice for FakeDevice {
    async fn profile(&self) -> Option<DeviceProfileDto> {
        Some(device(self.0.clone()))
    }
}

fn download(item_ref: &str, status: DownloadStatus) -> DownloadDto {
    DownloadDto {
        id: DownloadId::new(crate::platform::ids::new_id()),
        item_kind: DownloadItemKind::Runtime,
        item_ref: item_ref.to_owned(),
        display_name: item_ref.to_owned(),
        size_bytes: 1,
        bytes_done: 0,
        status,
        priority: 8,
        target_path: "C:/downloads/runtimes/a.zip".to_owned(),
        url: "https://github.test/a.zip".to_owned(),
        checksum: None,
        version: None,
        error: None,
        run_id: None,
        rate_bytes_per_second: 0.0,
        eta_ms: None,
        created_at: 1,
        updated_at: 1,
        started_at: None,
        finished_at: None,
    }
}

#[derive(Default)]
struct FakeDownloads {
    started: Mutex<Vec<String>>,
    rows: Mutex<Vec<DownloadDto>>,
    forgotten: Mutex<Vec<usize>>,
}

#[async_trait]
impl RuntimeDownloads for FakeDownloads {
    async fn start(&self, input: &StartDownloadInput) -> Result<DownloadDto, AppError> {
        lock(&self.started).push(input.item_ref().to_owned());
        Ok(download(input.item_ref(), DownloadStatus::Queued))
    }

    fn list(&self, filter: &DownloadListFilter) -> Result<Vec<DownloadDto>, AppError> {
        Ok(lock(&self.rows)
            .iter()
            .filter(|row| {
                filter
                    .kinds
                    .as_ref()
                    .is_none_or(|kinds| kinds.contains(&row.item_kind))
            })
            .cloned()
            .collect())
    }

    async fn forget_by_ref(&self, matches: &RefMatcher) -> Result<usize, AppError> {
        let mut rows = lock(&self.rows);
        let before = rows.len();
        rows.retain(|row| !matches(&row.item_ref));
        let removed = before - rows.len();
        lock(&self.forgotten).push(removed);
        Ok(removed)
    }
}

#[derive(Default)]
struct FakeCatalogue {
    offered: Mutex<Vec<CatalogueItem>>,
    items: Mutex<Vec<CatalogueItem>>,
}

#[async_trait]
impl RuntimeCatalogue for FakeCatalogue {
    fn offer(&self, item: CatalogueItem) -> CatalogueItem {
        lock(&self.offered).push(item.clone());
        item
    }

    async fn find(&self, item_ref: &str) -> Result<CatalogueItem, AppError> {
        lock(&self.offered)
            .iter()
            .chain(lock(&self.items).iter())
            .find(|item| item.item_ref == item_ref)
            .cloned()
            .ok_or_else(|| {
                AppError::new(
                    AppErrorCode::NotFound,
                    format!("No catalogue item: {item_ref}"),
                )
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Spawned {
    model_path: String,
    gpu_layers: u32,
    role: LlamaServerRole,
    mmproj_path: Option<String>,
}

struct FakeServer {
    options: LlamaServerOptions,
    running: std::sync::atomic::AtomicBool,
    prompts: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl EngineServer for FakeServer {
    fn running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn model_path(&self) -> String {
        self.options.model_path.clone()
    }

    fn role(&self) -> LlamaServerRole {
        self.options.role
    }

    async fn embed(
        &self,
        _texts: &[String],
        _cancel: &CancellationToken,
    ) -> Result<Vec<Vec<f32>>, AppError> {
        Ok(vec![vec![0.5, 0.25]])
    }

    async fn rerank(
        &self,
        _query: &str,
        passages: &[String],
        _cancel: &CancellationToken,
    ) -> Result<Vec<f64>, AppError> {
        Ok((0..passages.len()).map(|index| index as f64).collect())
    }

    async fn describe_image(
        &self,
        _media_type: &str,
        _bytes: &[u8],
        prompt: &str,
        _cancel: &CancellationToken,
    ) -> Result<String, AppError> {
        lock(&self.prompts).push(prompt.to_owned());
        Ok("  распознанный текст \n".to_owned())
    }

    fn stop(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    async fn stopped(&self) {}
}

struct Fixture {
    service: RuntimeService,
    downloads: Arc<FakeDownloads>,
    catalogue: Arc<FakeCatalogue>,
    spawned: Arc<Mutex<Vec<Spawned>>>,
    prompts: Arc<Mutex<Vec<String>>>,
    server: MockServer,
    workspace: TemporaryDirectory,
    _database: TemporaryDatabase,
    settings: SettingService,
}

impl Fixture {
    async fn new(gpu: Option<DeviceGpuDto>) -> Self {
        Self::on(gpu, Platform::new("win32", "x64")).await
    }

    async fn on(gpu: Option<DeviceGpuDto>, platform: Platform) -> Self {
        let database = temporary_database();
        let workspace = temporary_directory("runtimes-");
        let clock = Arc::new(FakeClock::default());
        let settings = SettingService::new(database.shared(), clock.clone());
        let server = MockServer::start().await;
        server.reply("/releases", Reply::json(release_payload()));
        let resolver = ReleaseResolver::new(ReleaseResolverOptions {
            url: server.url("/releases"),
            platform,
            ..ReleaseResolverOptions::new(direct_http(), clock, silent_logger())
        });
        let downloads = Arc::new(FakeDownloads::default());
        let catalogue = Arc::new(FakeCatalogue::default());
        let spawned = Arc::new(Mutex::new(Vec::new()));
        let prompts = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&spawned);
        let asked = Arc::clone(&prompts);
        let create_server: ServerFactory = Arc::new(move |options: LlamaServerOptions| {
            lock(&recorded).push(Spawned {
                model_path: options.model_path.clone(),
                gpu_layers: options.gpu_layers,
                role: options.role,
                mmproj_path: options.mmproj_path.clone(),
            });
            Arc::new(FakeServer {
                options,
                running: std::sync::atomic::AtomicBool::new(true),
                prompts: Arc::clone(&asked),
            }) as SharedEngineServer
        });
        let service = RuntimeService::new(RuntimeServiceOptions {
            downloads: downloads.clone(),
            catalogue: catalogue.clone(),
            settings: settings.clone(),
            downloads_dir: workspace.path().to_path_buf(),
            device: Some(Arc::new(FakeDevice(gpu))),
            resolver,
            http: direct_http(),
            logger: silent_logger(),
            create_server: Some(create_server),
            context_size: None,
        });
        Self {
            service,
            downloads,
            catalogue,
            spawned,
            prompts,
            server,
            workspace,
            _database: database,
            settings,
        }
    }

    fn mark_installed(&self, id: &str, executable: &str) {
        let install_path = self.workspace.join("runtimes").join(id);
        let executable_path = if executable.is_empty() {
            String::new()
        } else {
            install_path.join(executable).to_string_lossy().into_owned()
        };
        self.settings
            .set(
                INSTALLED_KEY,
                &json!({
                    id: {
                        "tag": TAG,
                        "installPath": install_path.to_string_lossy(),
                        "executablePath": executable_path,
                        "sizeBytes": 1024,
                    }
                }),
            )
            .expect("stores");
    }

    fn weights(&self, folder: &str, name: &str) {
        let directory = self.workspace.join(folder);
        std::fs::create_dir_all(&directory).expect("creates");
        std::fs::write(directory.join(name), "weights").expect("writes");
    }

    fn spawned(&self) -> Vec<Spawned> {
        lock(&self.spawned).clone()
    }

    fn stored(&self) -> Option<Value> {
        self.settings
            .get(INSTALLED_KEY)
            .expect("reads")
            .map(|setting| setting.value)
    }
}

fn zip_with(name: &str, body: &[u8]) -> Vec<u8> {
    let name = name.as_bytes();
    let size = u32::try_from(body.len()).expect("small");
    let name_length = u16::try_from(name.len()).expect("short");
    let mut out = Vec::new();
    out.extend_from_slice(&0x0403_4b50_u32.to_le_bytes());
    out.extend_from_slice(&20_u16.to_le_bytes());
    out.extend_from_slice(&[0; 12]);
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&name_length.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(body);
    let central_at = u32::try_from(out.len()).expect("small");
    out.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
    out.extend_from_slice(&20_u16.to_le_bytes());
    out.extend_from_slice(&20_u16.to_le_bytes());
    out.extend_from_slice(&[0; 12]);
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&name_length.to_le_bytes());
    out.extend_from_slice(&[0; 12]);
    out.extend_from_slice(&0_u32.to_le_bytes());
    out.extend_from_slice(name);
    let central_size = u32::try_from(out.len()).expect("small") - central_at;
    out.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_at.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out
}

fn archive_row(item_kind: DownloadItemKind, item_ref: &str, target: &Path) -> DownloadRow {
    DownloadRow {
        id: crate::platform::ids::new_id().to_string(),
        item_kind,
        item_ref: item_ref.to_owned(),
        display_name: item_ref.to_owned(),
        version: None,
        size_bytes: 0,
        bytes_done: 0,
        status: DownloadStatus::Running,
        priority: 8,
        target_path: target.to_string_lossy().into_owned(),
        url: "https://github.test/a.zip".to_owned(),
        checksum_algorithm: None,
        checksum_value: None,
        error: None,
        run_id: None,
        created_at: 1,
        updated_at: 1,
        started_at: None,
        finished_at: None,
    }
}

fn row<'a>(overview: &'a RuntimeOverviewDto, id: &str) -> &'a RuntimeDto {
    overview
        .runtimes
        .iter()
        .find(|runtime| runtime.id == id)
        .expect("a runtime row")
}

fn cancel() -> CancellationToken {
    CancellationToken::new()
}

#[tokio::test]
async fn opening_the_page_costs_no_network_call_and_marks_the_recommended_build() {
    let fixture = Fixture::new(gpu("NVIDIA", "RTX 4070")).await;

    let overview = fixture.service.overview().await.expect("an overview");

    assert!(fixture.server.calls().is_empty());
    assert_eq!(overview.plan.accelerator, Accelerator::Vulkan);
    assert!(!overview.plan.ready);
    assert_eq!(
        overview.plan.gpu.as_ref().map(|gpu| gpu.model.as_str()),
        Some("RTX 4070")
    );
    assert_eq!(
        overview
            .runtimes
            .iter()
            .find(|runtime| runtime.recommended)
            .map(|runtime| runtime.id.as_str()),
        Some("llama-cpp-vulkan")
    );
    let metal = row(&overview, "llama-cpp-metal");
    assert_eq!(metal.state, RuntimeState::Unsupported);
    assert_eq!(metal.blocked_reason.as_deref(), Some(UNSUPPORTED_PLATFORM));
    assert_eq!(
        row(&overview, "llama-cpp-cpu").state,
        RuntimeState::Available
    );
    assert!(
        overview
            .recommended_models
            .contains(&"curated:embedding:bge-m3-f16".to_owned())
    );
}

#[tokio::test]
async fn installing_with_no_id_queues_the_archives_of_the_recommended_build() {
    let fixture = Fixture::new(gpu("AMD", "RX 7800")).await;

    let started = fixture
        .service
        .install(&InstallRuntimeInput::default())
        .await
        .expect("installs");

    let offered = lock(&fixture.catalogue.offered).clone();
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].kind, DownloadItemKind::Runtime);
    assert_eq!(
        offered[0].file_name,
        format!("llama-{TAG}-bin-win-vulkan-x64.zip")
    );
    assert_eq!(offered[0].version.as_deref(), Some(TAG));
    assert!(offered[0].checksum.is_some());
    assert_eq!(
        lock(&fixture.downloads.started).clone(),
        [runtime_ref(
            "llama-cpp-vulkan",
            TAG,
            &format!("llama-{TAG}-bin-win-vulkan-x64.zip")
        )]
    );
    assert_eq!(started.len(), 1);
    assert_eq!(fixture.server.calls().len(), 1);
}

#[tokio::test]
async fn a_cuda_install_queues_the_build_and_its_runtime_libraries() {
    let fixture = Fixture::new(None).await;

    fixture
        .service
        .install(&InstallRuntimeInput {
            id: Some("llama-cpp-cuda".to_owned()),
        })
        .await
        .expect("installs");

    assert_eq!(
        lock(&fixture.downloads.started).clone(),
        [
            runtime_ref(
                "llama-cpp-cuda",
                TAG,
                &format!("llama-{TAG}-bin-win-cuda-13.3-x64.zip")
            ),
            runtime_ref(
                "llama-cpp-cuda",
                TAG,
                "cudart-llama-bin-win-cuda-13.3-x64.zip"
            ),
        ]
    );
}

#[tokio::test]
async fn a_build_this_system_has_no_asset_for_is_refused_without_a_request() {
    let fixture = Fixture::new(None).await;

    let refused = fixture
        .service
        .install(&InstallRuntimeInput {
            id: Some("llama-cpp-metal".to_owned()),
        })
        .await
        .expect_err("refuses");

    assert_eq!(refused.code, AppErrorCode::UnsupportedFormat);
    assert!(refused.message.contains("llama.cpp · Metal (Apple)"));
    assert!(fixture.server.calls().is_empty());
    assert_eq!(
        fixture
            .service
            .install(&InstallRuntimeInput {
                id: Some("llama-cpp-nope".to_owned()),
            })
            .await
            .expect_err("refuses")
            .code,
        AppErrorCode::NotFound
    );
}

#[tokio::test]
async fn an_unpacked_archive_records_the_build_tag_and_the_server_it_found() {
    let fixture = Fixture::new(None).await;
    let archives = fixture.workspace.join("runtimes");
    std::fs::create_dir_all(&archives).expect("creates");
    let archive = archives.join(format!("llama-{TAG}-bin-win-vulkan-x64.zip"));
    std::fs::write(&archive, zip_with("build/bin/llama-server.exe", b"binary")).expect("writes");

    fixture
        .service
        .unpack_download(&archive_row(
            DownloadItemKind::Runtime,
            &runtime_ref(
                "llama-cpp-vulkan",
                TAG,
                &format!("llama-{TAG}-bin-win-vulkan-x64.zip"),
            ),
            &archive,
        ))
        .await
        .expect("unpacks");

    let overview = fixture.service.overview().await.expect("an overview");
    let vulkan = row(&overview, "llama-cpp-vulkan");
    assert_eq!(vulkan.state, RuntimeState::Installed);
    assert_eq!(vulkan.installed_version.as_deref(), Some(TAG));
    assert!(
        vulkan
            .executable_path
            .as_deref()
            .is_some_and(|path| path.ends_with("llama-server.exe"))
    );
    assert_eq!(vulkan.size_bytes, 6);
    assert!(overview.plan.ready);
    assert!(!archive.exists());
    assert!(
        fixture
            .workspace
            .join("runtimes")
            .join("llama-cpp-vulkan")
            .join("build")
            .join("bin")
            .join("llama-server.exe")
            .is_file()
    );

    let companion = archives.join("cudart-llama-bin-win-cuda-13.3-x64.zip");
    std::fs::write(&companion, zip_with("cudart64_13.dll", b"dll")).expect("writes");
    fixture
        .service
        .unpack_download(&archive_row(
            DownloadItemKind::Runtime,
            &runtime_ref(
                "llama-cpp-vulkan",
                TAG,
                "cudart-llama-bin-win-cuda-13.3-x64.zip",
            ),
            &companion,
        ))
        .await
        .expect("unpacks");
    let after = fixture.service.overview().await.expect("an overview");
    assert_eq!(row(&after, "llama-cpp-vulkan").size_bytes, 9);
    assert!(row(&after, "llama-cpp-vulkan").executable_path.is_some());
}

#[tokio::test]
async fn a_download_of_another_kind_is_left_alone_by_the_unpacker() {
    let fixture = Fixture::new(None).await;

    fixture
        .service
        .unpack_download(&archive_row(
            DownloadItemKind::Embedding,
            "curated:embedding:bge-m3-f16",
            &fixture
                .workspace
                .join("embeddings")
                .join("bge-m3-FP16.gguf"),
        ))
        .await
        .expect("ignores");
    fixture
        .service
        .unpack_download(&archive_row(
            DownloadItemKind::Runtime,
            "runtime:llama-cpp-vulkan",
            &fixture.workspace.join("x.zip"),
        ))
        .await
        .expect("ignores");

    assert_eq!(fixture.stored(), None);
}

#[tokio::test]
async fn embedding_without_an_installed_engine_explains_what_to_do_about_it() {
    let fixture = Fixture::new(None).await;

    let error = fixture
        .service
        .embed("bge-m3-FP16.gguf", &["привет".to_owned()], &cancel())
        .await
        .expect_err("refuses");

    assert_eq!(error.code, AppErrorCode::Conflict);
    assert_eq!(error.message, NO_ENGINE);
}

#[tokio::test]
async fn an_onnx_file_is_refused_with_its_own_reason_not_the_missing_engine_one() {
    let fixture = Fixture::new(None).await;

    let error = fixture
        .service
        .embed("bge-m3.onnx", &["привет".to_owned()], &cancel())
        .await
        .expect_err("refuses");

    assert_eq!(error.code, AppErrorCode::Conflict);
    assert_eq!(error.message, NO_ONNX_ENGINE);
    assert_eq!(
        fixture
            .service
            .embed("notes.txt", &["a".to_owned()], &cancel())
            .await
            .expect_err("refuses")
            .code,
        AppErrorCode::UnsupportedFormat
    );
    assert!(!fixture.service.supports(ModelFormat::Gguf));
}

#[tokio::test]
async fn an_accelerated_build_is_started_with_layers_on_the_gpu_and_a_cpu_build_with_none() {
    let fixture = Fixture::new(None).await;
    fixture.weights("embeddings", "bge-m3-FP16.gguf");
    fixture.mark_installed("llama-cpp-vulkan", "llama-server.exe");

    let vectors = fixture
        .service
        .embed("bge-m3-FP16.gguf", &["привет".to_owned()], &cancel())
        .await
        .expect("embeds");

    assert_eq!(vectors, [vec![0.5_f32, 0.25]]);
    let spawned = fixture.spawned();
    assert_eq!(spawned.len(), 1);
    assert_eq!(spawned[0].gpu_layers, 999);
    assert!(spawned[0].model_path.ends_with("bge-m3-FP16.gguf"));
    assert!(fixture.service.supports(ModelFormat::Gguf));

    fixture
        .service
        .embed("bge-m3-FP16.gguf", &["снова".to_owned()], &cancel())
        .await
        .expect("embeds");
    assert_eq!(fixture.spawned().len(), 1);
    let overview = fixture.service.overview().await.expect("an overview");
    assert_eq!(
        row(&overview, "llama-cpp-vulkan").state,
        RuntimeState::Ready
    );

    assert_eq!(
        fixture
            .service
            .embed("missing.gguf", &["a".to_owned()], &cancel())
            .await
            .expect_err("refuses")
            .code,
        AppErrorCode::NotFound
    );
}

#[test]
fn a_runtime_ref_survives_a_round_trip_and_rejects_anything_else() {
    let item_ref = runtime_ref(
        "llama-cpp-cuda",
        TAG,
        "cudart-llama-bin-win-cuda-13.3-x64.zip",
    );

    assert_eq!(
        parse_runtime_ref(&item_ref),
        Some(RuntimeArchiveRef {
            runtime_id: "llama-cpp-cuda".to_owned(),
            tag: TAG.to_owned(),
            asset_name: "cudart-llama-bin-win-cuda-13.3-x64.zip".to_owned(),
        })
    );
    assert_eq!(parse_runtime_ref("curated:embedding:bge-m3-f16"), None);
    assert_eq!(parse_runtime_ref("runtime:llama-cpp-cuda:v1:a.zip"), None);
}

#[tokio::test]
async fn the_catalogue_offers_one_row_per_build_not_one_per_archive() {
    let fixture = Fixture::new(gpu("NVIDIA", "RTX 4070")).await;

    let rows = fixture
        .service
        .catalogue_provider()
        .list()
        .await
        .expect("lists");

    let refs: Vec<&str> = rows.iter().map(|row| row.item_ref.as_str()).collect();
    assert_eq!(
        refs,
        [
            runtime_catalogue_ref("llama-cpp-cpu"),
            runtime_catalogue_ref("llama-cpp-vulkan"),
            runtime_catalogue_ref("llama-cpp-cuda"),
        ]
    );
    assert!(rows.iter().all(|row| row.kind == DownloadItemKind::Runtime));
    assert!(!rows.iter().any(|row| row.item_ref.contains("metal")));
    assert!(
        rows.iter()
            .find(|row| row.item_ref.contains("vulkan"))
            .is_some_and(|row| row.tags.contains(&"рекомендуется".to_owned()))
    );
    assert!(rows.iter().all(|row| row.url == LLAMA_RELEASES_PAGE));
    assert!(fixture.server.calls().is_empty());
}

#[tokio::test]
async fn a_catalogue_row_reports_its_own_installed_state_archives_or_not() {
    let fixture = Fixture::new(None).await;
    let provider = fixture.service.catalogue_provider();
    let vulkan = |rows: &[CatalogueItem]| {
        rows.iter()
            .find(|row| row.name == "llama-cpp-vulkan")
            .cloned()
            .expect("a vulkan row")
    };

    assert_eq!(vulkan(&provider.list().await.expect("lists")).state, None);

    fixture.mark_installed("llama-cpp-vulkan", "llama-server.exe");

    let installed = vulkan(&provider.list().await.expect("lists"));
    assert_eq!(installed.state, Some(CatalogueItemState::Installed));
    assert_eq!(installed.version.as_deref(), Some(TAG));
    assert_eq!(installed.size_bytes, 1024);
    assert_eq!(
        provider.installed().await.expect("lists"),
        [InstalledItem {
            kind: DownloadItemKind::Runtime,
            name: "llama-cpp-vulkan".to_owned(),
            version: Some(TAG.to_owned()),
            size_bytes: 1024,
            digest: None,
        }]
    );

    fixture.mark_installed("llama-cpp-vulkan", "");
    assert_eq!(vulkan(&provider.list().await.expect("lists")).state, None);
}

#[tokio::test]
async fn a_build_with_archives_in_flight_reads_as_downloading() {
    let fixture = Fixture::new(None).await;
    lock(&fixture.downloads.rows).push(download(
        &runtime_ref(
            "llama-cpp-cuda",
            TAG,
            &format!("llama-{TAG}-bin-win-cuda-13.3-x64.zip"),
        ),
        DownloadStatus::Running,
    ));

    let rows = fixture
        .service
        .catalogue_provider()
        .list()
        .await
        .expect("lists");

    let state = |name: &str| {
        rows.iter()
            .find(|row| row.name == name)
            .and_then(|row| row.state)
    };
    assert_eq!(
        state("llama-cpp-cuda"),
        Some(CatalogueItemState::Downloading)
    );
    assert_eq!(state("llama-cpp-vulkan"), None);
    let overview = fixture.service.overview().await.expect("an overview");
    let cuda = row(&overview, "llama-cpp-cuda");
    assert_eq!(cuda.state, RuntimeState::Installing);
    assert_eq!(cuda.refs.len(), 1);
}

#[tokio::test]
async fn removing_a_build_deletes_it_from_disk_from_settings_and_from_the_download_rows() {
    let fixture = Fixture::new(None).await;
    let target = fixture.workspace.join("runtimes").join("llama-cpp-vulkan");
    std::fs::create_dir_all(&target).expect("creates");
    std::fs::write(target.join("llama-server.exe"), "binary").expect("writes");
    fixture.mark_installed("llama-cpp-vulkan", "llama-server.exe");
    lock(&fixture.downloads.rows).push(download(
        &runtime_ref(
            "llama-cpp-vulkan",
            TAG,
            &format!("llama-{TAG}-bin-win-vulkan-x64.zip"),
        ),
        DownloadStatus::Succeeded,
    ));
    lock(&fixture.downloads.rows).push(download(
        &runtime_ref(
            "llama-cpp-cpu",
            TAG,
            &format!("llama-{TAG}-bin-win-cpu-x64.zip"),
        ),
        DownloadStatus::Succeeded,
    ));

    fixture
        .service
        .uninstall("llama-cpp-vulkan")
        .await
        .expect("removes");

    assert!(!target.exists());
    assert_eq!(fixture.stored(), Some(json!({})));
    assert_eq!(lock(&fixture.downloads.forgotten).clone(), [1]);
    assert_eq!(lock(&fixture.downloads.rows).len(), 1);
    assert!(
        fixture
            .service
            .catalogue_provider()
            .list()
            .await
            .expect("lists")
            .iter()
            .all(|row| row.state.is_none())
    );
}

#[tokio::test]
async fn removing_a_build_that_is_not_installed_is_refused_rather_than_silently_ignored() {
    let fixture = Fixture::new(None).await;

    assert_eq!(
        fixture
            .service
            .uninstall("llama-cpp-vulkan")
            .await
            .expect_err("refuses")
            .code,
        AppErrorCode::Conflict
    );
    assert_eq!(
        fixture
            .service
            .uninstall("llama-cpp-nope")
            .await
            .expect_err("refuses")
            .code,
        AppErrorCode::NotFound
    );
}

#[test]
fn a_catalogue_ref_round_trips_to_its_build_id_and_ignores_an_archive_ref() {
    assert_eq!(
        runtime_id_of_catalogue_ref(&runtime_catalogue_ref("llama-cpp-cuda")).as_deref(),
        Some("llama-cpp-cuda")
    );
    assert_eq!(
        runtime_id_of_catalogue_ref(&runtime_ref("llama-cpp-cuda", TAG, "a.zip")),
        None
    );
    assert_eq!(
        runtime_id_of_catalogue_ref("curated:embedding:bge-m3-f16"),
        None
    );
}

fn reranker() -> CatalogueItem {
    CatalogueItem {
        display_name: "BGE Reranker v2 M3".to_owned(),
        size_bytes: 1,
        file_name: "bge-reranker-v2-m3-FP16.gguf".to_owned(),
        url: "https://example.test/reranker.gguf".to_owned(),
        ..CatalogueItem::new(
            "curated:model:bge-reranker-v2-m3-f16",
            DownloadItemKind::Model,
            CatalogueSource::Curated,
            "bge-reranker-v2-m3",
        )
    }
}

#[tokio::test]
async fn reranking_runs_its_own_server_over_the_catalogue_model_beside_the_embedding_one() {
    let fixture = Fixture::new(None).await;
    lock(&fixture.catalogue.items).push(reranker());
    fixture.weights("models", "bge-reranker-v2-m3-FP16.gguf");
    fixture.weights("embeddings", "bge-m3-FP16.gguf");
    fixture.mark_installed("llama-cpp-cpu", "llama-server.exe");

    fixture
        .service
        .embed("bge-m3-FP16.gguf", &["a".to_owned()], &cancel())
        .await
        .expect("embeds");
    let scores = fixture
        .service
        .rerank(
            "curated:model:bge-reranker-v2-m3-f16",
            "query",
            &["a".to_owned(), "b".to_owned()],
            &cancel(),
        )
        .await
        .expect("reranks");
    fixture
        .service
        .rerank(
            "curated:model:bge-reranker-v2-m3-f16",
            "again",
            &["c".to_owned()],
            &cancel(),
        )
        .await
        .expect("reranks");

    assert_eq!(scores, [0.0, 1.0]);
    let spawned = fixture.spawned();
    assert_eq!(
        spawned.iter().map(|given| given.role).collect::<Vec<_>>(),
        [LlamaServerRole::Embedding, LlamaServerRole::Rerank]
    );
    assert!(
        spawned[1]
            .model_path
            .ends_with("bge-reranker-v2-m3-FP16.gguf")
    );
    assert_eq!(spawned[1].gpu_layers, 0);

    let mut onnx = reranker();
    onnx.item_ref = "curated:model:onnx-reranker".to_owned();
    onnx.file_name = "reranker.onnx".to_owned();
    lock(&fixture.catalogue.items).push(onnx);
    assert_eq!(
        fixture
            .service
            .rerank(
                "curated:model:onnx-reranker",
                "q",
                &["a".to_owned()],
                &cancel()
            )
            .await
            .expect_err("refuses")
            .code,
        AppErrorCode::UnsupportedFormat
    );
}

#[tokio::test]
async fn reading_an_image_pairs_the_vision_model_with_the_projector_that_shares_its_name() {
    let fixture = Fixture::new(None).await;
    fixture.mark_installed("llama-cpp-vulkan", "llama-server.exe");
    lock(&fixture.catalogue.items).push(CatalogueItem {
        file_name: "Qwen2.5-VL-3B-Instruct-Q4_K_M.gguf".to_owned(),
        ..CatalogueItem::new(
            "curated:model:qwen2.5-vl-3b-instruct-q4_k_m",
            DownloadItemKind::Model,
            CatalogueSource::Curated,
            "qwen2.5-vl:3b",
        )
    });
    fixture.weights("models", "Qwen2.5-VL-3B-Instruct-Q4_K_M.gguf");

    let missing = fixture
        .service
        .read_image(
            "curated:model:qwen2.5-vl-3b-instruct-q4_k_m",
            "image/png",
            b"png",
            VectorOcrLanguage::Rus,
            &cancel(),
        )
        .await
        .expect_err("refuses");
    assert_eq!(missing.code, AppErrorCode::NotFound);
    assert_eq!(missing.message, NO_PROJECTOR);

    fixture.weights("models", "MMPROJ-another-model-F16.gguf");
    fixture.weights("models", "mmproj-Qwen2.5-VL-3B-Instruct-Q8_0.gguf");
    let text = fixture
        .service
        .read_image(
            "curated:model:qwen2.5-vl-3b-instruct-q4_k_m",
            "image/png",
            b"png",
            VectorOcrLanguage::Rus,
            &cancel(),
        )
        .await
        .expect("reads");

    assert_eq!(text, "распознанный текст");
    let spawned = fixture.spawned();
    assert_eq!(spawned[0].role, LlamaServerRole::Vision);
    assert!(
        spawned[0]
            .mmproj_path
            .as_deref()
            .is_some_and(|path| path.ends_with("mmproj-Qwen2.5-VL-3B-Instruct-Q8_0.gguf"))
    );
    let prompts = lock(&fixture.prompts).clone();
    assert!(prompts[0].ends_with(" Текст на русском языке."));
    assert!(ocr_prompt(VectorOcrLanguage::Auto).ends_with("ответь пустой строкой."));
}

#[tokio::test]
async fn stopping_a_build_reports_whether_anything_was_running_and_dispose_stops_everything() {
    let fixture = Fixture::new(None).await;
    fixture.weights("embeddings", "bge-m3-FP16.gguf");
    fixture.mark_installed("llama-cpp-vulkan", "llama-server.exe");
    fixture
        .service
        .embed("bge-m3-FP16.gguf", &["a".to_owned()], &cancel())
        .await
        .expect("embeds");

    assert!(fixture.service.stop("llama-cpp-vulkan"));
    assert!(!fixture.service.stop("llama-cpp-vulkan"));
    assert!(!fixture.service.stop("llama-cpp-cpu"));

    fixture
        .service
        .embed("bge-m3-FP16.gguf", &["a".to_owned()], &cancel())
        .await
        .expect("embeds");
    fixture.service.dispose().await;
    let overview = fixture.service.overview().await.expect("an overview");
    assert_eq!(
        row(&overview, "llama-cpp-vulkan").state,
        RuntimeState::Installed
    );
}
