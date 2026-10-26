use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use crate::platform::long_path::extended;

pub const CONVERSATIONS_DIR: &str = "conversations";
pub const WORK_DIR: &str = "work";
pub const OUTPUTS_DIR: &str = "outputs";
pub const SNAPSHOTS_DIR: &str = "snapshots";
pub const MANIFEST_FILE: &str = "manifest.json";

pub fn is_owner(owner: &str) -> bool {
    !owner.is_empty()
        && owner.len() <= 64
        && owner
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputEntry {
    pub name: String,
    pub path: PathBuf,
    pub run_id: String,
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

#[derive(Debug, Clone)]
pub struct ConversationFolders {
    root: PathBuf,
    manifests: Arc<Mutex<()>>,
}

impl ConversationFolders {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            manifests: Arc::new(Mutex::new(())),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn folder(&self, owner: &str) -> Option<PathBuf> {
        is_owner(owner).then(|| self.root.join(owner))
    }

    pub fn work(&self, owner: &str) -> Option<PathBuf> {
        self.folder(owner).map(|folder| folder.join(WORK_DIR))
    }

    pub fn snapshots(&self, owner: &str, run: &str) -> Option<PathBuf> {
        let run = self.folder(run).and(Some(run))?;
        self.folder(owner)
            .map(|folder| folder.join(SNAPSHOTS_DIR).join(run))
    }

    fn manifest(&self, owner: &str) -> Option<PathBuf> {
        self.folder(owner)
            .map(|folder| folder.join(OUTPUTS_DIR).join(MANIFEST_FILE))
    }

    pub fn remove(&self, owner: &str) {
        if let Some(folder) = self.folder(owner) {
            let _ = std::fs::remove_dir_all(extended(&folder));
        }
    }

    pub fn outputs(&self, owner: &str) -> Vec<OutputEntry> {
        self.manifest(owner)
            .and_then(|path| std::fs::read(extended(&path)).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn write_outputs(&self, owner: &str, entries: &[OutputEntry]) -> bool {
        let Some(path) = self.manifest(owner) else {
            return false;
        };
        if entries.is_empty() {
            let _ = std::fs::remove_file(extended(&path));
            return true;
        }
        let Some(parent) = path.parent() else {
            return false;
        };
        let Ok(bytes) = serde_json::to_vec_pretty(entries) else {
            return false;
        };
        std::fs::create_dir_all(extended(parent)).is_ok()
            && std::fs::write(extended(&path), bytes).is_ok()
    }

    pub fn record_outputs(&self, owner: &str, run: &str, files: &[PathBuf]) -> Vec<OutputEntry> {
        if files.is_empty() || !is_owner(owner) {
            return Vec::new();
        }
        let _guard = self
            .manifests
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut entries = self.outputs(owner);
        let mut taken: HashSet<String> = entries
            .iter()
            .map(|entry| entry.name.to_lowercase())
            .collect();
        let mut added = Vec::new();
        for file in files {
            if let Some(existing) = entries.iter_mut().find(|entry| entry.path == *file) {
                existing.run_id = run.to_owned();
                added.push(existing.clone());
                continue;
            }
            let Some(original) = file
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
            else {
                continue;
            };
            let entry = OutputEntry {
                name: unique_name(&original, &mut taken),
                path: file.clone(),
                run_id: run.to_owned(),
            };
            entries.push(entry.clone());
            added.push(entry);
        }
        if self.write_outputs(owner, &entries) {
            added
        } else {
            Vec::new()
        }
    }

    pub fn output(&self, owner: &str, name: &str) -> Option<PathBuf> {
        self.outputs(owner)
            .into_iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.path)
    }

    pub fn forget_run(&self, owner: &str, run: &str) {
        if let Some(snapshots) = self.snapshots(owner, run) {
            let _ = std::fs::remove_dir_all(extended(&snapshots));
            if let Some(parent) = snapshots.parent() {
                let _ = std::fs::remove_dir(extended(parent));
            }
        }
        {
            let _guard = self
                .manifests
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let entries = self.outputs(owner);
            let kept: Vec<OutputEntry> = entries
                .iter()
                .filter(|entry| entry.run_id != run)
                .cloned()
                .collect();
            if kept.len() != entries.len() {
                self.write_outputs(owner, &kept);
                if kept.is_empty()
                    && let Some(manifest) = self.manifest(owner)
                    && let Some(outputs) = manifest.parent()
                {
                    let _ = std::fs::remove_dir(extended(outputs));
                }
            }
        }
        if let Some(folder) = self.folder(owner) {
            let _ = std::fs::remove_dir(extended(&folder));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("creates");
        std::fs::write(path, text).expect("writes");
    }

    #[test]
    fn a_conversation_owns_one_folder_with_work_outputs_and_snapshots() {
        let root = tempfile::tempdir().expect("a root");
        let folders = ConversationFolders::new(root.path().to_path_buf());
        let folder = root.path().join("c1");
        assert_eq!(folders.work("c1"), Some(folder.join("work")));
        assert_eq!(
            folders.snapshots("c1", "r1"),
            Some(folder.join("snapshots").join("r1"))
        );
        assert_eq!(folders.work("../c1"), None);
        assert_eq!(folders.snapshots("c1", ".."), None);
        assert_eq!(folders.work(""), None);
    }

    #[test]
    fn outputs_are_recorded_by_reference_with_unique_names_and_no_copy() {
        let root = tempfile::tempdir().expect("a root");
        let folders = ConversationFolders::new(root.path().join("conversations"));
        let work = folders.work("c1").expect("a work folder");
        let first = work.join("a").join("report.docx");
        let second = work.join("b").join("Report.docx");
        write(&first, "one");
        write(&second, "two");

        let recorded = folders.record_outputs("c1", "r1", &[first.clone(), second.clone()]);
        let again = folders.record_outputs("c1", "r2", std::slice::from_ref(&first));

        let names: Vec<&str> = recorded.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["report.docx", "Report (2).docx"]);
        assert_eq!(again[0].name, "report.docx");
        assert_eq!(folders.outputs("c1").len(), 2);
        assert_eq!(folders.output("c1", "Report (2).docx"), Some(second));
        let stored: Vec<_> = std::fs::read_dir(root.path().join("conversations/c1/outputs"))
            .expect("lists")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(stored, [MANIFEST_FILE]);
        assert!(folders.record_outputs("..", "r1", &[first]).is_empty());
    }

    #[test]
    fn forgetting_a_run_drops_its_snapshots_and_outputs_and_an_empty_folder() {
        let root = tempfile::tempdir().expect("a root");
        let folders = ConversationFolders::new(root.path().to_path_buf());
        let file = root.path().join("elsewhere").join("deck.pptx");
        write(&file, "deck");
        let snapshot = folders.snapshots("c1", "r1").expect("snapshots");
        write(&snapshot.join("batch").join("a.txt"), "before");
        write(
            &folders
                .snapshots("c1", "r2")
                .expect("snapshots")
                .join("b.txt"),
            "keep",
        );
        folders.record_outputs("c1", "r1", std::slice::from_ref(&file));

        folders.forget_run("c1", "r1");

        assert!(!snapshot.exists());
        assert!(folders.outputs("c1").is_empty());
        assert!(!root.path().join("c1/outputs").exists());
        assert!(file.is_file());
        assert!(root.path().join("c1/snapshots/r2/b.txt").is_file());

        folders.forget_run("c1", "r2");
        assert!(!root.path().join("c1").exists());
    }
}
