use std::path::Path;
use std::time::Duration;

use flate2::Compression;
use flate2::write::GzEncoder;
use serde_json::{Value, json};
use zvs_contract::{
    CatalogueSource, ChecksumAlgorithm, ChecksumDto, DownloadItemKind, DownloadStatus,
    StartDownloadInput, ToolchainDto, ToolchainRuntime, ToolchainStatus,
};

use super::*;
use crate::downloads::CatalogueItem;
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::platform::logger::silent_logger;
use crate::runtimes::Platform;
use crate::services::SystemService;
use crate::testing::{TestServices, sha256_hex, test_services};
use crate::toolchain::definitions::{companion_name, primary_name};

fn registered() -> (TestServices, IpcServer) {
    let host = test_services(SystemService::new(
        Arc::new(FakeClock::default()),
        EventBus::default(),
        Duration::ZERO,
    ));
    let mut server = IpcServer::new(host.shared(), silent_logger()).with_output_validation(true);
    register(&mut server);
    (host, server)
}

fn copy_in(directory: &Path, runtime: ToolchainRuntime, banner: &str) -> String {
    let platform = Platform::current();
    std::fs::create_dir_all(directory).expect("creates");
    let primary = directory.join(primary_name(runtime, platform));
    std::fs::write(&primary, banner).expect("writes");
    std::fs::write(directory.join(companion_name(runtime, platform)), "shim").expect("writes");
    primary.to_string_lossy().into_owned()
}

fn uv_archive(version: &str) -> Vec<u8> {
    let platform = Platform::current();
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    for (name, body) in [
        (
            primary_name(ToolchainRuntime::Uv, platform),
            format!("uv {version}"),
        ),
        (
            companion_name(ToolchainRuntime::Uv, platform),
            "shim".to_owned(),
        ),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, format!("uv-{version}/{name}"), body.as_bytes())
            .expect("appends");
    }
    builder
        .into_inner()
        .expect("finishes")
        .finish()
        .expect("compresses")
}

fn runtime_of(listed: &Value, runtime: &str) -> ToolchainDto {
    let rows: Vec<ToolchainDto> =
        serde_json::from_value(listed["data"].clone()).expect("toolchain rows");
    rows.into_iter()
        .find(|row| row.runtime.as_str() == runtime)
        .expect("the runtime")
}

#[tokio::test]
async fn the_channels_connect_a_picked_copy_refuse_an_empty_path_and_disconnect() {
    let (host, server) = registered();
    let listed = server.dispatch("toolchain.list", json!({})).await;
    assert_eq!(listed["ok"], json!(true), "{listed}");
    assert_eq!(listed["data"].as_array().map(Vec::len), Some(3));
    assert_eq!(runtime_of(&listed, "node").status, ToolchainStatus::Missing);

    let primary = copy_in(
        &host.workspace.join("node"),
        ToolchainRuntime::Node,
        "v22.11.0",
    );
    let selected = server
        .dispatch(
            "toolchain.select",
            json!({ "runtime": "node", "path": primary }),
        )
        .await;
    assert_eq!(selected["ok"], json!(true), "{selected}");
    assert_eq!(selected["data"]["source"], json!("manual"));
    assert_eq!(selected["data"]["version"], json!("22.11.0"));

    let scanned = server
        .dispatch("toolchain.scan", json!({ "runtime": "uv" }))
        .await;
    assert_eq!(scanned["error"]["code"], json!("NOT_FOUND"));
    assert_eq!(
        server
            .dispatch("toolchain.update", json!({ "runtime": "node" }))
            .await["error"]["code"],
        json!("CONFLICT")
    );
    assert_eq!(
        server
            .dispatch("toolchain.install", json!({ "runtime": "python" }))
            .await["error"]["code"],
        json!("VALIDATION_FAILED")
    );

    let removed = server
        .dispatch("toolchain.remove", json!({ "runtime": "node" }))
        .await;
    assert_eq!(removed["data"]["status"], json!("missing"), "{removed}");
    assert!(Path::new(&primary).exists());
}

#[tokio::test]
async fn a_download_finished_from_the_downloads_page_connects_itself_without_a_scan() {
    let (host, server) = registered();
    let bytes = uv_archive("0.9.5");
    let asset = "uv-0.9.5.tar.gz";
    let item_ref = format!("toolchain:uv:0.9.5:{asset}");
    let url = "https://github.test/uv-0.9.5.tar.gz";
    host.download_jobs.serve(url, &bytes, None);
    host.downloads.catalogue_source().offer(CatalogueItem {
        url: url.to_owned(),
        file_name: asset.to_owned(),
        version: Some("0.9.5".to_owned()),
        checksum: Some(ChecksumDto {
            algorithm: ChecksumAlgorithm::Sha256,
            value: sha256_hex(&bytes),
        }),
        ..CatalogueItem::new(
            item_ref.clone(),
            DownloadItemKind::Runtime,
            CatalogueSource::Github,
            "uv/uv-0.9.5.tar.gz",
        )
    });
    let started = host
        .downloads
        .start(&StartDownloadInput::new(item_ref))
        .await
        .expect("starts");
    let mut status = started.status;
    for _ in 0..600 {
        status = host
            .downloads
            .get(&started.id.to_string())
            .expect("a row")
            .status;
        if !matches!(status, DownloadStatus::Queued | DownloadStatus::Running) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(status, DownloadStatus::Succeeded);

    let listed = server.dispatch("toolchain.list", json!({})).await;
    let uv = runtime_of(&listed, "uv");
    assert_eq!(uv.status, ToolchainStatus::Ready, "{listed}");
    assert_eq!(uv.version.as_deref(), Some("0.9.5"));
    assert_eq!(uv.managed_versions, ["0.9.5"]);
    let catalogue = host
        .downloads
        .catalogue(&zvs_contract::CatalogueFilter::default())
        .await
        .expect("a catalogue");
    let row = catalogue
        .iter()
        .find(|row| row.item_ref == "toolchain:uv")
        .expect("the uv row");
    assert_eq!(row.state, zvs_contract::CatalogueItemState::Installed);
    assert_eq!(row.installed_version.as_deref(), Some("0.9.5"));
}
