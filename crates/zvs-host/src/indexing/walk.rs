use std::cmp::Ordering;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::Path;

use tokio_util::sync::CancellationToken;
use zvs_contract::AppError;

use super::patterns::{accepts, normalize_relative};
use super::{NOTES_MAX, cancelled};
use crate::data::schema::VectorSourceRow;
use crate::platform::long_path::extended;
use crate::services::vector_sources::resolve_path;

pub const DEFAULT_MAX_FILES: usize = 200_000;
pub const DEFAULT_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFile {
    pub path: String,
    pub bytes: u64,
    pub source_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalkLimits {
    pub max_files: usize,
    pub max_file_bytes: u64,
}

impl Default for WalkLimits {
    fn default() -> Self {
        Self {
            max_files: DEFAULT_MAX_FILES,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WalkOutcome {
    pub files: Vec<DiscoveredFile>,
    pub notes: Vec<String>,
    pub unreachable: Vec<String>,
    pub truncated: bool,
}

fn reason(error: &std::io::Error) -> String {
    match error.kind() {
        ErrorKind::NotFound => "путь не найден".to_owned(),
        ErrorKind::PermissionDenied => "нет доступа".to_owned(),
        ErrorKind::InvalidFilename => "слишком длинный путь".to_owned(),
        _ => error.to_string(),
    }
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn locale_order(left: &str, right: &str) -> Ordering {
    left.to_lowercase()
        .cmp(&right.to_lowercase())
        .then_with(|| left.cmp(right))
}

struct Walk {
    limits: WalkLimits,
    found: HashMap<String, DiscoveredFile>,
    outcome: WalkOutcome,
}

impl Walk {
    fn note(&mut self, message: String) {
        if self.outcome.notes.len() < NOTES_MAX {
            self.outcome.notes.push(message);
        }
    }

    fn source(
        &mut self,
        source: &VectorSourceRow,
        cancel: &CancellationToken,
    ) -> Result<(), AppError> {
        let working_directory = std::env::current_dir().unwrap_or_default();
        let root = resolve_path(&source.path, &working_directory);
        let root_text = display(&root);
        let entry = match std::fs::metadata(extended(&root)) {
            Ok(entry) => entry,
            Err(error) => {
                self.note(format!(
                    "Источник недоступен: {} — {}",
                    source.path,
                    reason(&error)
                ));
                self.outcome.unreachable.push(root_text);
                return Ok(());
            }
        };
        if entry.is_file() {
            let name = root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !accepts(&name, &source.include, &source.exclude) {
                return Ok(());
            }
            if entry.len() > self.limits.max_file_bytes {
                self.note(format!("Файл слишком велик: {root_text}"));
                return Ok(());
            }
            self.found.insert(
                root_text.clone(),
                DiscoveredFile {
                    path: root_text,
                    bytes: entry.len(),
                    source_id: source.id.clone(),
                },
            );
            return Ok(());
        }
        if !entry.is_dir() {
            self.note(format!(
                "Источник не является файлом или папкой: {}",
                source.path
            ));
            return Ok(());
        }
        let mut queue = vec![root.clone()];
        while let Some(directory) = queue.pop() {
            if cancel.is_cancelled() {
                return Err(cancelled());
            }
            let entries = match std::fs::read_dir(extended(&directory)) {
                Ok(entries) => entries,
                Err(error) => {
                    self.note(format!(
                        "Папка недоступна: {} — {}",
                        display(&directory),
                        reason(&error)
                    ));
                    self.outcome.unreachable.push(display(&directory));
                    continue;
                }
            };
            for child in entries {
                let Ok(child) = child else {
                    continue;
                };
                let absolute = directory.join(child.file_name());
                let relative = absolute
                    .strip_prefix(&root)
                    .map(display)
                    .unwrap_or_default();
                let relative = normalize_relative(&relative);
                let Ok(kind) = child.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    if source.recursive && accepts(&relative, &[], &source.exclude) {
                        queue.push(absolute);
                    }
                    continue;
                }
                if !kind.is_file() || !accepts(&relative, &source.include, &source.exclude) {
                    continue;
                }
                let absolute_text = display(&absolute);
                if self.found.contains_key(&absolute_text) {
                    continue;
                }
                let info = match std::fs::metadata(extended(&absolute)) {
                    Ok(info) => info,
                    Err(error) => {
                        self.note(format!(
                            "Файл недоступен: {absolute_text} — {}",
                            reason(&error)
                        ));
                        self.outcome.unreachable.push(absolute_text);
                        continue;
                    }
                };
                if info.len() > self.limits.max_file_bytes {
                    self.note(format!("Файл слишком велик: {absolute_text}"));
                    continue;
                }
                self.found.insert(
                    absolute_text.clone(),
                    DiscoveredFile {
                        path: absolute_text,
                        bytes: info.len(),
                        source_id: source.id.clone(),
                    },
                );
                if self.found.len() >= self.limits.max_files {
                    self.note(format!(
                        "Достигнут предел в {} файлов",
                        self.limits.max_files
                    ));
                    self.outcome.truncated = true;
                    queue.clear();
                    break;
                }
            }
        }
        Ok(())
    }
}

pub fn walk_sources_blocking(
    sources: &[VectorSourceRow],
    limits: WalkLimits,
    cancel: &CancellationToken,
) -> Result<WalkOutcome, AppError> {
    let mut walk = Walk {
        limits,
        found: HashMap::new(),
        outcome: WalkOutcome::default(),
    };
    for source in sources {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        walk.source(source, cancel)?;
    }
    let mut files: Vec<DiscoveredFile> = walk.found.into_values().collect();
    files.sort_by(|left, right| locale_order(&left.path, &right.path));
    walk.outcome.files = files;
    Ok(walk.outcome)
}

pub async fn walk_sources(
    sources: Vec<VectorSourceRow>,
    limits: WalkLimits,
    cancel: CancellationToken,
) -> Result<WalkOutcome, AppError> {
    tokio::task::spawn_blocking(move || walk_sources_blocking(&sources, limits, &cancel))
        .await
        .map_err(|_| super::worker_failed())?
}

#[cfg(test)]
mod tests {
    use zvs_contract::{AppErrorCode, VectorSourceKind};

    use super::*;
    use crate::testing::temporary_directory;

    fn source(id: &str, path: &Path, include: &[&str], exclude: &[&str]) -> VectorSourceRow {
        VectorSourceRow {
            id: id.to_owned(),
            store_id: "store".to_owned(),
            kind: VectorSourceKind::Folder,
            path: display(path),
            include: include.iter().map(|value| (*value).to_owned()).collect(),
            exclude: exclude.iter().map(|value| (*value).to_owned()).collect(),
            recursive: true,
            created_at: 1,
        }
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(extended(path.parent().expect("a parent"))).expect("creates");
        std::fs::write(extended(path), text).expect("writes");
    }

    fn walk(sources: &[VectorSourceRow], limits: WalkLimits) -> WalkOutcome {
        walk_sources_blocking(sources, limits, &CancellationToken::new()).expect("walks")
    }

    fn paths(outcome: &WalkOutcome) -> Vec<String> {
        outcome.files.iter().map(|file| file.path.clone()).collect()
    }

    #[test]
    fn a_folder_is_walked_recursively_and_sorted() {
        let directory = temporary_directory("walk-");
        let root = directory.join("corpus");
        write(&root.join("b.md"), "b");
        write(&root.join("nested").join("a.txt"), "a");
        write(&root.join("A.md"), "A");
        let outcome = walk(&[source("s", &root, &[], &[])], WalkLimits::default());
        assert_eq!(
            paths(&outcome),
            [
                display(&root.join("A.md")),
                display(&root.join("b.md")),
                display(&root.join("nested").join("a.txt")),
            ]
        );
        assert!(outcome.files.iter().all(|file| file.source_id == "s"));
        assert_eq!(outcome.files[0].bytes, 1);
        assert!(!outcome.truncated);
        assert!(outcome.notes.is_empty());
    }

    #[test]
    fn an_excluded_folder_is_never_entered() {
        let directory = temporary_directory("walk-");
        let root = directory.join("corpus");
        write(&root.join("docs").join("guide.md"), "guide");
        write(&root.join("docs").join("skip.txt"), "skip");
        write(
            &root.join("node_modules").join("dep").join("index.md"),
            "dep",
        );
        let outcome = walk(
            &[source("s", &root, &["**/*.md"], &["node_modules"])],
            WalkLimits::default(),
        );
        assert_eq!(
            paths(&outcome),
            [display(&root.join("docs").join("guide.md"))]
        );
    }

    #[test]
    fn a_folder_that_is_not_recursive_keeps_to_its_own_files() {
        let directory = temporary_directory("walk-");
        let root = directory.join("corpus");
        write(&root.join("top.md"), "top");
        write(&root.join("nested").join("deep.md"), "deep");
        let mut flat = source("s", &root, &[], &[]);
        flat.recursive = false;
        assert_eq!(
            paths(&walk(&[flat], WalkLimits::default())),
            [display(&root.join("top.md"))]
        );
    }

    #[test]
    fn a_missing_source_is_noted_and_marked_unreachable() {
        let directory = temporary_directory("walk-");
        let missing = directory.join("unplugged");
        let outcome = walk(&[source("s", &missing, &[], &[])], WalkLimits::default());
        assert!(outcome.files.is_empty());
        assert_eq!(outcome.unreachable, [display(&missing)]);
        assert_eq!(outcome.notes.len(), 1);
        assert!(outcome.notes[0].starts_with("Источник недоступен"));
        assert!(outcome.notes[0].contains("путь не найден"));
    }

    #[test]
    fn a_file_source_is_taken_alone_and_checked_against_its_patterns() {
        let directory = temporary_directory("walk-");
        let file = directory.join("single.md");
        write(&file, "one");
        let outcome = walk(&[source("s", &file, &[], &[])], WalkLimits::default());
        assert_eq!(paths(&outcome), [display(&file)]);
        let refused = walk(
            &[source("s", &file, &["*.txt"], &[])],
            WalkLimits::default(),
        );
        assert!(refused.files.is_empty());
    }

    #[test]
    fn a_file_over_the_size_limit_is_noted_and_left_out() {
        let directory = temporary_directory("walk-");
        let root = directory.join("corpus");
        write(&root.join("big.md"), "0123456789");
        write(&root.join("small.md"), "0");
        let outcome = walk(
            &[source("s", &root, &[], &[])],
            WalkLimits {
                max_file_bytes: 5,
                ..WalkLimits::default()
            },
        );
        assert_eq!(paths(&outcome), [display(&root.join("small.md"))]);
        assert_eq!(
            outcome.notes,
            [format!(
                "Файл слишком велик: {}",
                display(&root.join("big.md"))
            )]
        );
    }

    #[test]
    fn the_file_limit_cuts_the_walk_short_and_says_so() {
        let directory = temporary_directory("walk-");
        let root = directory.join("corpus");
        for index in 0..5 {
            write(&root.join(format!("{index}.md")), "x");
        }
        let outcome = walk(
            &[source("s", &root, &[], &[])],
            WalkLimits {
                max_files: 3,
                ..WalkLimits::default()
            },
        );
        assert_eq!(outcome.files.len(), 3);
        assert!(outcome.truncated);
        assert_eq!(outcome.notes, ["Достигнут предел в 3 файлов"]);
    }

    #[test]
    fn two_sources_over_one_file_list_it_once() {
        let directory = temporary_directory("walk-");
        let root = directory.join("corpus");
        write(&root.join("a.md"), "a");
        let outcome = walk(
            &[
                source("first", &root, &[], &[]),
                source("second", &root, &[], &[]),
            ],
            WalkLimits::default(),
        );
        assert_eq!(outcome.files.len(), 1);
        assert_eq!(outcome.files[0].source_id, "first");
    }

    #[test]
    fn a_path_longer_than_the_windows_limit_is_walked() {
        let directory = temporary_directory("walk-");
        let root = directory.join("corpus");
        let mut deep = root.clone();
        while display(&deep).len() < 300 {
            deep = deep.join("a-rather-long-folder-name-for-the-walk");
        }
        let file = deep.join("note.md");
        write(&file, "deep text");
        let outcome = walk(&[source("s", &root, &[], &[])], WalkLimits::default());
        assert_eq!(paths(&outcome), [display(&file)]);
        assert!(display(&file).len() > 260);
    }

    #[test]
    fn a_cancelled_walk_stops_with_a_cancellation() {
        let directory = temporary_directory("walk-");
        let root = directory.join("corpus");
        write(&root.join("a.md"), "a");
        let cancel = CancellationToken::new();
        cancel.cancel();
        let error = walk_sources_blocking(
            &[source("s", &root, &[], &[])],
            WalkLimits::default(),
            &cancel,
        )
        .expect_err("cancelled");
        assert_eq!(error.code, AppErrorCode::RunCancelled);
    }
}
