use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use zvs_contract::{
    AppErrorCode, DeviceGpuDto, DeviceProfileDto, DeviceTier, HostEvent, Json, Page, PageRequest,
    ProviderId, ResourceSampleDto, ResultEnvelope, RunOutcomeDto, SettingsGetInput,
    SettingsGetOutput, SettingsSetInput, SettingsSetOutput, Sort, SortDirection, StreamId,
    SystemDemoJobInput, SystemDemoStreamInput, SystemDemoStreamOutput, SystemDeviceInput,
    SystemNativePingInput, SystemNativePingOutput, SystemPingInput, SystemPingOutput, Validate,
};

type Check = fn(Value) -> Result<Value, String>;

const TABLE: &[(&str, Check)] = &[
    ("primitives/ProviderId", round_trip::<ProviderId>),
    ("primitives/StreamId", round_trip::<StreamId>),
    ("primitives/PageRequest", round_trip::<PageRequest>),
    ("primitives/Page", round_trip::<Page<Json>>),
    ("primitives/Sort", round_trip::<Sort<String>>),
    ("primitives/SortDirection", round_trip::<SortDirection>),
    ("errors/AppErrorCode", round_trip::<AppErrorCode>),
    ("errors/ResultEnvelope", round_trip::<ResultEnvelope<Json>>),
    ("events/RunOutcomeDto", round_trip::<RunOutcomeDto>),
    ("events/HostEvent", round_trip::<HostEvent>),
    ("system/DeviceTier", round_trip::<DeviceTier>),
    ("system/DeviceGpuDto", round_trip::<DeviceGpuDto>),
    ("system/DeviceProfileDto", round_trip::<DeviceProfileDto>),
    ("system/ResourceSampleDto", round_trip::<ResourceSampleDto>),
    ("system/SystemPingInput", round_trip::<SystemPingInput>),
    ("system/SystemPingOutput", round_trip::<SystemPingOutput>),
    (
        "system/SystemDemoStreamInput",
        round_trip::<SystemDemoStreamInput>,
    ),
    (
        "system/SystemDemoStreamOutput",
        round_trip::<SystemDemoStreamOutput>,
    ),
    (
        "system/SystemDemoJobInput",
        round_trip::<SystemDemoJobInput>,
    ),
    ("system/SystemDeviceInput", round_trip::<SystemDeviceInput>),
    (
        "system/SystemNativePingInput",
        round_trip::<SystemNativePingInput>,
    ),
    (
        "system/SystemNativePingOutput",
        round_trip::<SystemNativePingOutput>,
    ),
    ("settings/SettingsGetInput", round_trip::<SettingsGetInput>),
    (
        "settings/SettingsGetOutput",
        round_trip::<SettingsGetOutput>,
    ),
    ("settings/SettingsSetInput", round_trip::<SettingsSetInput>),
    (
        "settings/SettingsSetOutput",
        round_trip::<SettingsSetOutput>,
    ),
];

fn round_trip<T: DeserializeOwned + Serialize + Validate>(value: Value) -> Result<Value, String> {
    let parsed: T = serde_json::from_value(value).map_err(|error| error.to_string())?;
    parsed.validate().map_err(|error| error.to_string())?;
    serde_json::to_value(&parsed).map_err(|error| error.to_string())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Valid,
    Invalid,
}

struct Fixture {
    name: String,
    kind: Kind,
    path: PathBuf,
}

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/shared/fixtures")
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<Fixture>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|error| panic!("{}: {error}", dir.display()));
    for entry in entries {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            collect(&path, root, out);
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("inside the fixtures root")
            .to_string_lossy()
            .replace('\\', "/");
        let (name, kind) = if let Some(name) = relative.strip_suffix(".valid.json") {
            (name.to_owned(), Kind::Valid)
        } else if let Some(name) = relative.strip_suffix(".invalid.json") {
            (name.to_owned(), Kind::Invalid)
        } else {
            panic!("{relative} is neither <Name>.valid.json nor <Name>.invalid.json");
        };
        out.push(Fixture { name, kind, path });
    }
}

fn all_fixtures() -> Vec<Fixture> {
    let root = fixtures_root();
    let mut out = Vec::new();
    collect(&root, &root, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn cases(path: &Path) -> Vec<(String, Value)> {
    let text =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let parsed: Vec<Value> =
        serde_json::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    assert!(!parsed.is_empty(), "{} has no cases", path.display());
    parsed
        .into_iter()
        .map(|case| {
            let name = case["name"]
                .as_str()
                .unwrap_or_else(|| panic!("{}: a case without a name", path.display()))
                .to_owned();
            let value = case
                .get("value")
                .cloned()
                .unwrap_or_else(|| panic!("{}: case {name} has no value", path.display()));
            (name, value)
        })
        .collect()
}

fn check_for(name: &str) -> Option<Check> {
    TABLE
        .iter()
        .find(|(entry, _)| *entry == name)
        .map(|(_, check)| *check)
}

#[test]
fn every_fixture_has_a_rust_type_and_every_type_has_a_valid_fixture() {
    let fixtures = all_fixtures();
    let mut problems = Vec::new();
    for fixture in &fixtures {
        if check_for(&fixture.name).is_none() {
            problems.push(format!(
                "{} has no entry in the Rust fixture table",
                fixture.name
            ));
        }
    }
    let valid: BTreeSet<&str> = fixtures
        .iter()
        .filter(|fixture| fixture.kind == Kind::Valid)
        .map(|fixture| fixture.name.as_str())
        .collect();
    for (name, _) in TABLE {
        if !valid.contains(name) {
            problems.push(format!(
                "{name} is in the table but has no {name}.valid.json"
            ));
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn valid_fixtures_deserialise_validate_and_round_trip_exactly() {
    let mut problems = Vec::new();
    for fixture in all_fixtures()
        .iter()
        .filter(|fixture| fixture.kind == Kind::Valid)
    {
        let Some(check) = check_for(&fixture.name) else {
            continue;
        };
        for (case, value) in cases(&fixture.path) {
            match check(value.clone()) {
                Ok(output) if output == value => {}
                Ok(output) => problems.push(format!(
                    "{} / {case}: re-serialised JSON differs\n  fixture: {value}\n  serde:   {output}",
                    fixture.name
                )),
                Err(error) => problems.push(format!("{} / {case}: rejected: {error}", fixture.name)),
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn invalid_fixtures_fail_deserialisation_or_validation() {
    let mut problems = Vec::new();
    for fixture in all_fixtures()
        .iter()
        .filter(|fixture| fixture.kind == Kind::Invalid)
    {
        let Some(check) = check_for(&fixture.name) else {
            continue;
        };
        for (case, value) in cases(&fixture.path) {
            if check(value).is_ok() {
                problems.push(format!(
                    "{} / {case}: accepted but must be rejected",
                    fixture.name
                ));
            }
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[test]
fn the_error_code_fixture_lists_every_rust_code() {
    let path = fixtures_root().join("errors/AppErrorCode.valid.json");
    let listed: Vec<Value> = cases(&path).into_iter().map(|(_, value)| value).collect();
    let ours: Vec<Value> = AppErrorCode::ALL
        .iter()
        .map(|code| Value::String(code.as_str().to_owned()))
        .collect();
    assert_eq!(listed, ours);
}

#[test]
fn every_host_event_variant_and_outcome_has_a_fixture() {
    let path = fixtures_root().join("events/HostEvent.valid.json");
    let values: Vec<Value> = cases(&path).into_iter().map(|(_, value)| value).collect();
    let types: BTreeSet<&str> = values
        .iter()
        .filter_map(|value| value["type"].as_str())
        .collect();
    for variant in ["token", "progress", "step", "log", "approval", "end"] {
        assert!(
            types.contains(variant),
            "no fixture for the {variant} event"
        );
    }
    let outcomes: BTreeSet<&str> = values
        .iter()
        .filter_map(|value| value["outcome"]["status"].as_str())
        .collect();
    for status in ["ok", "cancelled", "failed"] {
        assert!(
            outcomes.contains(status),
            "no end fixture with status {status}"
        );
    }
}
