use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::state::shortcuts::Keymap;

pub const APP_DATA_FOLDER: &str = "com.zvs.aistudio";
pub const SHORTCUTS_FILE: &str = "shortcuts.json";

pub fn default_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|root| {
        PathBuf::from(root)
            .join(APP_DATA_FOLDER)
            .join("cli")
            .join(SHORTCUTS_FILE)
    })
}

pub fn load_from(path: &Path) -> Keymap {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .map_or_else(Keymap::default, |value| Keymap::from_json(&value))
}

pub fn save_to(path: &Path, value: &Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    let staged = path.with_extension("json.tmp");
    std::fs::write(&staged, text)?;
    std::fs::rename(&staged, path)
}

pub fn load() -> Keymap {
    default_path().map_or_else(Keymap::default, |path| load_from(&path))
}

pub fn save(value: &Value) -> bool {
    default_path().is_some_and(|path| save_to(&path, value).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::shortcuts::{Chord, Shortcut};

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("zvs-cli-keymap-{}-{name}", std::process::id()))
            .join("cli")
            .join(SHORTCUTS_FILE)
    }

    #[test]
    fn a_saved_keymap_is_read_back_and_a_missing_or_broken_file_gives_defaults() {
        let path = scratch("round-trip");
        assert_eq!(load_from(&path), Keymap::default());

        let mut keymap = Keymap::default();
        keymap.bind(Shortcut::Models, Chord::alt('o'));
        save_to(&path, &keymap.to_json()).expect("saves");
        assert_eq!(load_from(&path), keymap);

        std::fs::write(&path, b"{not json").expect("writes");
        assert_eq!(load_from(&path), Keymap::default());
        let _ = std::fs::remove_dir_all(path.parent().and_then(Path::parent).expect("root"));
    }

    #[test]
    fn the_file_lives_beside_the_app_data() {
        if let Some(path) = default_path() {
            let text = path.to_string_lossy();
            assert!(text.contains(APP_DATA_FOLDER));
            assert!(text.ends_with(SHORTCUTS_FILE));
        }
    }
}
