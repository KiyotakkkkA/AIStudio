use std::path::{Path, PathBuf};

use super::BootError;

const PART_SUFFIX: &str = ".migrating";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Copy,
    Link,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transfer {
    pub from: PathBuf,
    pub to: PathBuf,
    pub mode: Mode,
    pub skip: Vec<PathBuf>,
}

impl Transfer {
    pub fn new(from: PathBuf, to: PathBuf, mode: Mode) -> Self {
        Self {
            from,
            to,
            mode,
            skip: Vec::new(),
        }
    }

    #[must_use]
    pub fn skipping(mut self, skip: &[&[&str]]) -> Self {
        self.skip = skip
            .iter()
            .map(|parts| parts.iter().collect::<PathBuf>())
            .collect();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    pub from: PathBuf,
    pub to: PathBuf,
    pub size: u64,
    pub mode: Mode,
}

fn io(context: &Path, error: &std::io::Error) -> BootError {
    BootError::Io(format!("{}: {error}", context.display()))
}

fn walk(
    root: &Path,
    relative: &Path,
    transfer: &Transfer,
    planned: &mut Vec<PlannedFile>,
) -> Result<(), BootError> {
    let current = root.join(relative);
    let entries = std::fs::read_dir(&current).map_err(|error| io(&current, &error))?;
    for entry in entries {
        let entry = entry.map_err(|error| io(&current, &error))?;
        let inner = relative.join(entry.file_name());
        if transfer.skip.contains(&inner) {
            continue;
        }
        let metadata =
            std::fs::symlink_metadata(entry.path()).map_err(|error| io(&entry.path(), &error))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            walk(root, &inner, transfer, planned)?;
        } else if metadata.is_file() {
            planned.push(PlannedFile {
                from: entry.path(),
                to: transfer.to.join(&inner),
                size: metadata.len(),
                mode: transfer.mode,
            });
        }
    }
    Ok(())
}

pub fn plan(transfers: &[Transfer]) -> Result<Vec<PlannedFile>, BootError> {
    let mut planned = Vec::new();
    for transfer in transfers {
        match std::fs::symlink_metadata(&transfer.from) {
            Ok(metadata) if metadata.is_dir() => {
                walk(&transfer.from, Path::new(""), transfer, &mut planned)?;
            }
            Ok(metadata) if metadata.is_file() => planned.push(PlannedFile {
                from: transfer.from.clone(),
                to: transfer.to.clone(),
                size: metadata.len(),
                mode: transfer.mode,
            }),
            _ => {}
        }
    }
    Ok(planned)
}

fn complete(file: &PlannedFile) -> bool {
    std::fs::metadata(&file.to)
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() == file.size)
}

fn copy_file(from: &Path, to: &Path) -> std::io::Result<()> {
    let staged = PathBuf::from(format!("{}{PART_SUFFIX}", to.to_string_lossy()));
    std::fs::copy(from, &staged)?;
    std::fs::rename(&staged, to)
}

pub fn apply(file: &PlannedFile) -> Result<(), BootError> {
    if complete(file) {
        return Ok(());
    }
    if let Some(parent) = file.to.parent() {
        std::fs::create_dir_all(parent).map_err(|error| io(parent, &error))?;
    }
    if file.to.exists() {
        std::fs::remove_file(&file.to).map_err(|error| io(&file.to, &error))?;
    }
    let linked = file.mode == Mode::Link && std::fs::hard_link(&file.from, &file.to).is_ok();
    if !linked {
        copy_file(&file.from, &file.to).map_err(|error| io(&file.from, &error))?;
    }
    Ok(())
}

pub fn verify(planned: &[PlannedFile]) -> Result<(), BootError> {
    match planned.iter().find(|file| !complete(file)) {
        Some(file) => Err(BootError::Mismatch(file.to.display().to_string())),
        None => Ok(()),
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
    fn a_plan_walks_the_tree_skips_excluded_folders_and_applies_twice_without_change() {
        let root = tempfile::tempdir().expect("a folder");
        let from = root.path().join("legacy").join("mcp-toolchain");
        write(&from.join("node").join("26").join("node.exe"), "node");
        write(&from.join("uv").join("cache").join("blob"), "cache");
        write(&from.join("uv").join("python").join("python.exe"), "py");
        write(&from.join("npm-cache").join("x"), "npm");
        let to = root.path().join("state").join("assets").join("toolchain");
        let transfers = [Transfer::new(from.clone(), to.clone(), Mode::Link)
            .skipping(&[&["uv", "cache"], &["npm-cache"]])];

        let planned = plan(&transfers).expect("plans");
        let mut targets: Vec<PathBuf> = planned.iter().map(|file| file.to.clone()).collect();
        targets.sort();
        assert_eq!(
            targets,
            vec![
                to.join("node").join("26").join("node.exe"),
                to.join("uv").join("python").join("python.exe"),
            ]
        );
        assert!(verify(&planned).is_err());
        for file in &planned {
            apply(file).expect("applies");
        }
        verify(&planned).expect("verifies");
        for file in &planned {
            apply(file).expect("applies again");
        }
        assert_eq!(
            std::fs::read_to_string(to.join("uv").join("python").join("python.exe"))
                .expect("reads"),
            "py"
        );
        assert!(from.join("node").join("26").join("node.exe").is_file());
    }

    #[test]
    fn a_partial_target_is_replaced_and_a_missing_source_plans_nothing() {
        let root = tempfile::tempdir().expect("a folder");
        let from = root.path().join("a.txt");
        write(&from, "complete");
        let to = root.path().join("out").join("a.txt");
        write(&to, "half");
        let planned = plan(&[Transfer::new(from, to.clone(), Mode::Copy)]).expect("plans");
        apply(&planned[0]).expect("applies");
        assert_eq!(std::fs::read_to_string(&to).expect("reads"), "complete");
        assert!(
            plan(&[Transfer::new(
                root.path().join("missing"),
                root.path().join("x"),
                Mode::Copy
            )])
            .expect("plans")
            .is_empty()
        );
    }
}
