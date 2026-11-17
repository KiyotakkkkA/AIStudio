use std::path::{Path, PathBuf};

use zvs_contract::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathEnvironment {
    pub state_dir: PathBuf,
    pub resource_dir: PathBuf,
    pub executable_dir: PathBuf,
    pub packaged: bool,
}

pub const DATA_DIR: &str = "data";
pub const ASSETS_DIR: &str = "assets";
pub const INDEXES_DIR: &str = "indexes";
pub const PROFILES_DIR: &str = "profiles";
pub const CACHE_DIR: &str = "cache";
pub const TEMP_DIR: &str = "temp";
pub const LOGS_DIR: &str = "logs";
pub const BACKUPS_DIR: &str = "backups";
pub const DATABASE_FILE: &str = "studio.sqlite";
pub const NOTIFICATION_ICON: [&str; 2] = ["resources", "notification.png"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudioPaths {
    pub root: PathBuf,
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub conversations_dir: PathBuf,
    pub scenarios_dir: PathBuf,
    pub imports_dir: PathBuf,
    pub transfer_journals_dir: PathBuf,
    pub skills_dir: PathBuf,
    pub dashboards_dir: PathBuf,
    pub policy_dir: PathBuf,
    pub cli_settings_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub toolchain_dir: PathBuf,
    pub computer_dir: PathBuf,
    pub extensions_dir: PathBuf,
    pub indexes_dir: PathBuf,
    pub vector_stores_dir: PathBuf,
    pub profiles_dir: PathBuf,
    pub shell_profile_dir: PathBuf,
    pub browser_profile_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub documents_cache_dir: PathBuf,
    pub thumbnails_dir: PathBuf,
    pub favicons_dir: PathBuf,
    pub capture_dir: PathBuf,
    pub package_cache_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub temp_downloads_dir: PathBuf,
    pub staging_dir: PathBuf,
    pub runs_dir: PathBuf,
    pub boot_profile_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub streams_dir: PathBuf,
    pub backups_dir: PathBuf,
    pub resources_dir: PathBuf,
    pub notification_icon: PathBuf,
    pub sidecar_path: PathBuf,
    pub cli_path: PathBuf,
    pub packaged: bool,
}

pub const SIDECAR_NAME: &str = "zvs-jobd";
pub const CLI_FOLDER: &str = "cli";

pub fn cli_path(executable_dir: &Path, packaged: bool) -> PathBuf {
    let name = format!("{}.exe", zvs_contract::CLI_COMMAND);
    if packaged {
        executable_dir.join(CLI_FOLDER).join(name)
    } else {
        executable_dir.join(name)
    }
}

pub fn sidecar_file_name(windows: bool) -> String {
    if windows {
        format!("{SIDECAR_NAME}.exe")
    } else {
        SIDECAR_NAME.to_owned()
    }
}

pub fn sidecar_path(executable_dir: &Path, windows: bool) -> PathBuf {
    executable_dir.join(sidecar_file_name(windows))
}

pub fn nested(root: &Path, parts: &[&str]) -> PathBuf {
    parts
        .iter()
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

impl StudioPaths {
    pub fn resolve(env: &PathEnvironment) -> Self {
        let root = &env.state_dir;
        let data = root.join(DATA_DIR);
        let assets = root.join(ASSETS_DIR);
        let indexes = root.join(INDEXES_DIR);
        let profiles = root.join(PROFILES_DIR);
        let cache = root.join(CACHE_DIR);
        let temp = root.join(TEMP_DIR);
        let logs = root.join(LOGS_DIR);
        Self {
            root: root.clone(),
            db_path: data.join(DATABASE_FILE),
            conversations_dir: data.join(crate::conversations::CONVERSATIONS_DIR),
            scenarios_dir: data.join("scenarios"),
            imports_dir: data.join("imports"),
            transfer_journals_dir: data.join("transfer-journals"),
            skills_dir: nested(&data, &["library", "skills"]),
            dashboards_dir: nested(&data, &["library", "dashboards"]),
            policy_dir: nested(&data, &["library", "policy"]),
            cli_settings_dir: zvs_contract::layout::cli_settings_dir(root),
            data_dir: data,
            toolchain_dir: assets.join("toolchain"),
            computer_dir: nested(&assets, &["environments", "computer"]),
            extensions_dir: assets.join("extensions"),
            assets_dir: assets,
            vector_stores_dir: indexes.join("vectors"),
            indexes_dir: indexes,
            shell_profile_dir: profiles.join("shell"),
            browser_profile_dir: nested(&profiles, &crate::browser::PROFILE_DIR),
            profiles_dir: profiles,
            documents_cache_dir: cache.join("documents"),
            thumbnails_dir: cache.join("thumbnails"),
            favicons_dir: cache.join("favicons"),
            capture_dir: cache.join("capture"),
            package_cache_dir: cache.join("packages"),
            cache_dir: cache,
            temp_downloads_dir: temp.join("downloads"),
            staging_dir: temp.join("staging"),
            runs_dir: temp.join("runs"),
            boot_profile_dir: temp.join("boot"),
            temp_dir: temp,
            streams_dir: logs.join("streams"),
            logs_dir: logs,
            backups_dir: root.join(BACKUPS_DIR),
            resources_dir: env.resource_dir.clone(),
            notification_icon: nested(&env.resource_dir, &NOTIFICATION_ICON),
            sidecar_path: sidecar_path(&env.executable_dir, cfg!(windows)),
            cli_path: cli_path(&env.executable_dir, env.packaged),
            packaged: env.packaged,
        }
    }

    pub fn download_dir(&self, kind: DownloadKind) -> PathBuf {
        kind.dir(&self.assets_dir)
    }

    pub fn stored_paths(&self) -> StoredPaths {
        StoredPaths::new(vec![
            (StoredRoot::Data, self.data_dir.clone()),
            (StoredRoot::Assets, self.assets_dir.clone()),
            (StoredRoot::Indexes, self.indexes_dir.clone()),
        ])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoredRoot {
    Data,
    Assets,
    Indexes,
}

impl StoredRoot {
    pub const ALL: [Self; 3] = [Self::Data, Self::Assets, Self::Indexes];

    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Data => "data:",
            Self::Assets => "assets:",
            Self::Indexes => "indexes:",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredPaths {
    roots: Vec<(StoredRoot, PathBuf)>,
}

impl StoredPaths {
    pub fn new(roots: Vec<(StoredRoot, PathBuf)>) -> Self {
        Self { roots }
    }

    pub fn encode(&self, path: &Path) -> String {
        for (root, base) in &self.roots {
            if let Ok(relative) = path.strip_prefix(base) {
                let parts: Vec<String> = relative
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect();
                return format!("{}{}", root.prefix(), parts.join("/"));
            }
        }
        path.to_string_lossy().into_owned()
    }

    pub fn decode(&self, stored: &str) -> PathBuf {
        for (root, base) in &self.roots {
            if let Some(relative) = stored.strip_prefix(root.prefix()) {
                return relative
                    .split('/')
                    .filter(|part| !part.is_empty())
                    .fold(base.clone(), |path, part| path.join(part));
            }
        }
        PathBuf::from(stored)
    }

    pub fn decode_text(&self, stored: &str) -> String {
        self.decode(stored).to_string_lossy().into_owned()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DownloadKind {
    Model,
    Embedding,
    Runtime,
    Mcp,
    Skill,
}

impl DownloadKind {
    pub const ALL: [Self; 5] = [
        Self::Model,
        Self::Embedding,
        Self::Runtime,
        Self::Mcp,
        Self::Skill,
    ];

    pub fn parse(kind: &str) -> Option<Self> {
        match kind {
            "model" => Some(Self::Model),
            "embedding" => Some(Self::Embedding),
            "runtime" => Some(Self::Runtime),
            "mcp" => Some(Self::Mcp),
            "skill" => Some(Self::Skill),
            _ => None,
        }
    }

    pub const fn folders(self) -> &'static [&'static str] {
        match self {
            Self::Model => &["models"],
            Self::Embedding => &["embeddings"],
            Self::Runtime => &["runtimes"],
            Self::Mcp => &["packages", "mcp"],
            Self::Skill => &["packages", "skills"],
        }
    }

    pub fn dir(self, assets: &Path) -> PathBuf {
        nested(assets, self.folders())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("Invalid vector store ID")]
    VectorStoreId,
    #[error("Invalid runtime id: {0}")]
    RuntimeId(String),
    #[error("Unknown download kind: {0}")]
    DownloadKind(String),
    #[error("Unsafe download file name: {0}")]
    FileName(String),
}

pub fn vector_store_path(directory: &Path, store_id: &str) -> Result<PathBuf, PathError> {
    let valid = !store_id.is_empty()
        && store_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    if valid {
        Ok(directory.join(store_id))
    } else {
        Err(PathError::VectorStoreId)
    }
}

pub fn runtime_install_path(downloads: &Path, runtime_id: &str) -> Result<PathBuf, PathError> {
    let bytes = runtime_id.as_bytes();
    let lower = |byte: &u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    let valid = (2..=63).contains(&bytes.len())
        && bytes.first().is_some_and(lower)
        && bytes[1..].iter().all(|byte| lower(byte) || *byte == b'-');
    if valid {
        Ok(DownloadKind::Runtime.dir(downloads).join(runtime_id))
    } else {
        Err(PathError::RuntimeId(runtime_id.to_owned()))
    }
}

pub fn download_target_path(
    directory: &Path,
    kind: &str,
    file_name: &str,
) -> Result<PathBuf, PathError> {
    let kind = DownloadKind::parse(kind).ok_or_else(|| PathError::DownloadKind(kind.to_owned()))?;
    let valid = !file_name.is_empty()
        && file_name != "."
        && file_name != ".."
        && file_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if valid {
        Ok(kind.dir(directory).join(file_name))
    } else {
        Err(PathError::FileName(file_name.to_owned()))
    }
}

pub fn log_file_path(directory: &Path, rotation: u32) -> PathBuf {
    if rotation == 0 {
        directory.join("studio.jsonl")
    } else {
        directory.join(format!("studio.{rotation}.jsonl"))
    }
}

pub fn stream_file_path(directory: &Path, at: Timestamp) -> PathBuf {
    directory.join(format!("{}.jsonl", utc_date(at)))
}

pub const BACKUP_PREFIX: &str = "db-";
pub const BACKUP_SUFFIX: &str = ".sqlite";

pub fn backup_file_path(directory: &Path, at: Timestamp) -> PathBuf {
    let time = at.rem_euclid(86_400_000);
    directory.join(format!(
        "{BACKUP_PREFIX}{}T{:02}-{:02}-{:02}-{:03}Z{BACKUP_SUFFIX}",
        utc_date(at),
        time / 3_600_000,
        time / 60_000 % 60,
        time / 1_000 % 60,
        time % 1_000
    ))
}

pub fn is_backup_file(name: &str) -> bool {
    name.starts_with(BACKUP_PREFIX) && name.ends_with(BACKUP_SUFFIX)
}

pub fn utc_iso(at: Timestamp) -> String {
    let time = at.rem_euclid(86_400_000);
    format!(
        "{}T{:02}:{:02}:{:02}.{:03}Z",
        utc_date(at),
        time / 3_600_000,
        time / 60_000 % 60,
        time / 1_000 % 60,
        time % 1_000
    )
}

pub fn utc_stamp(at: Timestamp) -> String {
    let time = at.rem_euclid(86_400_000);
    format!(
        "{}T{:02}-{:02}-{:02}",
        utc_date(at),
        time / 3_600_000,
        time / 60_000 % 60,
        time / 1_000 % 60
    )
}

fn utc_date(at: Timestamp) -> String {
    let days = at.div_euclid(86_400_000);
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> PathEnvironment {
        let root = std::env::temp_dir();
        PathEnvironment {
            state_dir: root.join("user"),
            resource_dir: root.join("bundle"),
            executable_dir: root.join("target").join("debug"),
            packaged: false,
        }
    }

    #[test]
    fn every_root_has_one_lifecycle_and_no_root_contains_another() {
        let env = env();
        let paths = StudioPaths::resolve(&env);
        let root = &env.state_dir;
        assert_eq!(paths.root, *root);
        assert_eq!(paths.data_dir, root.join("data"));
        assert_eq!(paths.db_path, root.join("data").join("studio.sqlite"));
        assert_eq!(
            paths.skills_dir,
            root.join("data").join("library").join("skills")
        );
        assert_eq!(
            paths.dashboards_dir,
            root.join("data").join("library").join("dashboards")
        );
        assert_eq!(
            paths.policy_dir,
            root.join("data").join("library").join("policy")
        );
        assert_eq!(paths.scenarios_dir, root.join("data").join("scenarios"));
        assert_eq!(
            paths.cli_settings_dir,
            root.join("data").join("settings").join("cli")
        );
        assert_eq!(paths.toolchain_dir, root.join("assets").join("toolchain"));
        assert_eq!(
            paths.computer_dir,
            root.join("assets").join("environments").join("computer")
        );
        assert_eq!(
            paths.vector_stores_dir,
            root.join("indexes").join("vectors")
        );
        assert_eq!(paths.shell_profile_dir, root.join("profiles").join("shell"));
        assert_eq!(
            paths.browser_profile_dir,
            root.join("profiles").join("browser").join("work")
        );
        assert_eq!(paths.package_cache_dir, root.join("cache").join("packages"));
        assert_eq!(
            paths.temp_downloads_dir,
            root.join("temp").join("downloads")
        );
        assert_eq!(paths.runs_dir, root.join("temp").join("runs"));
        assert_eq!(paths.boot_profile_dir, root.join("temp").join("boot"));
        assert!(!paths.boot_profile_dir.starts_with(&paths.profiles_dir));
        assert_eq!(paths.streams_dir, root.join("logs").join("streams"));
        assert_eq!(paths.backups_dir, root.join("backups"));
        assert_eq!(paths.resources_dir, env.resource_dir);
        assert_eq!(
            paths.notification_icon,
            env.resource_dir.join("resources").join("notification.png")
        );
        assert_eq!(
            paths.sidecar_path,
            env.executable_dir.join(sidecar_file_name(cfg!(windows)))
        );
        assert!(!paths.packaged);
        let roots = [
            &paths.data_dir,
            &paths.assets_dir,
            &paths.indexes_dir,
            &paths.profiles_dir,
            &paths.cache_dir,
            &paths.temp_dir,
            &paths.logs_dir,
            &paths.backups_dir,
        ];
        for (index, left) in roots.iter().enumerate() {
            assert_eq!(left.parent(), Some(root.as_path()));
            for right in &roots[index + 1..] {
                assert!(!left.starts_with(right) && !right.starts_with(left));
            }
        }
    }

    #[test]
    fn stored_paths_are_relative_to_their_root_and_foreign_paths_stay_absolute() {
        let paths = StudioPaths::resolve(&env());
        let stored = paths.stored_paths();
        let model = paths.assets_dir.join("models").join("qwen.gguf");
        assert_eq!(stored.encode(&model), "assets:models/qwen.gguf");
        assert_eq!(stored.decode("assets:models/qwen.gguf"), model);
        let skill = paths.skills_dir.join("pdf");
        assert_eq!(stored.encode(&skill), "data:library/skills/pdf");
        assert_eq!(stored.decode(&stored.encode(&skill)), skill);
        let store = paths.vector_stores_dir.join("s1");
        assert_eq!(stored.encode(&store), "indexes:vectors/s1");
        let foreign = std::env::temp_dir().join("elsewhere").join("a.txt");
        let text = foreign.to_string_lossy().into_owned();
        assert_eq!(stored.encode(&foreign), text);
        assert_eq!(stored.decode(&text), foreign);
        let moved = StoredPaths::new(vec![(
            StoredRoot::Assets,
            std::env::temp_dir().join("D").join("assets"),
        )]);
        assert_eq!(
            moved.decode("assets:models/qwen.gguf"),
            std::env::temp_dir()
                .join("D")
                .join("assets")
                .join("models")
                .join("qwen.gguf")
        );
        assert_eq!(
            StoredPaths::default().decode("assets:x"),
            PathBuf::from("assets:x")
        );
    }

    #[test]
    fn the_cli_has_its_own_folder_once_installed() {
        let installed = std::env::temp_dir().join("ZVS AI Studio");
        assert_eq!(
            cli_path(&installed, true),
            installed.join("cli").join("aistudio.exe")
        );
        assert_eq!(cli_path(&installed, false), installed.join("aistudio.exe"));
    }

    #[test]
    fn the_sidecar_sits_next_to_the_main_executable_in_both_layouts() {
        let root = std::env::temp_dir();
        let development = root.join("target").join("debug");
        assert_eq!(
            sidecar_path(&development, true),
            development.join("zvs-jobd.exe")
        );
        let installed = root.join("ZVS AI Studio");
        assert_eq!(sidecar_path(&installed, false), installed.join("zvs-jobd"));
        let packaged = StudioPaths::resolve(&PathEnvironment {
            executable_dir: installed.clone(),
            packaged: true,
            ..env()
        });
        assert_eq!(
            packaged.sidecar_path,
            installed.join(sidecar_file_name(cfg!(windows)))
        );
    }

    #[test]
    fn installed_assets_land_in_one_folder_per_kind() {
        let paths = StudioPaths::resolve(&env());
        let folders: Vec<PathBuf> = DownloadKind::ALL
            .iter()
            .map(|kind| paths.download_dir(*kind))
            .collect();
        let assets = &paths.assets_dir;
        let expected: Vec<PathBuf> = vec![
            assets.join("models"),
            assets.join("embeddings"),
            assets.join("runtimes"),
            assets.join("packages").join("mcp"),
            assets.join("packages").join("skills"),
        ];
        assert_eq!(folders, expected);
        for kind in DownloadKind::ALL {
            let name = format!("{kind:?}").to_lowercase();
            assert_eq!(DownloadKind::parse(&name), Some(kind));
        }
    }

    #[test]
    fn vector_store_ids_are_a_single_safe_segment() {
        let root = Path::new("vectors");
        assert_eq!(
            vector_store_path(root, "store_1-a"),
            Ok(root.join("store_1-a"))
        );
        for bad in ["", "..", "a/b", "a\\b", "a b", "ж"] {
            assert_eq!(
                vector_store_path(root, bad),
                Err(PathError::VectorStoreId),
                "{bad}"
            );
        }
    }

    #[test]
    fn runtime_ids_follow_the_catalogue_shape() {
        let root = Path::new("assets");
        assert_eq!(
            runtime_install_path(root, "llama-cpp"),
            Ok(root.join("runtimes").join("llama-cpp"))
        );
        assert!(runtime_install_path(root, &"a".repeat(63)).is_ok());
        let too_long = "a".repeat(64);
        for bad in ["a", "-ab", "Ab", "a_b", "../x", too_long.as_str()] {
            assert_eq!(
                runtime_install_path(root, bad),
                Err(PathError::RuntimeId(bad.to_owned()))
            );
        }
        assert_eq!(
            PathError::RuntimeId("x".to_owned()).to_string(),
            "Invalid runtime id: x"
        );
    }

    #[test]
    fn download_targets_refuse_unknown_kinds_and_unsafe_names() {
        let root = Path::new("assets");
        assert_eq!(
            download_target_path(root, "mcp", "server.tgz"),
            Ok(root.join("packages").join("mcp").join("server.tgz"))
        );
        assert_eq!(
            download_target_path(root, "model", "qwen.Q4_K_M.gguf"),
            Ok(root.join("models").join("qwen.Q4_K_M.gguf"))
        );
        assert_eq!(
            download_target_path(root, "nope", "a"),
            Err(PathError::DownloadKind("nope".to_owned()))
        );
        for bad in ["", ".", "..", "a/b", "a\\b", "a b", "a:b"] {
            assert_eq!(
                download_target_path(root, "skill", bad),
                Err(PathError::FileName(bad.to_owned()))
            );
        }
    }

    #[test]
    fn log_files_rotate_by_suffix() {
        let root = Path::new("logs");
        assert_eq!(log_file_path(root, 0), root.join("studio.jsonl"));
        assert_eq!(log_file_path(root, 2), root.join("studio.2.jsonl"));
    }

    #[test]
    fn stream_files_are_named_by_the_utc_date() {
        let root = Path::new("streams");
        assert_eq!(
            stream_file_path(root, 1_700_000_000_000),
            root.join("2023-11-14.jsonl")
        );
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(86_399_999), "1970-01-01");
        assert_eq!(utc_date(951_782_400_000), "2000-02-29");
        assert_eq!(utc_date(1_709_251_199_999), "2024-02-29");
        assert_eq!(utc_date(1_735_689_600_000), "2025-01-01");
        assert_eq!(utc_date(-1), "1969-12-31");
    }

    #[test]
    fn instants_format_as_iso_8601_and_as_a_file_safe_stamp() {
        assert_eq!(utc_iso(1_709_251_199_999), "2024-02-29T23:59:59.999Z");
        assert_eq!(utc_stamp(1_700_000_000_000), "2023-11-14T22-13-20");
    }

    #[test]
    fn backups_are_named_like_the_typescript_iso_stamp() {
        let root = Path::new("backups");
        assert_eq!(
            backup_file_path(root, 1_700_000_000_000),
            root.join("db-2023-11-14T22-13-20-000Z.sqlite")
        );
        assert_eq!(
            backup_file_path(root, 1_709_251_199_999),
            root.join("db-2024-02-29T23-59-59-999Z.sqlite")
        );
        assert!(is_backup_file("db-2023-11-14T22-13-20-000Z.sqlite"));
        assert!(!is_backup_file("studio.sqlite"));
        assert!(!is_backup_file("db-notes.txt"));
    }
}
