use std::path::{Path, PathBuf};
use std::time::SystemTime;

use zvs_contract::{CLI_THEME_FILE, CliTheme, CliThemeDto};

use crate::keymap_file::{self, APP_DATA_FOLDER};

pub fn default_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|root| {
        PathBuf::from(root)
            .join(APP_DATA_FOLDER)
            .join("cli")
            .join(CLI_THEME_FILE)
    })
}

pub fn load_from(path: &Path) -> CliTheme {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<CliThemeDto>(&bytes).ok())
        .unwrap_or_default()
        .theme
}

pub fn save_to(path: &Path, theme: CliTheme) -> std::io::Result<()> {
    let value = serde_json::to_value(CliThemeDto { theme }).map_err(std::io::Error::other)?;
    keymap_file::save_to(path, &value)
}

pub fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

pub fn load() -> CliTheme {
    default_path().map_or_else(CliTheme::default, |path| load_from(&path))
}

pub fn save(theme: CliTheme) -> bool {
    default_path().is_some_and(|path| save_to(&path, theme).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_theme_is_read_back_and_a_missing_or_broken_file_gives_the_default() {
        let path = std::env::temp_dir()
            .join(format!("zvs-cli-theme-{}", std::process::id()))
            .join("cli")
            .join(CLI_THEME_FILE);
        assert_eq!(load_from(&path), CliTheme::Ember);
        assert!(modified(&path).is_none());

        save_to(&path, CliTheme::Dusk).expect("saves");
        assert_eq!(load_from(&path), CliTheme::Dusk);
        assert!(modified(&path).is_some());

        std::fs::write(&path, b"{\"theme\":\"neon\"}").expect("writes");
        assert_eq!(load_from(&path), CliTheme::Ember);
        let _ = std::fs::remove_dir_all(path.parent().and_then(Path::parent).expect("root"));
    }
}
