use std::time::Duration;

use super::*;
use crate::testing::temporary_directory;

#[test]
fn the_runtime_follows_the_extension() {
    assert_eq!(
        ScriptRuntime::of("scripts/a.py"),
        Some(ScriptRuntime::Python)
    );
    for path in ["scripts/a.js", "scripts/a.MJS", "scripts/a.cjs"] {
        assert_eq!(ScriptRuntime::of(path), Some(ScriptRuntime::Node), "{path}");
    }
    for path in ["scripts/a.sh", "scripts/a.ps1", "scripts/a"] {
        assert_eq!(ScriptRuntime::of(path), None, "{path}");
    }
    assert_eq!(
        ScriptRuntime::Python.args("/s/a.py", &["-v".to_owned()]),
        ["run", "/s/a.py", "-v"]
    );
    assert_eq!(ScriptRuntime::Node.args("/s/a.js", &[]), ["/s/a.js"]);
}

#[test]
fn old_scratch_folders_are_pruned() {
    let root = temporary_directory("studio-skill-scratch-");
    for index in 0..SCRATCH_KEEP + 4 {
        std::fs::create_dir_all(root.path().join(format!("old-{index:02}"))).expect("a folder");
        std::thread::sleep(Duration::from_millis(15));
    }
    let scratch = prepare_scratch(root.path(), "current").expect("prepares");
    assert!(scratch.is_dir());
    let left: Vec<String> = std::fs::read_dir(root.path())
        .expect("lists")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left.len(), SCRATCH_KEEP);
    assert!(left.contains(&"current".to_owned()));
    assert!(!left.contains(&"old-00".to_owned()));
    assert!(left.contains(&format!("old-{:02}", SCRATCH_KEEP + 3)));
}

#[test]
fn a_pinned_scratch_folder_survives_pruning_until_it_is_released() {
    let root = temporary_directory("studio-skill-pinned-");
    let pinned = root.path().join("background");
    std::fs::create_dir_all(&pinned).expect("a folder");
    let pin = crate::toolchain::process::pin_scratch(&pinned);
    std::thread::sleep(Duration::from_millis(15));
    for index in 0..SCRATCH_KEEP + 2 {
        std::fs::create_dir_all(root.path().join(format!("new-{index:02}"))).expect("a folder");
        std::thread::sleep(Duration::from_millis(15));
    }
    prepare_scratch(root.path(), "current").expect("prepares");
    assert!(pinned.is_dir());
    drop(pin);
    prepare_scratch(root.path(), "current").expect("prepares");
    assert!(!pinned.exists());
}
