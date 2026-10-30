use std::collections::BTreeMap;
use std::path::Path;

const REQUIRED_APP_ENV: [&str; 2] = ["ZVS_CLI_VERSION", "ZVS_CLI_CHANNEL"];

fn read_app_env() -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app.env");
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

fn main() {
    for (key, value) in read_app_env() {
        println!("cargo:rustc-env={key}={value}");
    }
}
