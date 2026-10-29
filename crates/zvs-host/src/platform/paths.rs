use std::path::{Path, PathBuf};

use zvs_contract::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathEnvironment {
    pub data_dir: PathBuf,
    pub resource_dir: PathBuf,
    pub executable_dir: PathBuf,
    pub packaged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudioPaths {
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub logs_dir: PathBuf,
    pub streams_dir: PathBuf,
    pub backups_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub vector_stores_dir: PathBuf,
    pub downloads_dir: PathBuf,
    pub conversations_dir: PathBuf,
    pub resources_dir: PathBuf,
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

impl StudioPaths {
    pub fn resolve(env: &PathEnvironment) -> Self {
        let data = &env.data_dir;
        let logs_dir = data.join("logs");
        Self {
            data_dir: data.clone(),
            db_path: data.join("studio.sqlite"),
            streams_dir: logs_dir.join("streams"),
            logs_dir,
            backups_dir: data.join("backups"),
            cache_dir: data.join("cache"),
            vector_stores_dir: data.join("vectors"),
            downloads_dir: data.join("downloads"),
            conversations_dir: data.join(crate::conversations::CONVERSATIONS_DIR),
            resources_dir: env.resource_dir.clone(),
            sidecar_path: sidecar_path(&env.executable_dir, cfg!(windows)),
            cli_path: cli_path(&env.executable_dir, env.packaged),
            packaged: env.packaged,
        }
    }

    pub fn download_dir(&self, kind: DownloadKind) -> PathBuf {
        self.downloads_dir.join(kind.folder())
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

    pub const fn folder(self) -> &'static str {
        match self {
            Self::Model => "models",
            Self::Embedding => "embeddings",
            Self::Runtime => "runtimes",
            Self::Mcp => "mcp",
            Self::Skill => "skills",
        }
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
        Ok(downloads
            .join(DownloadKind::Runtime.folder())
            .join(runtime_id))
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
        Ok(directory.join(kind.folder()).join(file_name))
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
            data_dir: root.join("user"),
            resource_dir: root.join("bundle"),
            executable_dir: root.join("target").join("debug"),
            packaged: false,
        }
    }

    #[test]
    fn platform_paths_separate_persistent_data_from_bundled_resources() {
        let env = env();
        let paths = StudioPaths::resolve(&env);
        assert_eq!(paths.data_dir, env.data_dir);
        assert_eq!(paths.db_path, env.data_dir.join("studio.sqlite"));
        assert_eq!(paths.logs_dir, env.data_dir.join("logs"));
        assert_eq!(paths.streams_dir, env.data_dir.join("logs").join("streams"));
        assert_eq!(paths.backups_dir, env.data_dir.join("backups"));
        assert_eq!(paths.cache_dir, env.data_dir.join("cache"));
        assert_eq!(paths.vector_stores_dir, env.data_dir.join("vectors"));
        assert_eq!(paths.downloads_dir, env.data_dir.join("downloads"));
        assert_eq!(paths.resources_dir, env.resource_dir);
        assert_eq!(
            paths.sidecar_path,
            env.executable_dir.join(sidecar_file_name(cfg!(windows)))
        );
        assert!(!paths.packaged);
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
    fn downloads_land_in_one_folder_per_kind() {
        let paths = StudioPaths::resolve(&env());
        let folders: Vec<PathBuf> = DownloadKind::ALL
            .iter()
            .map(|kind| paths.download_dir(*kind))
            .collect();
        let expected: Vec<PathBuf> = ["models", "embeddings", "runtimes", "mcp", "skills"]
            .iter()
            .map(|folder| paths.downloads_dir.join(folder))
            .collect();
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
        let root = Path::new("downloads");
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
        let root = Path::new("downloads");
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
