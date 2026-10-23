use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use flate2::Compression;
use flate2::write::GzEncoder;
use serde_json::{Value, json};
use zvs_contract::{
    AppError, AppErrorCode, CatalogueItemState, ChecksumAlgorithm, DownloadDto, DownloadId,
    DownloadItemKind, DownloadListFilter, DownloadStatus, StartDownloadInput, ToolchainRuntime,
    ToolchainSelectInput, ToolchainSource, ToolchainStatus,
};

use super::definitions::{companion_name, primary_name};
use super::msi::MsiExtractor;
use super::probe::primary_for;
use super::releases::{checksum_line, newest_listed, newest_lts};
use super::service::{ALREADY_DOWNLOADING, NO_CHECKSUM, NOT_MANAGED, SETTINGS_HINT};
use super::*;
use crate::data::schema::DownloadRow;
use crate::downloads::{CatalogueItem, RefMatcher};
use crate::drivers::ai::mock::{MockServer, Reply, direct_http};
use crate::platform::clock::FakeClock;
use crate::platform::logger::silent_logger;
use crate::runtimes::{Platform, RuntimeCatalogue, RuntimeDownloads};
use crate::services::SettingService;
use crate::testing::{
    FakePicker, FakeProbe, TemporaryDatabase, TemporaryDirectory, sha256_hex, temporary_database,
    temporary_directory,
};

const WINDOWS: Platform = Platform::new("win32", "x64");
const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
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
        url: "https://nodejs.test/a.zip".to_owned(),
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
}

#[async_trait]
impl RuntimeDownloads for FakeDownloads {
    async fn start(&self, input: &StartDownloadInput) -> Result<DownloadDto, AppError> {
        lock(&self.started).push(input.item_ref().to_owned());
        let row = download(input.item_ref(), DownloadStatus::Queued);
        lock(&self.rows).push(row.clone());
        Ok(row)
    }

    fn list(&self, _filter: &DownloadListFilter) -> Result<Vec<DownloadDto>, AppError> {
        Ok(lock(&self.rows).clone())
    }

    async fn forget_by_ref(&self, matches: &RefMatcher) -> Result<usize, AppError> {
        let mut rows = lock(&self.rows);
        let before = rows.len();
        rows.retain(|row| !matches(&row.item_ref));
        Ok(before - rows.len())
    }
}

#[derive(Default)]
struct FakeCatalogue {
    offered: Mutex<Vec<CatalogueItem>>,
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
            .find(|item| item.item_ref == item_ref)
            .cloned()
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, item_ref.to_owned()))
    }
}

fn node_index(platform: Platform) -> Value {
    let key = super::definitions::node_file_key(platform).expect("a supported platform");
    json!([
        { "version": "v25.1.0", "lts": false, "files": [key] },
        { "version": "v24.11.0", "lts": "Krypton", "files": ["aix-ppc64"] },
        { "version": "v22.21.0", "lts": "Jod", "files": ["src", key] },
        { "version": "v22.20.0", "lts": "Jod", "files": [key] },
    ])
}

fn uv_release(tag: &str, asset: &str, draft: bool, prerelease: bool, digest: &str) -> Value {
    json!({
        "tag_name": tag,
        "draft": draft,
        "prerelease": prerelease,
        "assets": [
            {
                "name": asset,
                "size": 18_000_000,
                "browser_download_url": format!("https://github.test/{tag}/{asset}"),
                "digest": format!("sha256:{digest}"),
            },
            {
                "name": format!("{asset}.sha256"),
                "size": 90,
                "browser_download_url": format!("SHA/{tag}"),
            },
        ],
    })
}

async fn mock_resolver(platform: Platform) -> (MockServer, ToolchainResolver) {
    let server = MockServer::start().await;
    let asset = super::definitions::node_asset_name("v22.21.0", platform).expect("an asset");
    server.reply("/index.json", Reply::json(node_index(platform)));
    server.reply(
        "/dist/v22.21.0/SHASUMS256.txt",
        Reply::text(format!(
            "{HASH_B}  node-v22.21.0.tar.gz\n{}  {asset}\n",
            HASH_A.to_uppercase()
        )),
    );
    server.reply(
        "/libreoffice/",
        Reply::text(
            "<a href=\"../\">Parent</a><a href=\"25.8.7/\">25.8.7/</a>\
             <a href=\"26.8.1/\">26.8.1/</a><a href=\"26.2.6/\">26.2.6/</a>",
        ),
    );
    server.reply(
        "/libreoffice/26.8.1/win/x86_64/LibreOffice_26.8.1_Win_x86-64.msi.sha256",
        Reply::text(format!("{HASH_A}  LibreOffice_26.8.1_Win_x86-64.msi\n")),
    );
    let resolver = ToolchainResolver::new(ToolchainResolverOptions {
        node_index_url: server.url("/index.json"),
        node_dist_url: server.url("/dist"),
        uv_releases_url: server.url("/releases"),
        libreoffice_url: server.url("/libreoffice"),
        platform,
        ..ToolchainResolverOptions::new(
            direct_http(),
            Arc::new(FakeClock::default()),
            silent_logger(),
        )
    });
    (server, resolver)
}

struct Harness {
    service: ToolchainService,
    server: MockServer,
    downloads: Arc<FakeDownloads>,
    catalogue: Arc<FakeCatalogue>,
    picker: Arc<FakePicker>,
    probe: Arc<FakeProbe>,
    workspace: TemporaryDirectory,
    _database: TemporaryDatabase,
}

impl Harness {
    async fn new() -> Self {
        Self::with_path(None).await
    }

    async fn with_path(path: Option<OsString>) -> Self {
        Self::with_options(path, Vec::new(), Arc::new(FakeMsi)).await
    }

    async fn with_options(
        path: Option<OsString>,
        system_dirs: Vec<PathBuf>,
        msi: Arc<dyn MsiExtractor>,
    ) -> Self {
        let database = temporary_database();
        let workspace = temporary_directory("toolchain-");
        let (server, resolver) = mock_resolver(Platform::current()).await;
        let downloads = Arc::new(FakeDownloads::default());
        let catalogue = Arc::new(FakeCatalogue::default());
        let picker = FakePicker::shared();
        let probe = FakeProbe::shared();
        let service = ToolchainService::new(ToolchainServiceOptions {
            probe: probe.clone(),
            picker: Some(picker.clone()),
            path_env: Some(path.unwrap_or_default()),
            system_dirs: Some(system_dirs),
            msi,
            ..ToolchainServiceOptions::new(
                workspace.join("mcp-toolchain"),
                SettingService::new(database.shared(), Arc::new(FakeClock::default())),
                downloads.clone(),
                catalogue.clone(),
                resolver,
                silent_logger(),
            )
        });
        Self {
            service,
            server,
            downloads,
            catalogue,
            picker,
            probe,
            workspace,
            _database: database,
        }
    }

    fn managed(&self, runtime: ToolchainRuntime, version: &str) -> PathBuf {
        let directory = self
            .workspace
            .join("mcp-toolchain")
            .join(runtime.as_str())
            .join(version)
            .join("bin");
        install_copy(&directory, runtime, &banner(runtime, version))
    }

    fn started(&self) -> Vec<String> {
        lock(&self.downloads.started).clone()
    }
}

fn banner(runtime: ToolchainRuntime, version: &str) -> String {
    match runtime {
        ToolchainRuntime::Node => format!("v{version}"),
        ToolchainRuntime::Uv => format!("uv {version} (abcdef 2025-10-21)"),
        ToolchainRuntime::LibreOffice => format!("LibreOffice {version}.1 bf6ef2662401"),
    }
}

fn install_copy(directory: &Path, runtime: ToolchainRuntime, answer: &str) -> PathBuf {
    let platform = Platform::current();
    std::fs::create_dir_all(directory).expect("creates");
    let primary = directory.join(primary_name(runtime, platform));
    std::fs::write(&primary, answer).expect("writes");
    std::fs::write(directory.join(companion_name(runtime, platform)), "shim").expect("writes");
    primary
}

fn archive(entries: &[(String, String)]) -> Vec<u8> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    for (name, body) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, name, body.as_bytes())
            .expect("appends");
    }
    builder
        .into_inner()
        .expect("finishes")
        .finish()
        .expect("compresses")
}

fn node_archive(version: &str) -> Vec<u8> {
    let platform = Platform::current();
    let folder = format!("node-v{version}");
    archive(&[
        (
            format!(
                "{folder}/{}",
                primary_name(ToolchainRuntime::Node, platform)
            ),
            banner(ToolchainRuntime::Node, version),
        ),
        (
            format!(
                "{folder}/{}",
                companion_name(ToolchainRuntime::Node, platform)
            ),
            "shim".to_owned(),
        ),
    ])
}

fn row_for(item_ref: &str, path: &Path, checksum: Option<String>) -> DownloadRow {
    DownloadRow {
        id: "row".to_owned(),
        item_kind: DownloadItemKind::Runtime,
        item_ref: item_ref.to_owned(),
        display_name: item_ref.to_owned(),
        version: None,
        size_bytes: 1,
        bytes_done: 1,
        status: DownloadStatus::Running,
        priority: 8,
        target_path: path.to_string_lossy().into_owned(),
        url: "https://nodejs.test/archive".to_owned(),
        checksum_algorithm: checksum.as_ref().map(|_| ChecksumAlgorithm::Sha256),
        checksum_value: checksum,
        error: None,
        run_id: None,
        created_at: 1,
        updated_at: 1,
        started_at: None,
        finished_at: None,
    }
}

fn landed(harness: &Harness, version: &str, bytes: &[u8]) -> DownloadRow {
    let path = harness.workspace.join(&format!("node-v{version}.tar.gz"));
    std::fs::write(&path, bytes).expect("writes");
    row_for(
        &format!("toolchain:node:{version}:node-v{version}.tar.gz"),
        &path,
        Some(sha256_hex(bytes)),
    )
}

#[tokio::test]
async fn node_resolves_the_newest_lts_that_ships_for_this_platform_with_its_published_checksum() {
    let (server, resolver) = mock_resolver(WINDOWS).await;
    let build = resolver
        .latest(ToolchainRuntime::Node, true)
        .await
        .expect("resolves");
    assert_eq!(build.version, Version::new(22, 21, 0));
    assert_eq!(build.asset_name, "node-v22.21.0-win-x64.zip");
    assert_eq!(
        build.url,
        server.url("/dist/v22.21.0/node-v22.21.0-win-x64.zip")
    );
    assert_eq!(build.checksum.algorithm, ChecksumAlgorithm::Sha256);
    assert_eq!(build.checksum.value, HASH_A);
    let calls = server.calls().len();
    assert_eq!(
        resolver.known_latest(ToolchainRuntime::Node),
        Some(Version::new(22, 21, 0))
    );
    resolver
        .latest(ToolchainRuntime::Node, false)
        .await
        .expect("cached");
    assert_eq!(server.calls().len(), calls);
    resolver
        .latest(ToolchainRuntime::Node, true)
        .await
        .expect("refreshes");
    assert!(server.calls().len() > calls);
}

#[tokio::test]
async fn a_node_build_without_a_checksum_line_is_refused() {
    let (server, resolver) = mock_resolver(WINDOWS).await;
    server.reply(
        "/dist/v22.21.0/SHASUMS256.txt",
        Reply::text(format!("{HASH_B}  node-v22.21.0-linux-x64.tar.gz\n")),
    );
    let error = resolver
        .latest(ToolchainRuntime::Node, true)
        .await
        .expect_err("refused");
    assert_eq!(error.code, AppErrorCode::NotFound);
    assert_eq!(error.message, super::releases::NO_CHECKSUM);
    assert_eq!(resolver.known_latest(ToolchainRuntime::Node), None);
}

#[tokio::test]
async fn uv_resolves_the_newest_stable_release_and_reads_its_sha256_asset() {
    let (server, resolver) = mock_resolver(WINDOWS).await;
    let asset = "uv-x86_64-pc-windows-msvc.zip";
    let sha_url = server.url("/sha/0.9.5");
    server.reply(
        "/releases",
        Reply::json(
            serde_json::from_str(
                &json!([
                    uv_release("0.10.0", asset, false, true, HASH_A),
                    uv_release("0.9.9", asset, true, false, HASH_A),
                    uv_release("0.9.5", asset, false, false, HASH_A),
                    uv_release(
                        "0.9.6",
                        "uv-x86_64-unknown-linux-gnu.tar.gz",
                        false,
                        false,
                        HASH_A
                    ),
                    uv_release("0.9.4", asset, false, false, HASH_B),
                ])
                .to_string()
                .replace("SHA/0.9.5", &sha_url),
            )
            .expect("json"),
        ),
    );
    server.reply("/sha/0.9.5", Reply::text(format!("{HASH_A} *{asset}\n")));
    let build = resolver
        .latest(ToolchainRuntime::Uv, true)
        .await
        .expect("resolves");
    assert_eq!(build.version, Version::new(0, 9, 5));
    assert_eq!(build.asset_name, asset);
    assert_eq!(build.url, format!("https://github.test/0.9.5/{asset}"));
    assert_eq!(build.size_bytes, 18_000_000);
    assert_eq!(build.checksum.value, HASH_A);
}

#[tokio::test]
async fn a_uv_sha256_asset_that_disagrees_with_the_github_digest_is_refused() {
    let (server, resolver) = mock_resolver(WINDOWS).await;
    let asset = "uv-x86_64-pc-windows-msvc.zip";
    let sha_url = server.url("/sha");
    server.reply(
        "/releases",
        Reply::json(
            serde_json::from_str(
                &json!([uv_release("0.9.5", asset, false, false, HASH_A)])
                    .to_string()
                    .replace("SHA/0.9.5", &sha_url),
            )
            .expect("json"),
        ),
    );
    server.reply("/sha", Reply::text(format!("{HASH_B}  {asset}")));
    let error = resolver
        .latest(ToolchainRuntime::Uv, true)
        .await
        .expect_err("refused");
    assert_eq!(error.message, super::releases::CHECKSUM_MISMATCH);
}

#[test]
fn checksum_listings_and_the_lts_index_are_read_strictly() {
    let listing = format!("{HASH_A}  node-v1.0.0-win-x64.zip\nnot a hash  other.zip\n");
    assert_eq!(
        checksum_line(&listing, "node-v1.0.0-win-x64.zip").as_deref(),
        Some(HASH_A)
    );
    assert_eq!(checksum_line(&listing, "other.zip"), None);
    assert_eq!(checksum_line(&listing, "node-v1.0.0-win-x64"), None);
    let index = node_index(WINDOWS);
    assert_eq!(
        newest_lts(&index, "win-x64-zip").as_deref(),
        Some("v22.21.0")
    );
    assert_eq!(newest_lts(&index, "aix-ppc64").as_deref(), Some("v24.11.0"));
    assert_eq!(newest_lts(&index, "sunos-x64"), None);
}

#[tokio::test]
async fn a_copy_is_probed_for_its_companion_its_banner_and_the_minimum_version() {
    let workspace = temporary_directory("toolchain-probe-");
    let platform = Platform::current();
    let probe_with = FakeProbe::shared();
    let check = |path: PathBuf, runtime: ToolchainRuntime| {
        let probe_with = probe_with.clone();
        async move { probe(probe_with.as_ref(), runtime, platform, &path).await }
    };

    let good = install_copy(&workspace.join("good"), ToolchainRuntime::Node, "v22.11.0");
    assert_eq!(
        check(good.clone(), ToolchainRuntime::Node)
            .await
            .expect("passes")
            .version,
        Version::new(22, 11, 0)
    );

    let lonely = install_copy(
        &workspace.join("lonely"),
        ToolchainRuntime::Node,
        "v22.11.0",
    );
    std::fs::remove_file(
        lonely
            .parent()
            .expect("a directory")
            .join(companion_name(ToolchainRuntime::Node, platform)),
    )
    .expect("removes");
    let reason = check(lonely, ToolchainRuntime::Node)
        .await
        .expect_err("refused");
    assert!(
        reason.contains(companion_name(ToolchainRuntime::Node, platform)),
        "{reason}"
    );

    let old = install_copy(&workspace.join("old"), ToolchainRuntime::Node, "v16.20.2");
    let reason = check(old, ToolchainRuntime::Node)
        .await
        .expect_err("refused");
    assert!(
        reason.contains("16.20.2") && reason.contains("18.0.0"),
        "{reason}"
    );

    let python = install_copy(
        &workspace.join("python"),
        ToolchainRuntime::Uv,
        "Python 3.12.0",
    );
    assert!(check(python, ToolchainRuntime::Uv).await.is_err());

    let broken = install_copy(
        &workspace.join("broken"),
        ToolchainRuntime::Uv,
        "fail: boom",
    );
    let reason = check(broken, ToolchainRuntime::Uv)
        .await
        .expect_err("refused");
    assert!(reason.contains("boom"), "{reason}");

    let uv = install_copy(
        &workspace.join("uv"),
        ToolchainRuntime::Uv,
        "uv 0.9.5 (d5f39331a 2025-10-21)",
    );
    assert_eq!(
        check(uv, ToolchainRuntime::Uv)
            .await
            .expect("passes")
            .version,
        Version::new(0, 9, 5)
    );

    let reason = check(
        workspace.join("missing").join("node"),
        ToolchainRuntime::Node,
    )
    .await
    .expect_err("refused");
    assert!(!reason.is_empty());
    let picked_companion = good
        .parent()
        .expect("a directory")
        .join(companion_name(ToolchainRuntime::Node, platform));
    assert_eq!(
        primary_for(ToolchainRuntime::Node, platform, &picked_companion),
        good
    );
}

#[tokio::test]
async fn install_connects_an_existing_managed_copy_without_a_release_lookup() {
    let harness = Harness::new().await;
    harness.managed(ToolchainRuntime::Node, "20.18.0");
    let newest = harness.managed(ToolchainRuntime::Node, "22.11.0");
    let dto = harness
        .service
        .install(ToolchainRuntime::Node)
        .await
        .expect("installs");
    assert_eq!(dto.status, ToolchainStatus::Ready);
    assert_eq!(dto.source, Some(ToolchainSource::Managed));
    assert_eq!(dto.version.as_deref(), Some("22.11.0"));
    assert_eq!(
        dto.executable_path.as_deref(),
        Some(newest.to_string_lossy().as_ref())
    );
    assert_eq!(dto.managed_versions, ["22.11.0", "20.18.0"]);
    assert!(harness.started().is_empty());
    assert!(harness.server.calls().is_empty());
}

#[tokio::test]
async fn install_without_a_copy_queues_the_latest_lts_with_its_checksum_and_waits_for_it() {
    let harness = Harness::new().await;
    let dto = harness
        .service
        .install(ToolchainRuntime::Node)
        .await
        .expect("queues");
    let asset =
        super::definitions::node_asset_name("v22.21.0", Platform::current()).expect("an asset");
    let expected = format!("toolchain:node:22.21.0:{asset}");
    assert_eq!(harness.started(), std::slice::from_ref(&expected));
    assert_eq!(dto.status, ToolchainStatus::Installing);
    assert_eq!(dto.refs, std::slice::from_ref(&expected));
    let offered = lock(&harness.catalogue.offered).clone();
    assert_eq!(offered[0].item_ref, expected);
    assert_eq!(offered[0].kind, DownloadItemKind::Runtime);
    assert_eq!(
        offered[0]
            .checksum
            .as_ref()
            .map(|checksum| checksum.value.as_str()),
        Some(HASH_A)
    );
    assert_eq!(offered[0].file_name, asset);
    let again = harness
        .service
        .install(ToolchainRuntime::Node)
        .await
        .expect_err("busy");
    assert_eq!(again.message, ALREADY_DOWNLOADING);
}

#[tokio::test]
async fn a_verified_download_is_unpacked_and_connected_and_only_then_replaces_the_old_copy() {
    let harness = Harness::new().await;
    harness.managed(ToolchainRuntime::Node, "22.11.0");
    harness
        .service
        .install(ToolchainRuntime::Node)
        .await
        .expect("connects");
    let row = landed(&harness, "22.21.0", &node_archive("22.21.0"));
    harness
        .service
        .unpack_download(&row)
        .await
        .expect("unpacks");
    let dto = harness.service.get(ToolchainRuntime::Node).expect("reads");
    assert_eq!(dto.status, ToolchainStatus::Ready);
    assert_eq!(dto.version.as_deref(), Some("22.21.0"));
    assert_eq!(dto.managed_versions, ["22.21.0"]);
    let executable = PathBuf::from(dto.executable_path.expect("a path"));
    assert!(executable.starts_with(harness.service.root().join("node").join("22.21.0")));
    assert!(executable.is_file());
    assert!(!Path::new(&row.target_path).exists());
    assert!(
        harness
            .probe
            .probed()
            .iter()
            .any(|path| path.to_string_lossy().contains(".staging-22.21.0"))
    );
}

#[tokio::test]
async fn a_download_that_fails_its_probe_leaves_the_working_copy_connected() {
    let harness = Harness::new().await;
    let working = harness.managed(ToolchainRuntime::Node, "22.11.0");
    harness
        .service
        .install(ToolchainRuntime::Node)
        .await
        .expect("connects");
    let platform = Platform::current();
    let broken = archive(&[
        (
            format!("node/{}", primary_name(ToolchainRuntime::Node, platform)),
            "fail: crashed".to_owned(),
        ),
        (
            format!("node/{}", companion_name(ToolchainRuntime::Node, platform)),
            "shim".to_owned(),
        ),
    ]);
    let row = landed(&harness, "22.21.0", &broken);
    let error = harness
        .service
        .unpack_download(&row)
        .await
        .expect_err("refused");
    assert_eq!(error.code, AppErrorCode::Conflict);
    assert!(error.message.contains("crashed"), "{}", error.message);
    let dto = harness.service.get(ToolchainRuntime::Node).expect("reads");
    assert_eq!(dto.version.as_deref(), Some("22.11.0"));
    assert_eq!(
        dto.executable_path.as_deref(),
        Some(working.to_string_lossy().as_ref())
    );
    assert_eq!(dto.managed_versions, ["22.11.0"]);
    assert!(
        !harness
            .service
            .root()
            .join("node")
            .join(".staging-22.21.0")
            .exists()
    );

    let mismatched = landed(&harness, "22.21.0", &node_archive("22.20.0"));
    assert!(harness.service.unpack_download(&mismatched).await.is_err());
    assert_eq!(
        harness
            .service
            .get(ToolchainRuntime::Node)
            .expect("reads")
            .version
            .as_deref(),
        Some("22.11.0")
    );
}

#[tokio::test]
async fn a_download_without_a_checksum_is_never_connected_and_other_rows_are_ignored() {
    let harness = Harness::new().await;
    let path = harness.workspace.join("node.tar.gz");
    std::fs::write(&path, node_archive("22.21.0")).expect("writes");
    let row = row_for("toolchain:node:22.21.0:node.tar.gz", &path, None);
    let error = harness
        .service
        .unpack_download(&row)
        .await
        .expect_err("refused");
    assert_eq!(error.message, NO_CHECKSUM);
    assert_eq!(
        harness
            .service
            .get(ToolchainRuntime::Node)
            .expect("reads")
            .status,
        ToolchainStatus::Missing
    );
    let foreign = row_for("runtime:llama-cpp-cpu:b1:a.zip", &path, None);
    harness
        .service
        .unpack_download(&foreign)
        .await
        .expect("ignored");
    assert!(path.exists());
}

#[tokio::test]
async fn an_update_is_offered_only_for_a_newer_managed_copy_and_queues_it() {
    let harness = Harness::new().await;
    harness.managed(ToolchainRuntime::Node, "22.11.0");
    harness
        .service
        .install(ToolchainRuntime::Node)
        .await
        .expect("connects");
    let quiet = harness.service.list(false).await.expect("lists");
    assert!(quiet.iter().all(|dto| !dto.update_available));
    assert!(harness.server.calls().is_empty());
    let checked = harness.service.list(true).await.expect("lists");
    let node = checked
        .iter()
        .find(|dto| dto.runtime == ToolchainRuntime::Node)
        .expect("node");
    assert!(node.update_available);
    assert_eq!(node.latest_version.as_deref(), Some("22.21.0"));
    let uv = checked
        .iter()
        .find(|dto| dto.runtime == ToolchainRuntime::Uv)
        .expect("uv");
    assert_eq!(uv.status, ToolchainStatus::Missing);
    assert!(!uv.update_available);

    let updating = harness
        .service
        .update(ToolchainRuntime::Node)
        .await
        .expect("queues");
    assert_eq!(updating.status, ToolchainStatus::Installing);
    assert_eq!(updating.version.as_deref(), Some("22.11.0"));
    assert_eq!(harness.started().len(), 1);

    let refused = harness
        .service
        .update(ToolchainRuntime::Uv)
        .await
        .expect_err("not managed");
    assert_eq!(refused.message, NOT_MANAGED);
}

#[tokio::test]
async fn an_up_to_date_managed_copy_is_left_alone_by_update() {
    let harness = Harness::new().await;
    harness.managed(ToolchainRuntime::Node, "22.21.0");
    harness
        .service
        .install(ToolchainRuntime::Node)
        .await
        .expect("connects");
    let dto = harness
        .service
        .update(ToolchainRuntime::Node)
        .await
        .expect("checks");
    assert_eq!(dto.status, ToolchainStatus::Ready);
    assert!(!dto.update_available);
    assert!(harness.started().is_empty());
}

#[tokio::test]
async fn scan_connects_the_first_passing_copy_in_path_and_explains_a_refusal() {
    let workspace = temporary_directory("toolchain-path-");
    let old = workspace.join("old");
    let good = workspace.join("good");
    install_copy(&old, ToolchainRuntime::Node, "v16.0.0");
    let expected = install_copy(&good, ToolchainRuntime::Node, "v22.11.0");
    let path = std::env::join_paths([old.clone(), workspace.join("empty"), good]).expect("joins");
    let harness = Harness::with_path(Some(path)).await;
    let dto = harness
        .service
        .scan(ToolchainRuntime::Node)
        .await
        .expect("finds");
    assert_eq!(dto.source, Some(ToolchainSource::Path));
    assert_eq!(
        dto.executable_path.as_deref(),
        Some(expected.to_string_lossy().as_ref())
    );

    let missing = harness
        .service
        .scan(ToolchainRuntime::Uv)
        .await
        .expect_err("absent");
    assert_eq!(missing.code, AppErrorCode::NotFound);

    let only_old = Harness::with_path(Some(old.into_os_string())).await;
    let refused = only_old
        .service
        .scan(ToolchainRuntime::Node)
        .await
        .expect_err("refused");
    assert_eq!(refused.code, AppErrorCode::Conflict);
    assert!(refused.message.contains("16.0.0"), "{}", refused.message);
}

#[tokio::test]
async fn select_probes_the_picked_file_and_a_cancelled_pick_changes_nothing() {
    let harness = Harness::new().await;
    let platform = Platform::current();
    let primary = install_copy(
        &harness.workspace.join("tools"),
        ToolchainRuntime::Uv,
        "uv 0.9.5",
    );
    let cancelled = harness
        .service
        .select(&ToolchainSelectInput {
            runtime: ToolchainRuntime::Uv,
            path: None,
        })
        .await
        .expect("cancelled");
    assert_eq!(cancelled.status, ToolchainStatus::Missing);

    let companion = primary
        .parent()
        .expect("a directory")
        .join(companion_name(ToolchainRuntime::Uv, platform));
    harness
        .picker
        .answer(&[companion.to_string_lossy().as_ref()]);
    let picked = harness
        .service
        .select(&ToolchainSelectInput {
            runtime: ToolchainRuntime::Uv,
            path: None,
        })
        .await
        .expect("connects");
    assert_eq!(picked.source, Some(ToolchainSource::Manual));
    assert_eq!(
        picked.executable_path.as_deref(),
        Some(primary.to_string_lossy().as_ref())
    );

    let wrong = harness.workspace.join("readme.txt");
    std::fs::write(&wrong, "text").expect("writes");
    let refused = harness
        .service
        .select(&ToolchainSelectInput {
            runtime: ToolchainRuntime::Uv,
            path: Some(wrong.to_string_lossy().into_owned()),
        })
        .await
        .expect_err("refused");
    assert_eq!(refused.code, AppErrorCode::Conflict);
    assert_eq!(
        harness
            .service
            .get(ToolchainRuntime::Uv)
            .expect("reads")
            .source,
        Some(ToolchainSource::Manual)
    );
}

#[tokio::test]
async fn remove_disconnects_and_deletes_the_copies_the_app_downloaded_but_nothing_else() {
    let workspace = temporary_directory("toolchain-remove-");
    install_copy(
        &workspace.join("system"),
        ToolchainRuntime::Node,
        "v22.11.0",
    );
    let harness = Harness::with_path(Some(workspace.join("system").into_os_string())).await;
    harness.managed(ToolchainRuntime::Node, "20.18.0");
    harness
        .service
        .scan(ToolchainRuntime::Node)
        .await
        .expect("finds");
    let dto = harness
        .service
        .remove(ToolchainRuntime::Node)
        .await
        .expect("removes");
    assert_eq!(dto.status, ToolchainStatus::Missing);
    assert!(dto.managed_versions.is_empty());
    assert!(workspace.join("system").exists());
    assert!(harness.service.remove(ToolchainRuntime::Node).await.is_ok());
}

#[tokio::test]
async fn npx_launches_from_the_connected_runtime_with_its_directory_first_on_path() {
    let original =
        std::env::join_paths([PathBuf::from("first"), PathBuf::from("second")]).expect("joins");
    let harness = Harness::with_path(Some(original)).await;
    let platform = Platform::current();
    let primary = harness.managed(ToolchainRuntime::Node, "22.11.0");
    harness
        .service
        .install(ToolchainRuntime::Node)
        .await
        .expect("connects");
    let directory = primary.parent().expect("a directory").to_path_buf();
    let args = vec![
        "-y".to_owned(),
        "@modelcontextprotocol/server-memory".to_owned(),
    ];
    let spec = harness
        .service
        .launch_command("npx", &args)
        .expect("a runtime command")
        .expect("launches");
    assert_eq!(
        spec.command,
        directory
            .join(companion_name(ToolchainRuntime::Node, platform))
            .to_string_lossy()
    );
    assert_eq!(spec.args, args);
    let path: Vec<PathBuf> = std::env::split_paths(&spec.env["PATH"]).collect();
    assert_eq!(
        path,
        [
            directory.clone(),
            PathBuf::from("first"),
            PathBuf::from("second")
        ]
    );
    let cache = PathBuf::from(&spec.env["npm_config_cache"]);
    assert!(cache.starts_with(harness.service.root()));
    assert!(PathBuf::from(&spec.env["UV_TOOL_DIR"]).starts_with(harness.service.root()));

    let node = harness
        .service
        .launch_command("node.exe", &[])
        .expect("a runtime command")
        .expect("launches");
    assert_eq!(node.command, primary.to_string_lossy());
    assert!(harness.service.launch_command("python", &[]).is_none());
    assert!(
        harness
            .service
            .launch_command("C:/tools/npx.cmd", &[])
            .is_none()
    );

    let missing = harness
        .service
        .launch_command("uvx", &["mcp-server-time".to_owned()])
        .expect("a runtime command")
        .expect_err("not connected");
    assert_eq!(missing.code, AppErrorCode::NotFound);
    assert!(
        missing.message.ends_with(SETTINGS_HINT),
        "{}",
        missing.message
    );
    assert_eq!(
        missing
            .details
            .as_ref()
            .and_then(|details| details.get("runtime")),
        Some(&json!("uv"))
    );
}

#[tokio::test]
async fn a_connected_copy_that_disappears_is_broken_and_refuses_to_launch() {
    let harness = Harness::new().await;
    let primary = install_copy(
        &harness.workspace.join("manual"),
        ToolchainRuntime::Node,
        "v22.11.0",
    );
    harness
        .service
        .select(&ToolchainSelectInput {
            runtime: ToolchainRuntime::Node,
            path: Some(primary.to_string_lossy().into_owned()),
        })
        .await
        .expect("connects");
    std::fs::remove_file(&primary).expect("removes");
    let dto = harness.service.get(ToolchainRuntime::Node).expect("reads");
    assert_eq!(dto.status, ToolchainStatus::Broken);
    assert!(dto.problem.is_some());
    let error = harness
        .service
        .launch_command("npx", &[])
        .expect("a runtime command")
        .expect_err("broken");
    assert!(error.message.ends_with(SETTINGS_HINT));
}

#[tokio::test]
async fn the_downloads_catalogue_shows_both_runtimes_in_the_settings_state() {
    let harness = Harness::new().await;
    harness.managed(ToolchainRuntime::Node, "22.11.0");
    harness
        .service
        .install(ToolchainRuntime::Node)
        .await
        .expect("connects");
    let provider = harness.service.catalogue_provider();
    let rows = provider.list().await.expect("lists");
    let state = |runtime: ToolchainRuntime, rows: &[CatalogueItem]| {
        rows.iter()
            .find(|row| row.item_ref == toolchain_catalogue_ref(runtime))
            .and_then(|row| row.state)
    };
    assert_eq!(rows.len(), if cfg!(windows) { 3 } else { 2 });
    assert!(rows.iter().all(|row| row.kind == DownloadItemKind::Runtime));
    assert_eq!(
        state(ToolchainRuntime::Node, &rows),
        Some(CatalogueItemState::Installed)
    );
    assert_eq!(
        state(ToolchainRuntime::Uv, &rows),
        Some(CatalogueItemState::Available)
    );
    harness.service.list(true).await.expect("checks");
    let rows = provider.list().await.expect("lists");
    assert_eq!(
        state(ToolchainRuntime::Node, &rows),
        Some(CatalogueItemState::Update)
    );
    harness
        .service
        .update(ToolchainRuntime::Node)
        .await
        .expect("queues");
    let rows = provider.list().await.expect("lists");
    assert_eq!(
        state(ToolchainRuntime::Node, &rows),
        Some(CatalogueItemState::Downloading)
    );
    let installed = provider.installed().await.expect("lists");
    assert_eq!(installed[0].name, "node");
    assert_eq!(installed[0].version.as_deref(), Some("22.11.0"));
    assert_eq!(
        runtime_of_catalogue_ref("toolchain:uv"),
        Some(ToolchainRuntime::Uv)
    );
    assert_eq!(runtime_of_catalogue_ref("runtime:uv"), None);
    assert!(parse_toolchain_ref("toolchain:node:22.1.0:../evil.zip").is_none());
}

struct FakeMsi;

#[async_trait]
impl MsiExtractor for FakeMsi {
    async fn extract(&self, package: &Path, target: &Path) -> Result<(), AppError> {
        let version = std::fs::read_to_string(package).expect("reads the package");
        install_copy(
            &target.join("program"),
            ToolchainRuntime::LibreOffice,
            &banner(ToolchainRuntime::LibreOffice, version.trim()),
        );
        Ok(())
    }
}

#[tokio::test]
async fn libreoffice_resolves_the_newest_stable_msi_with_its_published_checksum() {
    let (_server, resolver) = mock_resolver(WINDOWS).await;
    let build = resolver
        .latest(ToolchainRuntime::LibreOffice, true)
        .await
        .expect("resolves");
    assert_eq!(build.version, Version::new(26, 8, 1));
    assert_eq!(build.asset_name, "LibreOffice_26.8.1_Win_x86-64.msi");
    assert!(
        build
            .url
            .ends_with("/26.8.1/win/x86_64/LibreOffice_26.8.1_Win_x86-64.msi")
    );
    assert_eq!(build.checksum.value, HASH_A);
    let (_server, linux) = mock_resolver(Platform::new("linux", "x64")).await;
    let error = linux
        .latest(ToolchainRuntime::LibreOffice, true)
        .await
        .expect_err("no managed build");
    assert_eq!(error.code, AppErrorCode::UnsupportedFormat);
    assert_eq!(
        newest_listed("<a href=\"7.6.0/\">"),
        Some(Version::new(7, 6, 0))
    );
    assert_eq!(newest_listed("<a href=\"latest/\">"), None);
}

#[tokio::test]
async fn libreoffice_installed_in_the_system_is_found_and_connected_as_a_system_copy() {
    let workspace = temporary_directory("toolchain-office-");
    let program = workspace.join("LibreOffice").join("program");
    let expected = install_copy(
        &program,
        ToolchainRuntime::LibreOffice,
        &banner(ToolchainRuntime::LibreOffice, "26.8.1"),
    );
    let harness = Harness::with_options(None, vec![program], Arc::new(FakeMsi)).await;
    let dto = harness
        .service
        .scan(ToolchainRuntime::LibreOffice)
        .await
        .expect("finds");
    assert_eq!(dto.source, Some(ToolchainSource::System));
    assert_eq!(dto.version.as_deref(), Some("26.8.1"));
    assert_eq!(
        dto.executable_path.as_deref(),
        Some(expected.to_string_lossy().as_ref())
    );

    let empty = Harness::new().await;
    let missing = empty
        .service
        .scan(ToolchainRuntime::LibreOffice)
        .await
        .expect_err("absent");
    assert_eq!(missing.code, AppErrorCode::NotFound);
    assert_eq!(missing.message, "LibreOffice не найден в системе");
}

#[tokio::test]
async fn the_converter_detects_an_installed_libreoffice_on_first_use() {
    let workspace = temporary_directory("toolchain-office-");
    let program = workspace.join("program");
    let old = install_copy(
        &program,
        ToolchainRuntime::LibreOffice,
        &banner(ToolchainRuntime::LibreOffice, "6.4.7"),
    );
    let harness = Harness::with_options(None, vec![program.clone()], Arc::new(FakeMsi)).await;
    assert_eq!(harness.service.office_executable().await, None);
    assert_eq!(
        harness
            .service
            .get(ToolchainRuntime::LibreOffice)
            .expect("reads")
            .status,
        ToolchainStatus::Missing
    );
    std::fs::write(&old, banner(ToolchainRuntime::LibreOffice, "26.8.1")).expect("upgrades");
    assert_eq!(harness.service.office_executable().await, Some(old.clone()));
    let dto = harness
        .service
        .get(ToolchainRuntime::LibreOffice)
        .expect("reads");
    assert_eq!(dto.status, ToolchainStatus::Ready);
    assert_eq!(dto.source, Some(ToolchainSource::System));
}

#[tokio::test]
async fn a_libreoffice_msi_is_unpacked_through_the_msi_extractor_and_connected() {
    let harness = Harness::new().await;
    let package = harness.workspace.join("LibreOffice_26.8.1_Win_x86-64.msi");
    std::fs::write(&package, "26.8.1").expect("writes");
    let row = row_for(
        "toolchain:libreoffice:26.8.1:LibreOffice_26.8.1_Win_x86-64.msi",
        &package,
        Some(HASH_A.to_owned()),
    );
    harness
        .service
        .unpack_download(&row)
        .await
        .expect("unpacks");
    let dto = harness
        .service
        .get(ToolchainRuntime::LibreOffice)
        .expect("reads");
    assert_eq!(dto.status, ToolchainStatus::Ready);
    assert_eq!(dto.source, Some(ToolchainSource::Managed));
    assert_eq!(dto.managed_versions, ["26.8.1"]);
    let executable = PathBuf::from(dto.executable_path.expect("a path"));
    assert!(
        executable.starts_with(
            harness
                .service
                .root()
                .join("libreoffice")
                .join("26.8.1")
                .join("program")
        )
    );
    assert_eq!(harness.service.office_executable().await, Some(executable));
    assert!(!package.exists());
}

#[test]
fn an_admin_image_loses_its_copy_of_the_package() {
    let workspace = temporary_directory("toolchain-msi-");
    let target = workspace.join("image");
    std::fs::create_dir_all(target.join("program")).expect("creates");
    std::fs::write(target.join("lo.msi"), "package").expect("writes");
    std::fs::write(target.join("program").join("soffice.com"), "x").expect("writes");
    super::msi::remove_packages(&target);
    assert!(!target.join("lo.msi").exists());
    assert!(target.join("program").join("soffice.com").exists());
}

#[tokio::test]
async fn libreoffice_is_a_component_in_the_downloads_catalogue() {
    let harness = Harness::new().await;
    let rows = harness
        .service
        .catalogue_provider()
        .list()
        .await
        .expect("lists");
    let office = rows
        .iter()
        .find(|row| row.item_ref == toolchain_catalogue_ref(ToolchainRuntime::LibreOffice));
    if cfg!(windows) {
        let office = office.expect("a LibreOffice row on Windows");
        assert_eq!(office.tags, ["компонент"]);
        assert_eq!(office.state, Some(CatalogueItemState::Available));
    } else {
        assert!(office.is_none());
    }
}
