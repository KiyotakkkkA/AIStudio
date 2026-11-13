use std::path::{Path, PathBuf};

use zvs_contract::{AppError, AppErrorCode, CLI_PREFERENCES_FILE, CliPreferencesDto};

pub const CLI_PREFERENCES_SAVE_FAILED: &str = "Не удалось сохранить настройки консольной версии";

pub struct CliPreferencesFile {
    path: PathBuf,
}

impl CliPreferencesFile {
    pub fn new(folder: &Path) -> Self {
        Self {
            path: folder.join(CLI_PREFERENCES_FILE),
        }
    }

    pub fn get(&self) -> CliPreferencesDto {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn set(&self, preferences: CliPreferencesDto) -> Result<CliPreferencesDto, AppError> {
        self.write(preferences).map_err(|error| {
            AppError::new(AppErrorCode::NativeError, CLI_PREFERENCES_SAVE_FAILED)
                .with_detail("reason", error.to_string())
        })?;
        Ok(preferences)
    }

    fn write(&self, preferences: CliPreferencesDto) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_vec_pretty(&preferences).map_err(std::io::Error::other)?;
        let staged = self.path.with_extension("json.tmp");
        std::fs::write(&staged, text)?;
        std::fs::rename(&staged, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_preferences_are_read_back_and_a_missing_or_broken_file_gives_the_defaults() {
        let root = tempfile::tempdir().expect("dir");
        let file = CliPreferencesFile::new(&root.path().join("cli"));
        assert!(file.get().auto_copy);

        let saved = file
            .set(CliPreferencesDto { auto_copy: false })
            .expect("saves");
        assert!(!saved.auto_copy);
        assert!(!file.get().auto_copy);

        std::fs::write(
            root.path().join("cli").join(CLI_PREFERENCES_FILE),
            b"{\"autoCopy\":1}",
        )
        .expect("writes");
        assert!(file.get().auto_copy);
    }
}
