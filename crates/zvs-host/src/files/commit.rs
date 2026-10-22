use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;

use super::plan::PlannedFile;
use crate::platform::ids::new_id;
use crate::platform::long_path::extended;

pub const MANIFEST: &str = "manifest.json";

pub type Fault = Arc<dyn Fn(usize, &Path) -> io::Result<()> + Send + Sync>;

#[derive(Debug)]
pub struct CommitFailure {
    pub index: usize,
    pub path: PathBuf,
    pub error: io::Error,
    pub restored: bool,
}

pub fn snapshot(folder: &Path, files: &[PlannedFile]) -> io::Result<()> {
    std::fs::create_dir_all(extended(folder))?;
    let mut manifest = Vec::with_capacity(files.len());
    for (index, file) in files.iter().enumerate() {
        let saved = match &file.before {
            Some(bytes) => {
                let name = index.to_string();
                std::fs::write(extended(&folder.join(&name)), bytes)?;
                Some(name)
            }
            None => None,
        };
        manifest.push(json!({
            "path": file.path.to_string_lossy(),
            "snapshot": saved,
            "deleted": file.after.is_none(),
        }));
    }
    std::fs::write(
        extended(&folder.join(MANIFEST)),
        serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?,
    )
}

fn temporary(path: &Path) -> io::Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?
        .to_string_lossy();
    Ok(parent.join(format!(".{name}.{}.zvs-tmp", new_id().simple())))
}

pub fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let staged = temporary(path)?;
    let written = (|| {
        let mut file = std::fs::File::create(extended(&staged))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(extended(&staged), extended(path))
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(extended(&staged));
    }
    written
}

fn ensure_folders(folder: &Path, created: &mut Vec<PathBuf>) -> io::Result<()> {
    let missing: Vec<&Path> = folder
        .ancestors()
        .take_while(|ancestor| !extended(ancestor).exists())
        .collect();
    for ancestor in missing.into_iter().rev() {
        std::fs::create_dir(extended(ancestor))?;
        created.push(ancestor.to_path_buf());
    }
    Ok(())
}

fn apply(file: &PlannedFile, created: &mut Vec<PathBuf>) -> io::Result<()> {
    match &file.after {
        Some(bytes) => {
            if let Some(parent) = file.path.parent() {
                ensure_folders(parent, created)?;
            }
            replace(&file.path, bytes)
        }
        None => std::fs::remove_file(extended(&file.path)),
    }
}

fn restore(done: &[PlannedFile], created: &[PathBuf]) -> bool {
    let mut restored = true;
    for file in done.iter().rev() {
        let outcome = match &file.before {
            Some(bytes) => replace(&file.path, bytes),
            None => match std::fs::remove_file(extended(&file.path)) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                other => other,
            },
        };
        restored &= outcome.is_ok();
    }
    for folder in created.iter().rev() {
        let _ = std::fs::remove_dir(extended(folder));
    }
    restored
}

pub fn commit(files: &[PlannedFile], fault: Option<&Fault>) -> Result<(), CommitFailure> {
    let mut created: Vec<PathBuf> = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let outcome = fault
            .map_or(Ok(()), |fault| fault(index, &file.path))
            .and_then(|()| apply(file, &mut created));
        if let Err(error) = outcome {
            let restored = restore(&files[..=index], &created);
            return Err(CommitFailure {
                index,
                path: file.path.clone(),
                error,
                restored,
            });
        }
    }
    Ok(())
}
