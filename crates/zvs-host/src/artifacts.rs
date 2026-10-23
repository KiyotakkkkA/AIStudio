use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use zvs_contract::{AppError, AppErrorCode, ArtifactDto, ArtifactSaveResult};

use crate::platform::external::{PathRevealer, system_path_revealer};
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
    let after = snapshot(directory);
    let mut found: Vec<PathBuf> = after
        .iter()
        .filter(|(path, state)| before.get(*path) != Some(*state))
        .map(|(path, _)| path.clone())
        .collect();
    found.sort();
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
    picker: Option<SharedPicker>,
    reveal: PathRevealer,
}

impl ArtifactStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            picker: None,
            reveal: system_path_revealer(),
        }
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
        let path = self.root.join(run).join(name);
        if path.parent().and_then(Path::parent) != Some(self.root.as_path()) {
            return Err(not_found(id));
        }
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
    use crate::testing::FakePicker;

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
        assert_eq!(names, ["draft.xlsx", "Отчёт.pdf", "final.pptx"]);
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
}
