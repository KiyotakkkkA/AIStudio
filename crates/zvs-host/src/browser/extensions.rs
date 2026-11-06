use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::Value;
use zvs_contract::browser_extensions::{EXTENSION_TEXT_MAX, EXTENSIONS_MAX, is_extension_id};
use zvs_contract::validate::js_slice;
use zvs_contract::{AppError, AppErrorCode, BrowserExtensionDto, BrowserExtensionsDto};

pub const EXTENSIONS_DIR: &str = "extensions";
pub const EXTENSIONS_OFF_DIR: &str = "extensions-off";
pub const NOT_AN_EXTENSION: &str =
    "В папке нет файла manifest.json распакованного расширения Chromium";
pub const EXTENSION_NOT_FOUND: &str = "Расширение не найдено";
pub const EXTENSIONS_FULL: &str = "Достигнуто наибольшее число расширений";
pub const EXTENSION_COPY_FAILED: &str = "Не удалось скопировать расширение";
pub const COPY_FILES_MAX: usize = 20_000;

pub type ExtensionsChanged = Arc<dyn Fn(Vec<PathBuf>) + Send + Sync>;

fn io_failed(cause: impl ToString) -> AppError {
    AppError::new(AppErrorCode::Unknown, EXTENSION_COPY_FAILED)
        .with_detail("reason", cause.to_string())
}

fn not_found() -> AppError {
    AppError::new(AppErrorCode::NotFound, EXTENSION_NOT_FOUND)
}

fn read_json(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

fn localized(folder: &Path, manifest: &Value, value: &str) -> String {
    let Some(key) = value
        .strip_prefix("__MSG_")
        .and_then(|rest| rest.strip_suffix("__"))
    else {
        return value.to_owned();
    };
    let locales = folder.join("_locales");
    let default = manifest
        .get("default_locale")
        .and_then(Value::as_str)
        .unwrap_or("en");
    ["ru", default, "en"]
        .iter()
        .filter_map(|locale| read_json(&locales.join(locale).join("messages.json")))
        .find_map(|messages| {
            messages
                .as_object()?
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(key))
                .and_then(|(_, entry)| entry.get("message"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| value.to_owned())
}

pub struct Manifest {
    pub name: String,
    pub version: String,
    pub description: String,
}

pub fn read_manifest(folder: &Path) -> Option<Manifest> {
    let manifest = read_json(&folder.join("manifest.json"))?;
    manifest.get("manifest_version")?.as_u64()?;
    let text = |field: &str| {
        let raw = manifest
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default();
        js_slice(localized(folder, &manifest, raw).trim(), EXTENSION_TEXT_MAX).to_owned()
    };
    let name = text("name");
    (!name.is_empty()).then(|| Manifest {
        name,
        version: text("version"),
        description: text("description"),
    })
}

pub fn slug(name: &str) -> String {
    let mut slug = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    let slug: String = slug.chars().take(64).collect();
    if slug.is_empty() {
        "extension".to_owned()
    } else {
        slug
    }
}

fn copy_tree(from: &Path, to: &Path, budget: &mut usize) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            continue;
        }
        *budget = budget
            .checked_sub(1)
            .ok_or_else(|| std::io::Error::other("too many files"))?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target, budget)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[derive(Clone)]
pub struct BrowserExtensions {
    root: PathBuf,
    changed: Arc<Mutex<Option<ExtensionsChanged>>>,
}

impl BrowserExtensions {
    pub fn new(browser_dir: &Path) -> Self {
        Self {
            root: browser_dir.to_path_buf(),
            changed: Arc::default(),
        }
    }

    pub fn on_change(&self, listener: ExtensionsChanged) {
        *self.changed.lock().unwrap_or_else(PoisonError::into_inner) = Some(listener);
    }

    pub fn enabled_dir(&self) -> PathBuf {
        self.root.join(EXTENSIONS_DIR)
    }

    fn disabled_dir(&self) -> PathBuf {
        self.root.join(EXTENSIONS_OFF_DIR)
    }

    fn folders(dir: &Path) -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .filter(|path| path.is_dir())
                    .filter(|path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(is_extension_id)
                    })
                    .collect()
            })
            .unwrap_or_default();
        folders.sort();
        folders
    }

    pub fn enabled_folders(&self) -> Vec<PathBuf> {
        Self::folders(&self.enabled_dir())
            .into_iter()
            .filter(|folder| read_manifest(folder).is_some())
            .collect()
    }

    pub fn list(&self) -> BrowserExtensionsDto {
        let mut items: Vec<BrowserExtensionDto> =
            [(self.enabled_dir(), true), (self.disabled_dir(), false)]
                .into_iter()
                .flat_map(|(dir, enabled)| {
                    Self::folders(&dir).into_iter().filter_map(move |folder| {
                        let manifest = read_manifest(&folder)?;
                        Some(BrowserExtensionDto {
                            id: folder.file_name()?.to_string_lossy().into_owned(),
                            name: manifest.name,
                            version: manifest.version,
                            description: manifest.description,
                            enabled,
                        })
                    })
                })
                .collect();
        items.sort_by_key(|item| item.name.to_lowercase());
        items.truncate(EXTENSIONS_MAX);
        BrowserExtensionsDto { items }
    }

    fn notify(&self) {
        let listener = self
            .changed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(listener) = listener {
            listener(self.enabled_folders());
        }
    }

    fn locate(&self, id: &str) -> Result<(PathBuf, bool), AppError> {
        if !is_extension_id(id) {
            return Err(not_found());
        }
        [(self.enabled_dir(), true), (self.disabled_dir(), false)]
            .into_iter()
            .map(|(dir, enabled)| (dir.join(id), enabled))
            .find(|(folder, _)| folder.is_dir())
            .ok_or_else(not_found)
    }

    fn taken(&self, id: &str) -> bool {
        self.enabled_dir().join(id).exists() || self.disabled_dir().join(id).exists()
    }

    pub fn add(&self, source: &Path) -> Result<BrowserExtensionsDto, AppError> {
        let manifest = read_manifest(source)
            .ok_or_else(|| AppError::new(AppErrorCode::ValidationFailed, NOT_AN_EXTENSION))?;
        if self.list().items.len() >= EXTENSIONS_MAX {
            return Err(AppError::new(AppErrorCode::Conflict, EXTENSIONS_FULL));
        }
        let base = slug(&manifest.name);
        let id = std::iter::once(base.clone())
            .chain((2..).map(|index| format!("{base}-{index}")))
            .find(|candidate| !self.taken(candidate))
            .unwrap_or(base);
        let target = self.enabled_dir().join(&id);
        let mut budget = COPY_FILES_MAX;
        if let Err(error) = copy_tree(source, &target, &mut budget) {
            let _ = std::fs::remove_dir_all(&target);
            return Err(io_failed(error));
        }
        self.notify();
        Ok(self.list())
    }

    pub fn remove(&self, id: &str) -> Result<BrowserExtensionsDto, AppError> {
        let (folder, _) = self.locate(id)?;
        std::fs::remove_dir_all(folder).map_err(io_failed)?;
        self.notify();
        Ok(self.list())
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<BrowserExtensionsDto, AppError> {
        let (folder, current) = self.locate(id)?;
        if current != enabled {
            let target_dir = if enabled {
                self.enabled_dir()
            } else {
                self.disabled_dir()
            };
            std::fs::create_dir_all(&target_dir).map_err(io_failed)?;
            std::fs::rename(&folder, target_dir.join(id)).map_err(io_failed)?;
            self.notify();
        }
        Ok(self.list())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use serde_json::json;

    use super::*;

    fn unpacked(parent: &Path, name: &str, manifest: &Value) -> PathBuf {
        let folder = parent.join(name);
        std::fs::create_dir_all(folder.join("icons")).expect("creates");
        std::fs::write(folder.join("manifest.json"), manifest.to_string()).expect("writes");
        std::fs::write(folder.join("icons").join("16.png"), "png").expect("writes");
        folder
    }

    #[test]
    fn localized_names_resolve_and_unsafe_names_become_slugs() {
        let home = tempfile::tempdir().expect("a folder");
        let folder = unpacked(
            home.path(),
            "src",
            &json!({ "manifest_version": 3, "name": "__MSG_appName__", "version": "1.0", "default_locale": "en" }),
        );
        std::fs::create_dir_all(folder.join("_locales").join("en")).expect("creates");
        std::fs::write(
            folder.join("_locales").join("en").join("messages.json"),
            json!({ "appname": { "message": "Dark Reader" } }).to_string(),
        )
        .expect("writes");
        assert_eq!(read_manifest(&folder).expect("reads").name, "Dark Reader");
        assert_eq!(slug("uBlock Origin!"), "ublock-origin");
        assert_eq!(slug("Блокировщик"), "extension");
        assert!(read_manifest(home.path()).is_none());
    }

    #[test]
    fn added_extensions_are_copied_toggled_and_removed() {
        let home = tempfile::tempdir().expect("a folder");
        let source = unpacked(
            home.path(),
            "source",
            &json!({ "manifest_version": 3, "name": "uBlock Origin", "version": "1.62.0", "description": "Блокировщик" }),
        );
        let extensions = BrowserExtensions::new(&home.path().join("browser"));
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        extensions.on_change(Arc::new(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
        }));
        let first = extensions.add(&source).expect("adds");
        assert_eq!(first.items[0].id, "ublock-origin");
        assert!(first.items[0].enabled);
        let second = extensions.add(&source).expect("adds again");
        assert_eq!(second.items.len(), 2);
        assert!(second.items.iter().any(|item| item.id == "ublock-origin-2"));
        assert!(
            extensions
                .enabled_dir()
                .join("ublock-origin")
                .join("icons")
                .join("16.png")
                .is_file()
        );
        let toggled = extensions
            .set_enabled("ublock-origin", false)
            .expect("turns off");
        assert_eq!(extensions.enabled_folders().len(), 1);
        assert!(
            toggled
                .items
                .iter()
                .any(|item| item.id == "ublock-origin" && !item.enabled)
        );
        extensions.remove("ublock-origin").expect("removes");
        assert_eq!(extensions.list().items.len(), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        assert_eq!(
            extensions.remove("..").expect_err("refused").message,
            EXTENSION_NOT_FOUND
        );
        assert_eq!(
            extensions
                .add(home.path())
                .expect_err("no manifest")
                .message,
            NOT_AN_EXTENSION
        );
    }
}
