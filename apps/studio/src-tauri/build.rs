use std::collections::BTreeMap;
use std::env;
use std::path::Path;

const REQUIRED_APP_ENV: [&str; 2] = ["ZVS_APP_VERSION", "ZVS_APP_CHANNEL"];

fn read_app_env() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../app.env");
    println!("cargo:rerun-if-changed={}", path.display());
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    let mut values = BTreeMap::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .filter(|(key, _)| {
                key.starts_with(|c: char| c.is_ascii_uppercase())
                    && key
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            })
            .unwrap_or_else(|| panic!("app.env:{}: expected KEY=value", index + 1));
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|inner| inner.strip_suffix('"'))
            .unwrap_or(value);
        values.insert(key.to_owned(), value.to_owned());
    }
    for key in REQUIRED_APP_ENV {
        if values.get(key).is_none_or(String::is_empty) {
            panic!("app.env is missing {key}");
        }
    }
    values
}

#[allow(unsafe_code)]
fn apply_app_env(values: &BTreeMap<String, String>) {
    for (key, value) in values {
        println!("cargo:rustc-env={key}={value}");
    }
    let mut overlay = match env::var("TAURI_CONFIG") {
        Ok(raw) => serde_json::from_str(&raw).expect("TAURI_CONFIG is not valid JSON"),
        Err(_) => serde_json::json!({}),
    };
    overlay["version"] = serde_json::json!(values["ZVS_APP_VERSION"]);
    let overlay = overlay.to_string();
    unsafe { env::set_var("TAURI_CONFIG", &overlay) };
    println!("cargo:rustc-env=TAURI_CONFIG={overlay}");
}

fn main() {
    apply_app_env(&read_app_env());
    let manifest = tauri_build::AppManifest::new().commands(&[
        "ipc_call",
        "events_subscribe",
        "browser_command",
        "browser_subscribe",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("tauri-build failed");
    println!("cargo:rerun-if-changed=icons");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-test-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }
}
