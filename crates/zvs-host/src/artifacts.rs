use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use zvs_contract::artifacts::BROWSERS_MAX;
use zvs_contract::{
    AppError, AppErrorCode, ArtifactApplicationDto, ArtifactBrowsersDto, ArtifactDto,
    ArtifactSaveResult, InstalledBrowserDto,
};

use crate::artifact_pages::{
    NOT_A_PAGE, PAGES_UNAVAILABLE, PageResolver, PageServer, SharedPageOpener, page_type,
};
use crate::platform::external::{
    PathRevealer, SharedFileLauncher, system_file_launcher, system_path_revealer,
};
use crate::platform::long_path::extended;
use crate::platform::picker::{SAVE_DIALOG_UNAVAILABLE, SharedPicker};

pub const ARTIFACT_EXTENSIONS: &[&str] = &[
    "pdf", "docx", "doc", "xlsx", "xlsm", "xls", "pptx", "ppt", "odt", "ods", "odp", "rtf", "csv",
    "md", "html", "png", "jpg", "jpeg", "svg", "zip",
];
pub const ARTIFACTS_PER_CALL: usize = 16;
pub const ARTIFACT_MAX_BYTES: u64 = 512 * 1024 * 1024;
const SNAPSHOT_DEPTH: usize = 4;
const SNAPSHOT_FILES_MAX: usize = 4096;

pub const ARTIFACT_NOT_FOUND: &str = "Файл не найден";
pub const ARTIFACT_SAVE_FAILED: &str = "Не удалось сохранить файл";
pub const BROWSER_NOT_FOUND: &str = "Браузер не найден";
pub const DASHBOARDS: &str = "dashboards";

pub type Snapshot = HashMap<PathBuf, (u64, Option<SystemTime>)>;

fn walk(directory: &Path, depth: usize, into: &mut Snapshot) {
    if depth > SNAPSHOT_DEPTH || into.len() >= SNAPSHOT_FILES_MAX {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            walk(&path, depth + 1, into);
        } else if kind.is_file()
            && let Ok(metadata) = entry.metadata()
        {
            into.insert(path, (metadata.len(), metadata.modified().ok()));
        }
        if into.len() >= SNAPSHOT_FILES_MAX {
            return;
        }
    }
}

pub fn snapshot(directory: &Path) -> Snapshot {
    let mut files = Snapshot::new();
    walk(directory, 0, &mut files);
    files
}

pub fn is_artifact(path: &Path) -> bool {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|extension| ARTIFACT_EXTENSIONS.contains(&extension.as_str()))
}

pub fn produced(
    directory: &Path,
    before: &Snapshot,
    mentioned: &[PathBuf],
    since: SystemTime,
) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for path in mentioned {
        let fresh = std::fs::metadata(path)
            .ok()
            .filter(std::fs::Metadata::is_file)
            .and_then(|metadata| metadata.modified().ok())
            .is_some_and(|modified| modified >= since);
        if fresh && !found.contains(path) {
            found.push(path.clone());
        }
    }
    let mut changed: Vec<PathBuf> = snapshot(directory)
        .into_iter()
        .filter(|(path, state)| before.get(path) != Some(state) && !found.contains(path))
        .map(|(path, _)| path)
        .collect();
    changed.sort();
    found.extend(changed);
    found
        .into_iter()
        .filter(|path| is_artifact(path))
        .filter(|path| {
            std::fs::metadata(path).is_ok_and(|metadata| metadata.len() <= ARTIFACT_MAX_BYTES)
        })
        .take(ARTIFACTS_PER_CALL)
        .collect()
}

pub fn mentioned_paths(arguments: &[String], base: &Path) -> Vec<PathBuf> {
    arguments
        .iter()
        .filter(|argument| !argument.starts_with('-'))
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                base.join(path)
            }
        })
        .filter(|path| is_artifact(path))
        .collect()
}

fn not_found(id: &str) -> AppError {
    AppError::new(AppErrorCode::NotFound, ARTIFACT_NOT_FOUND).with_detail("id", id)
}

fn unique_name(name: &str, taken: &mut HashSet<String>) -> String {
    let path = Path::new(name);
    let stem = path.file_stem().map_or_else(
        || name.to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    let extension = path
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    let mut candidate = name.to_owned();
    let mut index = 2;
    while !taken.insert(candidate.to_lowercase()) {
        candidate = format!("{stem} ({index}){extension}");
        index += 1;
    }
    candidate
}

#[derive(Clone)]
pub struct ArtifactStore {
    root: PathBuf,
    libraries: Vec<(String, PathBuf)>,
    picker: Option<SharedPicker>,
    reveal: PathRevealer,
    launcher: SharedFileLauncher,
    pages: Option<SharedPageOpener>,
    server: Arc<tokio::sync::OnceCell<PageServer>>,
}

pub fn library_env(name: &str) -> String {
    format!("ZVS_{}_DIR", name.to_ascii_uppercase())
}

impl ArtifactStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            libraries: Vec::new(),
            picker: None,
            reveal: system_path_revealer(),
            launcher: system_file_launcher(),
            pages: None,
            server: Arc::new(tokio::sync::OnceCell::new()),
        }
    }

    #[must_use]
    pub fn with_library(mut self, name: &str, folder: PathBuf) -> Self {
        self.libraries.push((name.to_owned(), folder));
        self
    }

    #[must_use]
    pub fn with_pages(mut self, pages: SharedPageOpener) -> Self {
        self.pages = Some(pages);
        self
    }

    pub fn libraries(&self) -> &[(String, PathBuf)] {
        &self.libraries
    }

    fn library(&self, name: &str) -> Option<&Path> {
        self.libraries
            .iter()
            .find(|(known, _)| known == name)
            .map(|(_, folder)| folder.as_path())
    }

    pub fn adopt(&self, library: &str, files: &[PathBuf]) -> Vec<ArtifactDto> {
        let Some(folder) = self.library(library) else {
            return Vec::new();
        };
        files
            .iter()
            .filter(|file| file.parent() == Some(folder))
            .filter_map(|file| {
                let name = file.file_name()?.to_string_lossy().into_owned();
                let size_bytes = std::fs::metadata(extended(file))
                    .ok()
                    .filter(std::fs::Metadata::is_file)?
                    .len();
                let format = Path::new(&name)
                    .extension()
                    .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                Some(ArtifactDto {
                    id: format!("{library}/{name}"),
                    name,
                    format,
                    size_bytes,
                })
            })
            .collect()
    }

    #[must_use]
    pub fn with_picker(mut self, picker: SharedPicker) -> Self {
        self.picker = Some(picker);
        self
    }

    #[must_use]
    pub fn with_revealer(mut self, reveal: PathRevealer) -> Self {
        self.reveal = reveal;
        self
    }

    #[must_use]
    pub fn with_launcher(mut self, launcher: SharedFileLauncher) -> Self {
        self.launcher = launcher;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn keep(&self, run: &str, files: &[PathBuf]) -> Vec<ArtifactDto> {
        let folder = self.root.join(run);
        if files.is_empty() || std::fs::create_dir_all(extended(&folder)).is_err() {
            return Vec::new();
        }
        let mut taken: HashSet<String> = HashSet::new();
        let mut kept = Vec::new();
        for file in files {
            let Some(original) = file
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
            else {
                continue;
            };
            let name = unique_name(&original, &mut taken);
            let target = folder.join(&name);
            let Ok(bytes) = std::fs::copy(extended(file), extended(&target)) else {
                continue;
            };
            let format = Path::new(&name)
                .extension()
                .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            kept.push(ArtifactDto {
                id: format!("{run}/{name}"),
                name,
                format,
                size_bytes: bytes,
            });
        }
        kept
    }

    pub fn resolve(&self, id: &str) -> Result<PathBuf, AppError> {
        zvs_contract::Validate::validate(&zvs_contract::ArtifactRef { id: id.to_owned() })?;
        let (run, name) = id.split_once('/').ok_or_else(|| not_found(id))?;
        let path = if let Some(folder) = self.library(run) {
            let path = folder.join(name);
            if path.parent() != Some(folder) {
                return Err(not_found(id));
            }
            path
        } else {
            let path = self.root.join(run).join(name);
            if path.parent().and_then(Path::parent) != Some(self.root.as_path()) {
                return Err(not_found(id));
            }
            path
        };
        if std::fs::metadata(extended(&path)).is_ok_and(|metadata| metadata.is_file()) {
            Ok(path)
        } else {
            Err(not_found(id))
        }
    }

    pub fn reveal(&self, id: &str) -> Result<(), AppError> {
        let path = self.resolve(id)?;
        (self.reveal)(&path)
    }

    pub fn open(&self, id: &str) -> Result<(), AppError> {
        let path = self.resolve(id)?;
        self.launcher.open(&path)
    }

    pub fn application(&self, id: &str) -> Result<ArtifactApplicationDto, AppError> {
        let path = self.resolve(id)?;
        let name = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
            .and_then(|extension| self.launcher.application(&extension))
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .map(|name| {
                name.chars()
                    .take(zvs_contract::artifacts::ARTIFACT_APPLICATION_MAX)
                    .collect()
            });
        Ok(ArtifactApplicationDto { name })
    }

    pub fn browsers(&self) -> ArtifactBrowsersDto {
        let browsers = self
            .launcher
            .browsers()
            .into_iter()
            .take(BROWSERS_MAX)
            .map(|browser| InstalledBrowserDto {
                id: browser.id,
                name: browser.name,
            })
            .collect();
        ArtifactBrowsersDto { browsers }
    }

    pub fn open_in_browser(&self, id: &str, browser: &str) -> Result<(), AppError> {
        let path = self.resolve(id)?;
        let installed = self
            .launcher
            .browsers()
            .into_iter()
            .find(|candidate| candidate.id == browser)
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, BROWSER_NOT_FOUND))?;
        self.launcher.open_in_browser(&installed, &path)
    }

    pub async fn open_in_app(&self, id: &str) -> Result<(), AppError> {
        let path = self.resolve(id)?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if page_type(&name).is_none() {
            return Err(AppError::new(AppErrorCode::ValidationFailed, NOT_A_PAGE));
        }
        let pages = self
            .pages
            .clone()
            .ok_or_else(|| AppError::new(AppErrorCode::NotImplemented, PAGES_UNAVAILABLE))?;
        let store = self.clone();
        let resolve: PageResolver = Arc::new(move |id: &str| store.resolve(id));
        let server = self
            .server
            .get_or_try_init(|| PageServer::start(resolve))
            .await?;
        pages.open(&server.url(id)?).await
    }

    pub async fn save(&self, id: &str) -> Result<ArtifactSaveResult, AppError> {
        let source = self.resolve(id)?;
        let picker = self
            .picker
            .clone()
            .ok_or_else(|| AppError::new(AppErrorCode::NotImplemented, SAVE_DIALOG_UNAVAILABLE))?;
        let name = source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(target) = picker.save_as(&name).await? else {
            return Ok(ArtifactSaveResult { path: None });
        };
        let destination = PathBuf::from(&target);
        tokio::task::spawn_blocking(move || {
            std::fs::copy(extended(&source), extended(&destination))
        })
        .await
        .map_err(|error| {
            AppError::new(AppErrorCode::Unknown, ARTIFACT_SAVE_FAILED)
                .with_detail("raw", error.to_string())
        })?
        .map_err(|error| {
            let code = if error.kind() == std::io::ErrorKind::PermissionDenied {
                AppErrorCode::PermissionDenied
            } else {
                AppErrorCode::Unknown
            };
            AppError::new(code, ARTIFACT_SAVE_FAILED).with_detail("raw", error.to_string())
        })?;
        Ok(ArtifactSaveResult { path: Some(target) })
    }
}

pub type SharedArtifactStore = Arc<ArtifactStore>;

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use super::*;
    use crate::testing::{FakeLauncher, FakePicker};

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("creates");
        std::fs::write(path, text).expect("writes");
    }

    #[test]
    fn only_new_or_changed_documents_count_as_produced() {
        let scratch = tempfile::tempdir().expect("a folder");
        let old = scratch.path().join("input.docx");
        let changed = scratch.path().join("draft.xlsx");
        write(&old, "old");
        write(&changed, "v1");
        let before = snapshot(scratch.path());
        std::thread::sleep(Duration::from_millis(20));
        let since = SystemTime::now();
        write(&changed, "version two");
        write(&scratch.path().join("out").join("Отчёт.pdf"), "pdf");
        write(&scratch.path().join("helper.py"), "print()");
        let outside = tempfile::tempdir().expect("another folder");
        let mentioned = outside.path().join("final.pptx");
        write(&mentioned, "deck");
        let found = produced(
            scratch.path(),
            &before,
            &[mentioned.clone(), old.clone()],
            since,
        );
        let names: Vec<String> = found
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["final.pptx", "draft.xlsx", "Отчёт.pdf"]);
    }

    #[test]
    fn documents_named_in_the_arguments_survive_the_cap() {
        let scratch = tempfile::tempdir().expect("a folder");
        let before = snapshot(scratch.path());
        let since = SystemTime::now() - Duration::from_secs(1);
        for index in 0..ARTIFACTS_PER_CALL + 4 {
            write(
                &scratch
                    .path()
                    .join("noise")
                    .join(format!("{index:02}.html")),
                "x",
            );
        }
        let outside = tempfile::tempdir().expect("another folder");
        let requested = outside.path().join("demo.pdf");
        write(&requested, "pdf");
        let found = produced(
            scratch.path(),
            &before,
            std::slice::from_ref(&requested),
            since,
        );
        assert_eq!(found.len(), ARTIFACTS_PER_CALL);
        assert_eq!(found[0], requested);
    }

    #[test]
    fn arguments_that_name_documents_resolve_against_the_working_folder() {
        let base = Path::new("/scratch");
        let found = mentioned_paths(
            &[
                "--out".to_owned(),
                "report.docx".to_owned(),
                "notes.txt".to_owned(),
            ],
            base,
        );
        assert_eq!(found, [base.join("report.docx")]);
    }

    #[test]
    fn kept_copies_survive_the_scratch_folder_and_never_collide() {
        let scratch = tempfile::tempdir().expect("a folder");
        let root = tempfile::tempdir().expect("the store");
        let store = ArtifactStore::new(root.path().to_path_buf());
        let first = scratch.path().join("a").join("report.docx");
        let second = scratch.path().join("b").join("report.docx");
        write(&first, "one");
        write(&second, "two");
        let kept = store.keep("run-1", &[first, second]);
        assert_eq!(
            kept.iter()
                .map(|artifact| artifact.id.as_str())
                .collect::<Vec<_>>(),
            ["run-1/report.docx", "run-1/report (2).docx"]
        );
        assert_eq!(kept[0].format, "docx");
        assert_eq!(kept[1].size_bytes, 3);
        drop(scratch);
        assert!(store.resolve("run-1/report (2).docx").is_ok());
    }

    #[test]
    fn ids_never_resolve_outside_the_store() {
        let root = tempfile::tempdir().expect("the store");
        let store = ArtifactStore::new(root.path().join("artifacts"));
        write(&root.path().join("secret.pdf"), "x");
        for id in [
            "../secret.pdf",
            "artifacts/../../secret.pdf",
            "run/missing.pdf",
        ] {
            let error = store.resolve(id).expect_err(id);
            assert!(
                matches!(
                    error.code,
                    AppErrorCode::NotFound | AppErrorCode::ValidationFailed
                ),
                "{id}"
            );
        }
    }

    #[test]
    fn open_hands_the_file_to_the_default_application_and_names_it() {
        let root = tempfile::tempdir().expect("the store");
        let launcher = Arc::new(FakeLauncher {
            opened: Mutex::new(Vec::new()),
            application: Some(" Word ".to_owned()),
        });
        let store = ArtifactStore::new(root.path().to_path_buf()).with_launcher(launcher.clone());
        let path = root.path().join("run-1").join("Отчёт.docx");
        write(&path, "x");

        store.open("run-1/Отчёт.docx").expect("opens");
        assert_eq!(*launcher.opened.lock().expect("records"), [path]);
        assert_eq!(
            store.application("run-1/Отчёт.docx").expect("names"),
            ArtifactApplicationDto {
                name: Some("Word".to_owned())
            }
        );
        let error = store.open("../secret.pdf").expect_err("outside");
        assert!(matches!(
            error.code,
            AppErrorCode::NotFound | AppErrorCode::ValidationFailed
        ));
        assert_eq!(launcher.opened.lock().expect("records").len(), 1);
    }

    #[test]
    fn a_format_without_a_registered_application_has_no_name() {
        let root = tempfile::tempdir().expect("the store");
        let store =
            ArtifactStore::new(root.path().to_path_buf()).with_launcher(Arc::new(FakeLauncher {
                opened: Mutex::new(Vec::new()),
                application: Some("  ".to_owned()),
            }));
        write(&root.path().join("run-1").join("data.csv"), "a,b");
        assert_eq!(
            store.application("run-1/data.csv").expect("answers"),
            ArtifactApplicationDto { name: None }
        );
    }

    #[tokio::test]
    async fn save_copies_to_the_picked_path_and_a_cancelled_dialog_saves_nothing() {
        let root = tempfile::tempdir().expect("the store");
        let picker = FakePicker::shared();
        let revealed = Arc::new(Mutex::new(Vec::new()));
        let seen = revealed.clone();
        let store = ArtifactStore::new(root.path().to_path_buf())
            .with_picker(picker.clone())
            .with_revealer(Arc::new(move |path: &Path| {
                seen.lock().expect("records").push(path.to_path_buf());
                Ok(())
            }));
        write(&root.path().join("run").join("plan.pdf"), "pdf");
        assert_eq!(
            store.save("run/plan.pdf").await.expect("cancelled"),
            ArtifactSaveResult { path: None }
        );
        let target = root.path().join("saved.pdf");
        picker.answer(&[&target.to_string_lossy()]);
        let saved = store.save("run/plan.pdf").await.expect("saves");
        assert_eq!(
            saved.path.as_deref(),
            Some(target.to_string_lossy().as_ref())
        );
        assert_eq!(std::fs::read_to_string(&target).expect("reads"), "pdf");
        store.reveal("run/plan.pdf").expect("reveals");
        assert_eq!(
            revealed.lock().expect("reads").as_slice(),
            [root.path().join("run").join("plan.pdf")]
        );
    }

    struct Browsers {
        launched: Mutex<Vec<(String, PathBuf)>>,
    }

    impl crate::platform::external::FileLauncher for Browsers {
        fn open(&self, _path: &Path) -> Result<(), AppError> {
            Ok(())
        }

        fn browsers(&self) -> Vec<crate::platform::external::InstalledBrowser> {
            vec![crate::platform::external::InstalledBrowser {
                id: "Google Chrome".to_owned(),
                name: "Google Chrome".to_owned(),
                program: PathBuf::from("C:/Chrome/chrome.exe"),
            }]
        }

        fn open_in_browser(
            &self,
            browser: &crate::platform::external::InstalledBrowser,
            path: &Path,
        ) -> Result<(), AppError> {
            self.launched
                .lock()
                .expect("records")
                .push((browser.id.clone(), path.to_path_buf()));
            Ok(())
        }
    }

    struct Pages(Mutex<Vec<String>>);

    #[async_trait::async_trait]
    impl crate::artifact_pages::PageOpener for Pages {
        async fn open(&self, url: &str) -> Result<(), AppError> {
            self.0.lock().expect("records").push(url.to_owned());
            Ok(())
        }
    }

    #[test]
    fn library_files_are_adopted_in_place_and_never_resolve_outside_their_folder() {
        let root = tempfile::tempdir().expect("the data folder");
        let dashboards = root.path().join(DASHBOARDS);
        let store = ArtifactStore::new(root.path().join("artifacts"))
            .with_library(DASHBOARDS, dashboards.clone());
        write(&dashboards.join("Продажи.html"), "<p>ok</p>");
        write(&dashboards.join("nested").join("deep.html"), "x");
        write(&root.path().join("secret.html"), "x");
        let adopted = store.adopt(
            DASHBOARDS,
            &[
                dashboards.join("Продажи.html"),
                dashboards.join("nested").join("deep.html"),
                root.path().join("secret.html"),
            ],
        );
        assert_eq!(
            adopted
                .iter()
                .map(|artifact| (artifact.id.as_str(), artifact.format.as_str()))
                .collect::<Vec<_>>(),
            [("dashboards/Продажи.html", "html")]
        );
        assert_eq!(adopted[0].size_bytes, 9);
        assert_eq!(
            store.resolve("dashboards/Продажи.html").expect("resolves"),
            dashboards.join("Продажи.html")
        );
        assert!(store.resolve("dashboards/..").is_err());
        assert!(store.resolve("dashboards/missing.html").is_err());
        assert!(
            store
                .adopt("unknown", &[dashboards.join("Продажи.html")])
                .is_empty()
        );
        assert_eq!(library_env(DASHBOARDS), "ZVS_DASHBOARDS_DIR");
    }

    #[test]
    fn a_file_opens_only_in_a_browser_the_system_lists() {
        let root = tempfile::tempdir().expect("the data folder");
        let dashboards = root.path().join(DASHBOARDS);
        write(&dashboards.join("sales.html"), "x");
        let launcher = Arc::new(Browsers {
            launched: Mutex::new(Vec::new()),
        });
        let store = ArtifactStore::new(root.path().join("artifacts"))
            .with_library(DASHBOARDS, dashboards.clone())
            .with_launcher(launcher.clone());
        assert_eq!(
            store.browsers().browsers,
            [InstalledBrowserDto {
                id: "Google Chrome".to_owned(),
                name: "Google Chrome".to_owned(),
            }]
        );
        store
            .open_in_browser("dashboards/sales.html", "Google Chrome")
            .expect("opens");
        let error = store
            .open_in_browser("dashboards/sales.html", "C:/Windows/System32/cmd.exe")
            .expect_err("an unlisted program is refused");
        assert_eq!(error.code, AppErrorCode::NotFound);
        assert_eq!(error.message, BROWSER_NOT_FOUND);
        assert_eq!(
            launcher.launched.lock().expect("reads").as_slice(),
            [("Google Chrome".to_owned(), dashboards.join("sales.html"))]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_page_opens_in_the_app_through_one_loopback_server() {
        let root = tempfile::tempdir().expect("the data folder");
        let dashboards = root.path().join(DASHBOARDS);
        write(&dashboards.join("sales.html"), "x");
        write(&dashboards.join("data.xlsx"), "x");
        let pages = Arc::new(Pages(Mutex::new(Vec::new())));
        let without = ArtifactStore::new(root.path().join("artifacts"))
            .with_library(DASHBOARDS, dashboards.clone());
        assert_eq!(
            without
                .open_in_app("dashboards/sales.html")
                .await
                .expect_err("no browser")
                .code,
            AppErrorCode::NotImplemented
        );
        let store = without.with_pages(pages.clone());
        store
            .open_in_app("dashboards/sales.html")
            .await
            .expect("opens");
        store
            .open_in_app("dashboards/sales.html")
            .await
            .expect("opens again");
        let error = store
            .open_in_app("dashboards/data.xlsx")
            .await
            .expect_err("not a page");
        assert_eq!(error.message, crate::artifact_pages::NOT_A_PAGE);
        let opened = pages.0.lock().expect("reads").clone();
        assert_eq!(opened.len(), 2);
        assert_eq!(opened[0], opened[1]);
        assert!(opened[0].starts_with("http://127.0.0.1:"));
        assert!(opened[0].ends_with("/dashboards/sales.html"));
    }
}
