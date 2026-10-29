use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::{Value, json};
use zvs_contract::FolderGrantScope;

use super::*;
use crate::cli::{CLI_INSTALL_FAILED, CLI_NOT_BUNDLED};
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::platform::logger::silent_logger;
use crate::platform::shell::TRAY_SETTING_KEY;
use crate::services::SystemService;
use crate::testing::{TestServices, test_services};

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

fn data(envelope: Value) -> Value {
    assert_eq!(envelope["ok"], json!(true), "{envelope}");
    envelope["data"].clone()
}

#[tokio::test]
async fn opening_a_folder_reports_whether_an_always_grant_covers_it() {
    let (host, server) = registered();
    let outside = tempfile::tempdir().expect("dir");
    let project = outside.path().join("project");
    std::fs::create_dir_all(project.join("src")).expect("creates");
    let cwd = project.join("src").to_string_lossy().into_owned();

    let session = data(server.dispatch("cli.open", json!({ "cwd": cwd })).await);
    assert_eq!(session["access"], json!("missing"));
    let folder = session["folder"].as_str().expect("folder").to_owned();

    let services = host.shared();
    services
        .folders
        .grant(&project, FolderGrantScope::Always, None)
        .expect("grants");
    let session = data(server.dispatch("cli.open", json!({ "cwd": cwd })).await);
    assert_eq!(session["access"], json!("granted"));
    assert_eq!(session["folder"], json!(folder));
}

#[tokio::test]
async fn opening_a_missing_folder_fails() {
    let (_host, server) = registered();
    let outside = tempfile::tempdir().expect("dir");
    let cwd = outside.path().join("absent").to_string_lossy().into_owned();
    let envelope = server.dispatch("cli.open", json!({ "cwd": cwd })).await;
    assert_eq!(envelope["error"]["code"], json!("NOT_FOUND"));
}

#[tokio::test]
async fn installing_puts_the_command_on_the_path_and_turns_the_tray_on() {
    let (host, server) = registered();
    let status = data(server.dispatch("cli.status", Value::Null).await);
    assert_eq!(
        status,
        json!({ "available": true, "installed": false, "command": "aistudio" })
    );

    let status = data(server.dispatch("cli.install", Value::Null).await);
    assert_eq!(status["installed"], json!(true));
    assert_eq!(host.shell.tray(), [true]);
    let tray = host
        .shared()
        .settings
        .get(TRAY_SETTING_KEY)
        .expect("reads")
        .expect("stored");
    assert_eq!(tray.value, json!(true));

    let status = data(server.dispatch("cli.uninstall", Value::Null).await);
    assert_eq!(status["installed"], json!(false));
    assert_eq!(host.shell.tray(), [true]);
}

#[tokio::test]
async fn a_failed_install_leaves_the_tray_alone() {
    let (host, server) = registered();
    host.cli.failing.store(true, Ordering::SeqCst);
    let envelope = server.dispatch("cli.install", Value::Null).await;
    assert_eq!(envelope["error"]["code"], json!("NATIVE_ERROR"));
    assert_eq!(envelope["error"]["message"], json!(CLI_INSTALL_FAILED));
    assert!(host.shell.tray().is_empty());
}

#[tokio::test]
async fn a_build_without_the_cli_cannot_install_it() {
    let (host, server) = registered();
    host.cli.available.store(false, Ordering::SeqCst);
    host.cli.installed.store(true, Ordering::SeqCst);
    let status = data(server.dispatch("cli.status", Value::Null).await);
    assert_eq!(status["installed"], json!(false));
    let envelope = server.dispatch("cli.install", Value::Null).await;
    assert_eq!(envelope["error"]["message"], json!(CLI_NOT_BUNDLED));
}
