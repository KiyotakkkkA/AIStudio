use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::json;
use zvs_contract::{AppError, AppErrorCode, FILE_DIFF_MAX, FileChangeKind, FileDiffDto};

use super::diff::diff_lines;
use super::text::{Decoded, TextFormat, decode, encode, fresh_format, normalize};
use crate::platform::long_path::extended;

pub const FILE_MISSING: &str = "Файл не найден";
pub const NOT_A_FILE: &str = "Путь указывает на папку, а не на файл";
pub const BINARY_REFUSED: &str = "Двоичные файлы и файлы не в кодировке UTF-8 изменять нельзя";
pub const EDIT_NOT_FOUND: &str = "Фрагмент для замены не найден в файле";
pub const EDIT_AMBIGUOUS: &str =
    "Фрагмент для замены встречается в файле несколько раз: уточните его";
pub const EDIT_EMPTY: &str = "Фрагмент для замены не может быть пустым";
pub const MOVE_TARGET_EXISTS: &str =
    "Файл назначения уже существует: сначала удалите его в этом же наборе изменений";
pub const MOVE_SAME: &str = "Исходный путь и путь назначения совпадают";
pub const READ_FAILED: &str = "Не удалось прочитать файл";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub old: String,
    pub new: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Write { path: PathBuf, content: String },
    Edit { path: PathBuf, edits: Vec<Edit> },
    Move { from: PathBuf, to: PathBuf },
    Delete { path: PathBuf },
}

pub trait Disk {
    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>>;
}

pub struct RealDisk;

impl Disk for RealDisk {
    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        let target = extended(path);
        match std::fs::metadata(&target) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
            Ok(metadata) if metadata.is_dir() => Err(io::Error::from(io::ErrorKind::IsADirectory)),
            Ok(_) => std::fs::read(&target).map(Some),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    pub path: PathBuf,
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub files: Vec<PlannedFile>,
    pub changes: Vec<FileDiffDto>,
    pub fresh: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
enum Slot {
    Absent,
    Binary,
    Text {
        text: String,
        format: TextFormat,
        origin: Option<PathBuf>,
    },
}

fn key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn refused(message: &str, index: usize, path: &Path) -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, message)
        .with_detail("change", json!(index))
        .with_detail("path", path_text(path))
}

struct Overlay<'a> {
    disk: &'a dyn Disk,
    stored: HashMap<String, (PathBuf, Option<Vec<u8>>)>,
    slots: HashMap<String, Slot>,
    order: Vec<String>,
    fresh: Vec<String>,
}

impl Overlay<'_> {
    fn stored(&mut self, index: usize, path: &Path) -> Result<Option<Vec<u8>>, AppError> {
        let name = key(path);
        if let Some((_, bytes)) = self.stored.get(&name) {
            return Ok(bytes.clone());
        }
        let bytes = self.disk.read(path).map_err(|error| {
            if error.kind() == io::ErrorKind::IsADirectory {
                refused(NOT_A_FILE, index, path)
            } else {
                refused(READ_FAILED, index, path)
            }
        })?;
        self.stored
            .insert(name, (path.to_path_buf(), bytes.clone()));
        Ok(bytes)
    }

    fn slot(&mut self, index: usize, path: &Path) -> Result<Slot, AppError> {
        let name = key(path);
        if let Some(slot) = self.slots.get(&name) {
            return Ok(slot.clone());
        }
        Ok(match self.stored(index, path)? {
            None => Slot::Absent,
            Some(bytes) => match decode(&bytes) {
                Decoded::Binary => Slot::Binary,
                Decoded::Text { text, format } => Slot::Text {
                    text,
                    format,
                    origin: Some(path.to_path_buf()),
                },
            },
        })
    }

    fn set(&mut self, path: &Path, slot: Slot) {
        let name = key(path);
        if !self.order.contains(&name) {
            self.order.push(name.clone());
        }
        self.slots.insert(name, slot);
    }

    fn touch(&mut self, origin: Option<&PathBuf>) {
        if let Some(origin) = origin {
            let name = key(origin);
            if !self.fresh.contains(&name) {
                self.fresh.push(name);
            }
        }
    }

    fn apply(&mut self, index: usize, change: &Change) -> Result<(), AppError> {
        match change {
            Change::Write { path, content } => {
                let format = match self.slot(index, path)? {
                    Slot::Binary => return Err(refused(BINARY_REFUSED, index, path)),
                    Slot::Absent => fresh_format(content),
                    Slot::Text { format, origin, .. } => {
                        self.touch(origin.as_ref());
                        format
                    }
                };
                self.set(
                    path,
                    Slot::Text {
                        text: normalize(content),
                        format,
                        origin: None,
                    },
                );
            }
            Change::Edit { path, edits } => {
                let (mut text, format, origin) = match self.slot(index, path)? {
                    Slot::Absent => return Err(refused(FILE_MISSING, index, path)),
                    Slot::Binary => return Err(refused(BINARY_REFUSED, index, path)),
                    Slot::Text {
                        text,
                        format,
                        origin,
                    } => (text, format, origin),
                };
                self.touch(origin.as_ref());
                for (position, edit) in edits.iter().enumerate() {
                    let old = normalize(&edit.old);
                    let detail = |message: &str| {
                        refused(message, index, path).with_detail("edit", json!(position))
                    };
                    if old.is_empty() {
                        return Err(detail(EDIT_EMPTY));
                    }
                    match text.matches(old.as_str()).take(2).count() {
                        0 => return Err(detail(EDIT_NOT_FOUND)),
                        1 => text = text.replacen(old.as_str(), &normalize(&edit.new), 1),
                        _ => return Err(detail(EDIT_AMBIGUOUS)),
                    }
                }
                self.set(
                    path,
                    Slot::Text {
                        text,
                        format,
                        origin,
                    },
                );
            }
            Change::Move { from, to } => {
                if key(from) == key(to) {
                    return Err(refused(MOVE_SAME, index, from));
                }
                let source = match self.slot(index, from)? {
                    Slot::Absent => return Err(refused(FILE_MISSING, index, from)),
                    Slot::Binary => return Err(refused(BINARY_REFUSED, index, from)),
                    text @ Slot::Text { .. } => text,
                };
                if !matches!(self.slot(index, to)?, Slot::Absent) {
                    return Err(refused(MOVE_TARGET_EXISTS, index, to));
                }
                self.set(to, source);
                self.set(from, Slot::Absent);
            }
            Change::Delete { path } => match self.slot(index, path)? {
                Slot::Absent => return Err(refused(FILE_MISSING, index, path)),
                Slot::Binary => return Err(refused(BINARY_REFUSED, index, path)),
                Slot::Text { origin, .. } => {
                    self.touch(origin.as_ref());
                    self.set(path, Slot::Absent);
                }
            },
        }
        Ok(())
    }
}

fn text_of(bytes: Option<&Vec<u8>>) -> String {
    match bytes.map(|bytes| decode(bytes)) {
        Some(Decoded::Text { text, .. }) => text,
        _ => String::new(),
    }
}

fn change(
    path: &Path,
    from: Option<&Path>,
    kind: FileChangeKind,
    before: &str,
    after: &str,
) -> FileDiffDto {
    let diff = diff_lines(before, after, FILE_DIFF_MAX);
    FileDiffDto {
        path: path_text(path),
        from: from.map(path_text),
        kind,
        added: diff.added,
        removed: diff.removed,
        diff: diff.text,
        truncated: diff.truncated,
    }
}

pub fn plan(changes: &[Change], disk: &dyn Disk) -> Result<Plan, AppError> {
    let mut overlay = Overlay {
        disk,
        stored: HashMap::new(),
        slots: HashMap::new(),
        order: Vec::new(),
        fresh: Vec::new(),
    };
    for (index, change) in changes.iter().enumerate() {
        overlay.apply(index, change)?;
    }
    let mut files: Vec<PlannedFile> = Vec::new();
    let mut finals: Vec<(PathBuf, Option<PathBuf>, Option<String>)> = Vec::new();
    for name in &overlay.order {
        let (path, before) = overlay
            .stored
            .get(name)
            .cloned()
            .unwrap_or_else(|| (PathBuf::from(name), None));
        let (after, origin, text) = match overlay.slots.get(name) {
            Some(Slot::Text {
                text,
                format,
                origin,
            }) => (
                Some(encode(text, *format)),
                origin.clone(),
                Some(text.clone()),
            ),
            _ => (None, None, None),
        };
        if before == after {
            continue;
        }
        files.push(PlannedFile {
            path: path.clone(),
            before,
            after,
        });
        finals.push((path, origin, text));
    }
    let deleted = |origin: &Path| {
        files.iter().any(|file| {
            key(&file.path) == key(origin) && file.before.is_some() && file.after.is_none()
        })
    };
    let mut moved: Vec<String> = Vec::new();
    let mut reported: Vec<FileDiffDto> = Vec::new();
    for (file, (path, origin, text)) in files.iter().zip(&finals) {
        let after = text.clone().unwrap_or_default();
        if file.before.is_none()
            && let Some(origin) = origin.as_ref().filter(|origin| key(origin) != key(path))
            && deleted(origin)
        {
            let source = overlay
                .stored
                .get(&key(origin))
                .and_then(|(_, bytes)| bytes.as_ref());
            moved.push(key(origin));
            reported.push(change(
                path,
                Some(origin),
                FileChangeKind::Move,
                &text_of(source),
                &after,
            ));
            continue;
        }
        let before = text_of(file.before.as_ref());
        let kind = match (&file.before, &file.after) {
            (None, _) => FileChangeKind::Create,
            (Some(_), None) => FileChangeKind::Delete,
            (Some(_), Some(_)) => FileChangeKind::Modify,
        };
        reported.push(change(path, None, kind, &before, &after));
    }
    let reported = reported
        .into_iter()
        .filter(|entry| {
            entry.kind != FileChangeKind::Delete || !moved.contains(&key(Path::new(&entry.path)))
        })
        .collect();
    let changed: Vec<String> = files.iter().map(|file| key(&file.path)).collect();
    let fresh = overlay
        .fresh
        .iter()
        .filter(|name| changed.contains(name))
        .filter_map(|name| overlay.stored.get(name).map(|(path, _)| path.clone()))
        .collect();
    Ok(Plan {
        files,
        changes: reported,
        fresh,
    })
}
