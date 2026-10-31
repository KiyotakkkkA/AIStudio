use std::path::{Path, PathBuf};

use zvs_contract::{AppError, AppErrorCode, CLI_THEME_FILE, CliThemeDto};

pub const CLI_THEME_SAVE_FAILED: &str = "Не удалось сохранить тему консольной версии";

pub struct CliThemeFile {
    path: PathBuf,
}

impl CliThemeFile {
    pub fn new(folder: &Path) -> Self {
        Self {
            path: folder.join(CLI_THEME_FILE),
        }
    }

    pub fn get(&self) -> CliThemeDto {
        std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn set(&self, theme: CliThemeDto) -> Result<CliThemeDto, AppError> {
        self.write(theme).map_err(|error| {
            AppError::new(AppErrorCode::NativeError, CLI_THEME_SAVE_FAILED)
                .with_detail("reason", error.to_string())
        })?;
        Ok(theme)
    }

    fn write(&self, theme: CliThemeDto) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_vec_pretty(&theme).map_err(std::io::Error::other)?;
        let staged = self.path.with_extension("json.tmp");
        std::fs::write(&staged, text)?;
        std::fs::rename(&staged, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use zvs_contract::CliTheme;

    use super::*;

    #[test]
    fn a_saved_theme_is_read_back_and_a_missing_or_broken_file_gives_the_default() {
        let root = tempfile::tempdir().expect("dir");
        let file = CliThemeFile::new(&root.path().join("cli"));
        assert_eq!(file.get().theme, CliTheme::Ember);

        let saved = file
            .set(CliThemeDto {
                theme: CliTheme::Moss,
            })
            .expect("saves");
        assert_eq!(saved.theme, CliTheme::Moss);
        assert_eq!(file.get().theme, CliTheme::Moss);

        std::fs::write(root.path().join("cli").join(CLI_THEME_FILE), b"{oops").expect("writes");
        assert_eq!(file.get().theme, CliTheme::Ember);
    }
}
