use std::path::{Path, PathBuf};
use std::time::SystemTime;

use zvs_contract::{CLI_PREFERENCES_FILE, CliPreferencesDto};

use crate::keymap_file;

pub fn default_path() -> Option<PathBuf> {
    keymap_file::settings_dir().map(|dir| dir.join(CLI_PREFERENCES_FILE))
}

pub fn load_from(path: &Path) -> CliPreferencesDto {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<CliPreferencesDto>(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_to(path: &Path, preferences: CliPreferencesDto) -> std::io::Result<()> {
    let value = serde_json::to_value(preferences).map_err(std::io::Error::other)?;
    keymap_file::save_to(path, &value)
}

pub fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

pub fn load() -> CliPreferencesDto {
    default_path().map_or_else(CliPreferencesDto::default, |path| load_from(&path))
}

pub fn save(preferences: CliPreferencesDto) -> bool {
    default_path().is_some_and(|path| save_to(&path, preferences).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_preferences_are_read_back_and_a_missing_or_broken_file_gives_the_defaults() {
        let path = std::env::temp_dir()
            .join(format!("zvs-cli-preferences-{}", std::process::id()))
            .join("cli")
            .join(CLI_PREFERENCES_FILE);
        assert!(load_from(&path).auto_copy);
        assert!(modified(&path).is_none());

        save_to(&path, CliPreferencesDto { auto_copy: false }).expect("saves");
        assert!(!load_from(&path).auto_copy);
        assert!(modified(&path).is_some());

        std::fs::write(&path, b"{\"autoCopy\":\"no\"}").expect("writes");
        assert!(load_from(&path).auto_copy);
        let _ = std::fs::remove_dir_all(path.parent().and_then(Path::parent).expect("root"));
    }
}
