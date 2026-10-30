use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::{Value, json};
use zvs_contract::{DownloadItemKind, DownloadStatus};

use super::*;
use crate::data::schema::DownloadRow;
use crate::downloads::DownloadInstaller;
use crate::platform::audio::{CaptureDevice, SAMPLE_RATE};
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::platform::logger::silent_logger;
use crate::services::SystemService;
use crate::testing::{TestServices, test_services};
use crate::voice::catalogue::{ENGINE_REF, SPEECH_MODELS};
use crate::voice::{
    UNNAMED_DEVICE, VOICE_DEVICE_SETTING_KEY, VOICE_ENGINE_SETTING_KEY, VOICE_MODEL_SETTING_KEY,
    recording_devices,
};

const SMALL: &str = "curated:speech:whisper-small-q8_0";

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

fn device(id: &str, name: &str, is_default: bool) -> CaptureDevice {
    CaptureDevice {
        id: id.to_owned(),
        name: name.to_owned(),
        is_default,
    }
}

fn data(envelope: Value) -> Value {
    assert_eq!(envelope["ok"], json!(true), "{envelope}");
    envelope["data"].clone()
}

fn code(envelope: &Value) -> &str {
    envelope["error"]["code"].as_str().unwrap_or_default()
}

fn model_state<'a>(models: &'a Value, model_ref: &str) -> &'a Value {
    &models["models"]
        .as_array()
        .and_then(|list| list.iter().find(|model| model["ref"] == json!(model_ref)))
        .expect("a model row")["state"]
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

fn landed_row(item_kind: DownloadItemKind, item_ref: &str, target: &Path) -> DownloadRow {
    DownloadRow {
        id: crate::platform::ids::new_id().to_string(),
        item_kind,
        item_ref: item_ref.to_owned(),
        display_name: item_ref.to_owned(),
        version: Some("v1.9.2".to_owned()),
        size_bytes: 0,
        bytes_done: 0,
        status: DownloadStatus::Running,
        priority: 8,
        target_path: target.to_string_lossy().into_owned(),
        url: "https://github.test/whisper.zip".to_owned(),
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

fn install_engine_and_model(host: &TestServices) {
    let services = host.shared();
    let executable = host
        .downloads_dir()
        .join("runtimes/whisper-cpp/whisper-cli.exe");
    std::fs::create_dir_all(executable.parent().expect("parent")).expect("creates");
    std::fs::write(&executable, b"").expect("writes");
    services
        .settings
        .set(
            VOICE_ENGINE_SETTING_KEY,
            &json!({ "executablePath": executable.to_string_lossy() }),
        )
        .expect("stores the engine");
    let model = host.downloads_dir().join("models/ggml-small-q8_0.bin");
    std::fs::create_dir_all(model.parent().expect("parent")).expect("creates");
    std::fs::write(&model, b"model").expect("writes");
    services
        .settings
        .set(VOICE_MODEL_SETTING_KEY, &json!(SMALL))
        .expect("selects the model");
}

#[test]
fn captured_devices_are_cleaned_up_for_the_contract() {
    let long = "x".repeat(300);
    let devices = recording_devices(vec![
        device("", "Ghost", false),
        device("a", "  Realtek  ", true),
        device("b", "", true),
        device("c", &long, false),
    ]);
    let names: Vec<&str> = devices.iter().map(|item| item.name.as_str()).collect();
    assert_eq!(names[..2], ["Realtek", UNNAMED_DEVICE]);
    assert_eq!(names[2].len(), 200);
    assert_eq!(
        devices
            .iter()
            .map(|item| item.is_default)
            .collect::<Vec<_>>(),
        [true, false, false]
    );
}

#[tokio::test]
async fn a_device_is_chosen_persisted_and_reset_to_the_system_default() {
    let (host, server) = registered();
    host.audio.set(vec![
        device("a", "Realtek", true),
        device("b", "USB", false),
    ]);

    let listed = data(server.dispatch("voice.devices", Value::Null).await);
    assert_eq!(listed["devices"].as_array().map(Vec::len), Some(2));
    assert_eq!(listed["selectedId"], Value::Null);

    let chosen = data(
        server
            .dispatch("voice.select", json!({ "deviceId": "b" }))
            .await,
    );
    assert_eq!(chosen["selectedId"], json!("b"));
    let stored = host
        .shared()
        .settings
        .get(VOICE_DEVICE_SETTING_KEY)
        .expect("reads");
    assert_eq!(stored.map(|setting| setting.value), Some(json!("b")));

    host.audio.set(vec![device("a", "Realtek", true)]);
    let unplugged = data(server.dispatch("voice.devices", Value::Null).await);
    assert_eq!(unplugged["selectedId"], json!("b"));

    let reset = data(
        server
            .dispatch("voice.select", json!({ "deviceId": null }))
            .await,
    );
    assert_eq!(reset["selectedId"], Value::Null);
}

#[tokio::test]
async fn an_absent_device_cannot_be_chosen_and_a_probe_failure_is_native() {
    let (host, server) = registered();
    host.audio.set(vec![device("a", "Realtek", true)]);
    let missing = server
        .dispatch("voice.select", json!({ "deviceId": "z" }))
        .await;
    assert_eq!(code(&missing), "NOT_FOUND");

    host.audio.failing.store(true, Ordering::SeqCst);
    let failed = server.dispatch("voice.devices", Value::Null).await;
    assert_eq!(code(&failed), "NATIVE_ERROR");
}

#[tokio::test]
async fn the_model_list_recommends_one_model_and_nothing_is_installed_yet() {
    let (_host, server) = registered();
    let models = data(server.dispatch("voice.models", Value::Null).await);
    assert_eq!(
        models["models"].as_array().map(Vec::len),
        Some(SPEECH_MODELS.len())
    );
    assert_eq!(models["recommendedRef"], json!(SMALL));
    assert_eq!(models["selectedRef"], Value::Null);
    assert_eq!(models["engineState"], json!("available"));
    assert_eq!(model_state(&models, SMALL), &json!("available"));
}

#[tokio::test]
async fn installing_a_model_queues_the_engine_and_the_model_once() {
    let (host, server) = registered();
    let models = data(
        server
            .dispatch("voice.installModel", json!({ "modelRef": SMALL }))
            .await,
    );
    assert_ne!(model_state(&models, SMALL), &json!("available"));
    assert_ne!(models["engineState"], json!("available"));

    let again = data(
        server
            .dispatch("voice.installModel", json!({ "modelRef": SMALL }))
            .await,
    );
    assert_ne!(model_state(&again, SMALL), &json!("available"));
    let rows = host
        .shared()
        .downloads
        .list(&zvs_contract::DownloadListFilter::default())
        .expect("lists");
    assert_eq!(rows.len(), 2);

    let unknown = server
        .dispatch(
            "voice.installModel",
            json!({ "modelRef": "curated:speech:nope" }),
        )
        .await;
    assert_eq!(code(&unknown), "NOT_FOUND");
}

#[tokio::test]
async fn a_landed_engine_is_unpacked_and_the_first_model_is_selected() {
    let (host, _server) = registered();
    let services = host.shared();
    let archive = host
        .downloads_dir()
        .join("runtimes/whisper-blas-bin-x64.zip");
    std::fs::create_dir_all(archive.parent().expect("parent")).expect("creates");
    std::fs::write(&archive, zip_with("Release/whisper-cli.exe", b"MZ")).expect("writes");
    services
        .voice
        .install(&landed_row(DownloadItemKind::Runtime, ENGINE_REF, &archive))
        .await
        .expect("unpacks");
    assert!(!archive.exists());
    let engine = services
        .settings
        .get(VOICE_ENGINE_SETTING_KEY)
        .expect("reads")
        .expect("stored");
    let executable = engine.value["executablePath"].as_str().expect("a path");
    assert!(executable.ends_with("whisper-cli.exe"));
    assert!(Path::new(executable).is_file());

    let model = host.downloads_dir().join("models/ggml-small-q8_0.bin");
    services
        .voice
        .install(&landed_row(DownloadItemKind::Model, SMALL, &model))
        .await
        .expect("adopts");
    let selected = services
        .settings
        .get(VOICE_MODEL_SETTING_KEY)
        .expect("reads")
        .map(|setting| setting.value);
    assert_eq!(selected, Some(json!(SMALL)));
}

#[tokio::test]
async fn an_engine_archive_without_the_cli_fails_the_install() {
    let (host, _server) = registered();
    let archive = host
        .downloads_dir()
        .join("runtimes/whisper-blas-bin-x64.zip");
    std::fs::create_dir_all(archive.parent().expect("parent")).expect("creates");
    std::fs::write(&archive, zip_with("Release/readme.txt", b"hi")).expect("writes");
    let failed = host
        .shared()
        .voice
        .install(&landed_row(DownloadItemKind::Runtime, ENGINE_REF, &archive))
        .await
        .expect_err("no cli");
    assert_eq!(failed.code, zvs_contract::AppErrorCode::UnsupportedFormat);
}

#[tokio::test]
async fn a_dictation_is_recorded_recognised_and_its_clip_removed() {
    let (host, server) = registered();
    install_engine_and_model(&host);
    host.audio.set(vec![device("a", "Realtek", true)]);
    host.audio.speak(vec![100; SAMPLE_RATE as usize * 2]);
    host.recognizer.answer("Напиши письмо коллеге.");

    data(server.dispatch("voice.startRecording", Value::Null).await);
    let busy = server.dispatch("voice.startRecording", Value::Null).await;
    assert_eq!(code(&busy), "CONFLICT");
    let transcript = data(server.dispatch("voice.stopRecording", Value::Null).await);
    assert_eq!(transcript["text"], json!("Напиши письмо коллеге."));

    let requests = host.recognizer.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].model.ends_with("ggml-small-q8_0.bin"));
    assert!(!requests[0].audio.exists());
    assert!(requests[0].threads >= 1);

    let idle = server.dispatch("voice.stopRecording", Value::Null).await;
    assert_eq!(code(&idle), "CONFLICT");
}

#[tokio::test]
async fn a_short_clip_is_empty_and_a_cancelled_one_is_never_recognised() {
    let (host, server) = registered();
    install_engine_and_model(&host);
    host.audio.speak(vec![0; 100]);
    data(server.dispatch("voice.startRecording", Value::Null).await);
    let transcript = data(server.dispatch("voice.stopRecording", Value::Null).await);
    assert_eq!(transcript["text"], json!(""));

    host.audio.speak(vec![0; SAMPLE_RATE as usize]);
    data(server.dispatch("voice.startRecording", Value::Null).await);
    data(server.dispatch("voice.cancelRecording", Value::Null).await);
    assert!(host.recognizer.requests().is_empty());
}

#[tokio::test]
async fn recording_needs_an_installed_model_and_falls_back_to_the_default_device() {
    let (host, server) = registered();
    let unready = server.dispatch("voice.startRecording", Value::Null).await;
    assert_eq!(code(&unready), "CONFLICT");

    install_engine_and_model(&host);
    host.shared()
        .settings
        .set(VOICE_DEVICE_SETTING_KEY, &json!("gone"))
        .expect("stores");
    host.audio.set(vec![device("a", "Realtek", true)]);
    data(server.dispatch("voice.startRecording", Value::Null).await);
    data(server.dispatch("voice.cancelRecording", Value::Null).await);
    assert_eq!(host.audio.recorded(), [None]);

    host.audio.record_failing.store(true, Ordering::SeqCst);
    let failed = server.dispatch("voice.startRecording", Value::Null).await;
    assert_eq!(code(&failed), "NATIVE_ERROR");
}

#[tokio::test]
async fn only_an_installed_model_can_be_selected() {
    let (host, server) = registered();
    let missing = server
        .dispatch("voice.selectModel", json!({ "modelRef": SMALL }))
        .await;
    assert_eq!(code(&missing), "CONFLICT");

    install_engine_and_model(&host);
    let cleared = data(
        server
            .dispatch("voice.selectModel", json!({ "modelRef": null }))
            .await,
    );
    assert_eq!(cleared["selectedRef"], Value::Null);
    let chosen = data(
        server
            .dispatch("voice.selectModel", json!({ "modelRef": SMALL }))
            .await,
    );
    assert_eq!(chosen["selectedRef"], json!(SMALL));
}
