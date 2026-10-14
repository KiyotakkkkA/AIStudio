fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "ipc_call",
        "events_subscribe",
        "browser_command",
        "browser_subscribe",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("tauri-build failed");
}
