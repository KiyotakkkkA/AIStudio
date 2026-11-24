use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const STATE_FILE: &str = "aistudio.toml";
pub const STATE_VENDOR: &str = "ZVS";
pub const STATE_FOLDER: &str = "AIStudio";
pub const DEV_STATE_FOLDER: &str = "AIStudio-Dev";
pub const LEGACY_FOLDER: &str = "com.zvs.aistudio";
pub const PROJECT_STRUCT: &str = "project_struct";
pub const DIALOG_HISTORY: &str = "dialog_history";
pub const SCHEMAS: [(&str, u32); 2] = [(PROJECT_STRUCT, 2), (DIALOG_HISTORY, 1)];
pub const CLI_SETTINGS_DIR: [&str; 3] = ["data", "settings", "cli"];

pub fn state_root(local_data: &Path, development: bool) -> PathBuf {
    let folder = if development {
        DEV_STATE_FOLDER
    } else {
        STATE_FOLDER
    };
    local_data.join(STATE_VENDOR).join(folder)
}

pub fn cli_settings_dir(root: &Path) -> PathBuf {
    CLI_SETTINGS_DIR
        .iter()
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

pub fn expected_version(schema: &str) -> Option<u32> {
    SCHEMAS
        .iter()
        .find(|(id, _)| *id == schema)
        .map(|(_, version)| *version)
}

#[derive(Debug, thiserror::Error)]
pub enum StateFileError {
    #[error("Could not read {STATE_FILE}: {0}")]
    Io(#[from] std::io::Error),
    #[error("{STATE_FILE} is malformed: {0}")]
    Malformed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LegacyRecord {
    pub source: String,
    pub migrated_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateFile {
    #[serde(default)]
    pub schemas: BTreeMap<String, u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy: Option<LegacyRecord>,
}

impl StateFile {
    pub fn current() -> Self {
        Self {
            schemas: SCHEMAS
                .iter()
                .map(|(id, version)| ((*id).to_owned(), *version))
                .collect(),
            legacy: None,
        }
    }

    pub fn path(root: &Path) -> PathBuf {
        root.join(STATE_FILE)
    }

    pub fn parse(text: &str) -> Result<Self, StateFileError> {
        toml::from_str(text).map_err(|error| StateFileError::Malformed(error.to_string()))
    }

    pub fn render(&self) -> Result<String, StateFileError> {
        toml::to_string(self).map_err(|error| StateFileError::Malformed(error.to_string()))
    }

    pub fn read(root: &Path) -> Result<Option<Self>, StateFileError> {
        match std::fs::read_to_string(Self::path(root)) {
            Ok(text) => Self::parse(&text).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn write(&self, root: &Path) -> Result<(), StateFileError> {
        std::fs::create_dir_all(root)?;
        let target = Self::path(root);
        let staged = root.join(format!("{STATE_FILE}.tmp"));
        {
            let mut file = std::fs::File::create(&staged)?;
            file.write_all(self.render()?.as_bytes())?;
            file.sync_all()?;
        }
        std::fs::rename(&staged, &target)?;
        Ok(())
    }

    pub fn version(&self, schema: &str) -> u32 {
        self.schemas.get(schema).copied().unwrap_or(0)
    }

    pub fn unknown_schemas(&self) -> Vec<String> {
        self.schemas
            .keys()
            .filter(|id| expected_version(id).is_none())
            .cloned()
            .collect()
    }

    pub fn matches_build(&self) -> bool {
        self.unknown_schemas().is_empty()
            && SCHEMAS
                .iter()
                .all(|(id, version)| self.version(id) == *version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("zvs-layout-{}-{name}", std::process::id()))
    }

    #[test]
    fn the_root_separates_production_and_development() {
        let local = Path::new("C:").join("Local");
        assert_eq!(
            state_root(&local, false),
            local.join("ZVS").join("AIStudio")
        );
        assert_eq!(
            state_root(&local, true),
            local.join("ZVS").join("AIStudio-Dev")
        );
        assert_eq!(
            cli_settings_dir(&local),
            local.join("data").join("settings").join("cli")
        );
    }

    #[test]
    fn the_current_file_round_trips_and_matches_the_build() {
        let file = StateFile::current();
        let text = file.render().expect("renders");
        assert!(text.contains("[schemas]"));
        assert!(text.contains("project_struct = 2"));
        let parsed = StateFile::parse(&text).expect("parses");
        assert_eq!(parsed, file);
        assert!(parsed.matches_build());
    }

    #[test]
    fn older_newer_and_unknown_schemas_do_not_match() {
        let older = StateFile::parse("[schemas]\nproject_struct = 0\n").expect("parses");
        assert_eq!(older.version(PROJECT_STRUCT), 0);
        assert!(!older.matches_build());
        let newer = StateFile::parse("[schemas]\nproject_struct = 2\n").expect("parses");
        assert!(!newer.matches_build());
        let unknown =
            StateFile::parse("[schemas]\nproject_struct = 1\nprofiles = 1\n").expect("parses");
        assert_eq!(unknown.unknown_schemas(), ["profiles"]);
        assert!(!unknown.matches_build());
        assert_eq!(StateFile::default().version(PROJECT_STRUCT), 0);
        assert!(StateFile::parse("[schemas\n").is_err());
    }

    #[test]
    fn the_legacy_record_survives_a_write_and_a_missing_file_reads_as_none() {
        let root = scratch("write");
        let _ = std::fs::remove_dir_all(&root);
        assert!(StateFile::read(&root).expect("reads").is_none());
        let mut file = StateFile::current();
        file.legacy = Some(LegacyRecord {
            source: "C:\\Roaming\\com.zvs.aistudio".to_owned(),
            migrated_at: 1_700_000_000_000,
        });
        file.write(&root).expect("writes");
        assert_eq!(StateFile::read(&root).expect("reads"), Some(file));
        assert!(!root.join("aistudio.toml.tmp").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
