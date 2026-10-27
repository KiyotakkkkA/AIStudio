use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use zvs_contract::{AppError, AppErrorCode};

use crate::artifacts::ArtifactStore;
use crate::platform::long_path::extended;

pub const CHAT_DIR_ENV: &str = "ZVS_CHAT_DIR";
pub const WORKING_PREFIX: char = '_';
pub const CHAT_FOLDER_MAX_BYTES: u64 = 200 * 1024 * 1024;
pub const CHAT_FOLDER_MAX_FILES: usize = 1000;

pub const NO_CHAT: &str = "Рабочая папка доступна только в диалоге или агентной сессии";
pub const CHAT_FOLDER_FULL: &str = "Рабочая папка диалога заполнена";
pub const CHAT_FOLDER_FAILED: &str = "Не удалось открыть рабочую папку диалога";
pub const BRANCH_FILES_FAILED: &str = "Не удалось скопировать файлы диалога в ветку";

pub fn is_working(relative: &Path) -> bool {
    relative.components().any(|component| {
        matches!(component, Component::Normal(name) if name.to_string_lossy().starts_with(WORKING_PREFIX))
    })
}

pub fn chat_dir(store: &ArtifactStore, owner: &str) -> Result<PathBuf, AppError> {
    let folder = store
        .folders()
        .work(owner)
        .ok_or_else(|| AppError::new(AppErrorCode::ValidationFailed, NO_CHAT))?;
    std::fs::create_dir_all(extended(&folder)).map_err(|error| {
        AppError::new(AppErrorCode::Unknown, CHAT_FOLDER_FAILED)
            .with_detail("raw", error.to_string())
    })?;
    Ok(folder)
}

pub fn copy_chat_dir(store: &ArtifactStore, from: &str, to: &str) -> Result<(), AppError> {
    store.folders().copy(from, to).map_err(|error| {
        store.folders().remove(to);
        AppError::new(AppErrorCode::Unknown, BRANCH_FILES_FAILED)
            .with_detail("raw", error.to_string())
    })
}

pub fn remove_chat_dir(store: &ArtifactStore, owner: &str) {
    store.folders().remove(owner);
}

fn walk(folder: &Path, base: &Path, out: &mut Vec<(String, u64)>) {
    let Ok(entries) = std::fs::read_dir(extended(folder)) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = folder.join(entry.file_name());
        if kind.is_dir() {
            walk(&path, base, out);
        } else if kind.is_file() {
            let relative = path
                .strip_prefix(base)
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            let size = entry.metadata().map_or(0, |metadata| metadata.len());
            out.push((relative, size));
        }
    }
}

pub fn chat_files(folder: &Path) -> Vec<(String, u64)> {
    let mut files = Vec::new();
    walk(folder, folder, &mut files);
    files
}

pub fn within_quota(folder: &Path, changes: &[(PathBuf, Option<u64>)]) -> Result<(), AppError> {
    let mut sizes: BTreeMap<PathBuf, u64> = chat_files(folder)
        .into_iter()
        .map(|(relative, size)| (folder.join(relative), size))
        .collect();
    let before = sizes.len();
    for (path, size) in changes {
        match size {
            Some(size) => sizes.insert(path.clone(), *size),
            None => sizes.remove(path),
        };
    }
    let total: u64 = sizes.values().sum();
    if total > CHAT_FOLDER_MAX_BYTES
        || (sizes.len() > before && sizes.len() > CHAT_FOLDER_MAX_FILES)
    {
        return Err(AppError::new(AppErrorCode::LimitExceeded, CHAT_FOLDER_FULL));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
