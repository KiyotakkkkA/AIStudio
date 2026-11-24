use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::rewrite::{PathRewrite, rewrite_database};
use super::transfer::{Mode, Transfer, apply, plan, verify};
use super::{BootError, LegacySources, StepContext, StepProgress};
use crate::platform::paths::{DATABASE_FILE, PathEnvironment, StudioPaths, nested};

pub const LEGACY_LABEL: &str = "Перенос данных приложения";
pub const SHELL_PROFILE: &str = "EBWebView";
const MIGRATING_SUFFIX: &str = ".migrating";

struct Route {
    from: &'static [&'static str],
    to: fn(&StudioPaths) -> PathBuf,
    mode: Option<Mode>,
    skip: &'static [&'static [&'static str]],
}

const fn route(
    from: &'static [&'static str],
    to: fn(&StudioPaths) -> PathBuf,
    mode: Option<Mode>,
) -> Route {
    Route {
        from,
        to,
        mode,
        skip: &[],
    }
}

fn routes() -> Vec<Route> {
    vec![
        route(
            &["conversations"],
            |paths| paths.conversations_dir.clone(),
            Some(Mode::Copy),
        ),
        route(
            &["scenario-files"],
            |paths| paths.scenarios_dir.clone(),
            Some(Mode::Copy),
        ),
        route(
            &["skills"],
            |paths| paths.skills_dir.clone(),
            Some(Mode::Copy),
        ),
        route(
            &["dashboards"],
            |paths| paths.dashboards_dir.clone(),
            Some(Mode::Copy),
        ),
        route(
            &["policy"],
            |paths| paths.policy_dir.clone(),
            Some(Mode::Copy),
        ),
        route(
            &["cli"],
            |paths| paths.cli_settings_dir.clone(),
            Some(Mode::Copy),
        ),
        route(
            &["downloads", "models"],
            |paths| paths.assets_dir.join("models"),
            Some(Mode::Link),
        ),
        route(
            &["downloads", "embeddings"],
            |paths| paths.assets_dir.join("embeddings"),
            Some(Mode::Link),
        ),
        route(
            &["downloads", "runtimes"],
            |paths| paths.assets_dir.join("runtimes"),
            Some(Mode::Link),
        ),
        route(
            &["downloads", "mcp"],
            |paths| nested(&paths.assets_dir, &["packages", "mcp"]),
            Some(Mode::Link),
        ),
        route(
            &["downloads", "skills"],
            |paths| nested(&paths.assets_dir, &["packages", "skills"]),
            Some(Mode::Link),
        ),
        route(
            &["mcp-toolchain", "npm-cache"],
            |paths| paths.package_cache_dir.join("npm"),
            None,
        ),
        route(
            &["mcp-toolchain", "pip-cache"],
            |paths| paths.package_cache_dir.join("pip"),
            None,
        ),
        route(
            &["mcp-toolchain", "uv", "cache"],
            |paths| paths.package_cache_dir.join("uv"),
            None,
        ),
        Route {
            from: &["mcp-toolchain"],
            to: |paths| paths.toolchain_dir.clone(),
            mode: Some(Mode::Link),
            skip: &[
                &["npm-cache"],
                &["pip-cache"],
                &["uv", "cache"],
                &["uv", "tools"],
                &["uv", "bin"],
            ],
        },
        route(
            &["computer"],
            |paths| paths.computer_dir.clone(),
            Some(Mode::Copy),
        ),
        route(
            &["browser", "extensions-off"],
            |paths| paths.extensions_dir.join("disabled"),
            Some(Mode::Copy),
        ),
        route(
            &["browser", "extensions"],
            |paths| paths.extensions_dir.join("enabled"),
            Some(Mode::Copy),
        ),
        route(
            &["browser", "work"],
            |paths| paths.browser_profile_dir.clone(),
            Some(Mode::Copy),
        ),
        route(
            &["vectors"],
            |paths| paths.vector_stores_dir.clone(),
            Some(Mode::Copy),
        ),
        route(
            &["cache", "code-runs"],
            |paths| paths.runs_dir.clone(),
            None,
        ),
        route(
            &["cache", "gallery"],
            |paths| paths.thumbnails_dir.clone(),
            None,
        ),
        route(&["cache"], |paths| paths.cache_dir.clone(), None),
        route(&["logs"], |paths| paths.logs_dir.clone(), Some(Mode::Copy)),
        route(
            &["backups"],
            |paths| paths.backups_dir.clone(),
            Some(Mode::Link),
        ),
    ]
}

pub(super) fn layout(root: &Path) -> StudioPaths {
    StudioPaths::resolve(&PathEnvironment {
        state_dir: root.to_path_buf(),
        resource_dir: PathBuf::new(),
        executable_dir: PathBuf::new(),
        packaged: false,
    })
}

pub fn path_rewrites(root: &Path, legacy: &Path) -> Vec<PathRewrite> {
    let paths = layout(root);
    let mut rewrites: Vec<PathRewrite> = routes()
        .iter()
        .map(|route| PathRewrite::new(nested(legacy, route.from), (route.to)(&paths)))
        .collect();
    rewrites.push(PathRewrite::new(
        legacy.join(DATABASE_FILE),
        paths.db_path.clone(),
    ));
    rewrites
}

pub fn transfers(root: &Path, legacy: &LegacySources) -> Vec<Transfer> {
    let paths = layout(root);
    let mut transfers: Vec<Transfer> = routes()
        .iter()
        .filter_map(|route| {
            route.mode.map(|mode| {
                Transfer::new(
                    nested(&legacy.roaming, route.from),
                    (route.to)(&paths),
                    mode,
                )
                .skipping(route.skip)
            })
        })
        .collect();
    if let Some(local) = &legacy.local {
        transfers.push(Transfer::new(
            local.join(SHELL_PROFILE),
            paths.shell_profile_dir.join(SHELL_PROFILE),
            Mode::Copy,
        ));
    }
    transfers
}

fn sibling(file: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", file.to_string_lossy()))
}

fn io(context: &Path, error: &std::io::Error) -> BootError {
    BootError::Io(format!("{}: {error}", context.display()))
}

fn database(error: &rusqlite::Error) -> BootError {
    BootError::Io(error.to_string())
}

fn migrate_database(root: &Path, legacy: &Path) -> Result<(), BootError> {
    let paths = layout(root);
    let target = paths.db_path.clone();
    if target.is_file() {
        return Ok(());
    }
    let source = legacy.join(DATABASE_FILE);
    if !source.is_file() {
        return Ok(());
    }
    let staged = sibling(&target, MIGRATING_SUFFIX);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| io(parent, &error))?;
    }
    for suffix in ["", "-wal", "-shm"] {
        let from = sibling(&source, suffix);
        let to = sibling(&staged, suffix);
        let _ = std::fs::remove_file(&to);
        if from.is_file() {
            std::fs::copy(&from, &to).map_err(|error| io(&from, &error))?;
        }
    }
    {
        let connection = Connection::open(&staged).map_err(|error| database(&error))?;
        rewrite_database(
            &connection,
            legacy,
            &path_rewrites(root, legacy),
            &paths.stored_paths(),
        )
        .map_err(|error| database(&error))?;
        connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .map_err(|error| database(&error))?;
    }
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(sibling(&staged, suffix));
    }
    std::fs::rename(&staged, &target).map_err(|error| io(&target, &error))
}

pub fn migrate_legacy(
    context: &StepContext<'_>,
    progress: &dyn Fn(StepProgress),
) -> Result<(), BootError> {
    let Some(legacy) = context.legacy else {
        return Ok(());
    };
    let planned = plan(&transfers(context.root, legacy))?;
    let total: u64 = planned.iter().map(|file| file.size).sum();
    let mut done = 0;
    progress(StepProgress { done, total });
    for file in &planned {
        apply(file)?;
        done += file.size;
        progress(StepProgress { done, total });
    }
    verify(&planned)?;
    migrate_database(context.root, &legacy.roaming)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot::LegacySources;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("creates");
        std::fs::write(path, text).expect("writes");
    }

    fn legacy_tree(roaming: &Path, local: &Path) {
        write(&roaming.join("conversations/c1/work/a.docx"), "docx");
        write(&roaming.join("downloads/models/m.gguf"), "model");
        write(
            &roaming.join("downloads/runtimes/llama/llama-server.exe"),
            "exe",
        );
        write(&roaming.join("mcp-toolchain/node/26/node.exe"), "node");
        write(&roaming.join("mcp-toolchain/uv/cache/blob"), "cache");
        write(
            &roaming.join("mcp-toolchain/uv/tools/ruff/pyvenv.cfg"),
            "venv",
        );
        write(&roaming.join("mcp-toolchain/uv/python/py.exe"), "py");
        write(&roaming.join("browser/work/Cookies"), "cookies");
        write(
            &roaming.join("browser/extensions/ublock/manifest.json"),
            "{}",
        );
        write(&roaming.join("cli/theme.json"), "{}");
        write(&roaming.join("cache/code-runs/r1/out.txt"), "out");
        write(&roaming.join("shell/notification.png"), "png");
        write(&roaming.join("backups/db-1.sqlite"), "backup");
        write(&local.join("EBWebView/Default/Preferences"), "prefs");
        let connection = Connection::open(roaming.join(DATABASE_FILE)).expect("opens");
        connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 CREATE TABLE download (id TEXT PRIMARY KEY, target_path TEXT NOT NULL);
                 CREATE TABLE skill (id TEXT PRIMARY KEY, path TEXT NOT NULL);
                 CREATE TABLE setting (key TEXT PRIMARY KEY, value TEXT);",
            )
            .expect("creates");
        connection
            .execute(
                "INSERT INTO download VALUES ('d1', ?1)",
                [roaming
                    .join("downloads")
                    .join("models")
                    .join("m.gguf")
                    .to_string_lossy()],
            )
            .expect("inserts");
        connection
            .execute(
                "INSERT INTO setting VALUES ('runtimes.installed', ?1)",
                [serde_json::json!({
                    "llama": {
                        "installPath": roaming.join("downloads").join("runtimes").join("llama")
                    }
                })
                .to_string()],
            )
            .expect("inserts");
    }

    #[test]
    fn the_legacy_root_is_copied_into_the_new_tree_and_left_in_place() {
        let home = tempfile::tempdir().expect("a folder");
        let roaming = home.path().join("Roaming").join("com.zvs.aistudio");
        let local = home.path().join("Local").join("com.zvs.aistudio");
        legacy_tree(&roaming, &local);
        let root = home.path().join("Local").join("ZVS").join("AIStudio");
        let legacy = LegacySources {
            roaming: roaming.clone(),
            local: Some(local.clone()),
        };
        let context = StepContext {
            root: &root,
            legacy: Some(&legacy),
        };
        let seen = std::sync::Mutex::new(Vec::new());

        migrate_legacy(&context, &|step| {
            seen.lock().expect("locks").push(step);
        })
        .expect("migrates");

        let paths = layout(&root);
        assert!(paths.conversations_dir.join("c1/work/a.docx").is_file());
        assert!(paths.assets_dir.join("models/m.gguf").is_file());
        assert!(
            paths
                .assets_dir
                .join("runtimes/llama/llama-server.exe")
                .is_file()
        );
        assert!(paths.toolchain_dir.join("node/26/node.exe").is_file());
        assert!(paths.toolchain_dir.join("uv/python/py.exe").is_file());
        assert!(!paths.toolchain_dir.join("uv/cache").exists());
        assert!(!paths.toolchain_dir.join("uv/tools").exists());
        assert!(paths.browser_profile_dir.join("Cookies").is_file());
        assert!(
            paths
                .extensions_dir
                .join("enabled/ublock/manifest.json")
                .is_file()
        );
        assert!(paths.cli_settings_dir.join("theme.json").is_file());
        assert!(paths.backups_dir.join("db-1.sqlite").is_file());
        assert!(
            paths
                .shell_profile_dir
                .join("EBWebView/Default/Preferences")
                .is_file()
        );
        assert!(!paths.runs_dir.exists());
        assert!(!paths.cache_dir.exists());
        assert!(!root.join("shell").exists());
        assert!(roaming.join("downloads/models/m.gguf").is_file());
        assert!(roaming.join(DATABASE_FILE).is_file());

        let steps = seen.into_inner().expect("unlocks");
        let last = steps.last().expect("progress");
        assert_eq!(last.done, last.total);
        assert!(last.total > 0);

        let connection = Connection::open(&paths.db_path).expect("opens");
        let target: String = connection
            .query_row("SELECT target_path FROM download", [], |row| row.get(0))
            .expect("reads");
        assert_eq!(target, "assets:models/m.gguf");
        let installed: String = connection
            .query_row("SELECT value FROM setting", [], |row| row.get(0))
            .expect("reads");
        let value: serde_json::Value = serde_json::from_str(&installed).expect("json");
        assert_eq!(
            value["llama"]["installPath"],
            serde_json::json!(paths.assets_dir.join("runtimes").join("llama"))
        );
        assert!(!sibling(&paths.db_path, MIGRATING_SUFFIX).exists());

        migrate_legacy(&context, &|_| {}).expect("a second run changes nothing");
        let again: String = connection
            .query_row("SELECT target_path FROM download", [], |row| row.get(0))
            .expect("reads");
        assert_eq!(again, "assets:models/m.gguf");
    }
}
