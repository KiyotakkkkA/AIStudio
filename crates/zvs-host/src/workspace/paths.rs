use std::ffi::OsString;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::platform::long_path::from_extended_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    File,
    Folder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantTarget {
    pub path: PathBuf,
    pub folder: PathBuf,
    pub creates: Vec<PathBuf>,
}

pub fn canonical(path: &Path) -> io::Result<PathBuf> {
    let resolved = std::fs::canonicalize(path)?;
    Ok(PathBuf::from(from_extended_path(
        &resolved.to_string_lossy(),
    )))
}

fn component_key(component: Component<'_>) -> Option<String> {
    match component {
        Component::CurDir => None,
        Component::RootDir => Some(std::path::MAIN_SEPARATOR.to_string()),
        other => Some(other.as_os_str().to_string_lossy().to_lowercase()),
    }
}

fn keys(path: &Path) -> Vec<String> {
    path.components().filter_map(component_key).collect()
}

pub fn same_folder(left: &Path, right: &Path) -> bool {
    keys(left) == keys(right)
}

pub fn covers(grant: &Path, target: &Path) -> bool {
    let grant = keys(grant);
    let target = keys(target);
    !grant.is_empty() && target.len() >= grant.len() && target[..grant.len()] == grant[..]
}

pub fn relative_to(base: &Path, path: &Path) -> Option<PathBuf> {
    covers(base, path).then(|| {
        path.components()
            .filter(|component| component_key(*component).is_some())
            .skip(keys(base).len())
            .collect()
    })
}

pub fn is_network(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.starts_with(r"\\") || text.starts_with("//")
}

pub fn is_drive_root(path: &Path) -> bool {
    path.parent().is_none()
}

pub fn normalized(path: &Path) -> Option<PathBuf> {
    let mut parts: Vec<Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match parts.last() {
                Some(Component::Normal(_)) => {
                    parts.pop();
                }
                _ => return None,
            },
            other => parts.push(other),
        }
    }
    Some(parts.into_iter().collect())
}

pub fn resolve(base: Option<&Path>, path: &Path) -> Option<PathBuf> {
    if path.is_absolute() {
        normalized(path)
    } else {
        base.and_then(|base| normalized(&base.join(path)))
    }
}

pub fn grant_target(path: &Path, kind: TargetKind) -> io::Result<GrantTarget> {
    let absolute = normalized(path)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Path is not absolute"))?;
    let mut ancestor = absolute.as_path();
    let mut tail: Vec<OsString> = Vec::new();
    loop {
        match std::fs::metadata(ancestor) {
            Ok(metadata) => {
                let resolved = canonical(ancestor)?;
                let mut target = resolved.clone();
                for name in tail.iter().rev() {
                    target.push(name);
                }
                if tail.is_empty() {
                    let folder = if metadata.is_dir() {
                        resolved
                    } else {
                        resolved
                            .parent()
                            .map(Path::to_path_buf)
                            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?
                    };
                    return Ok(GrantTarget {
                        path: target,
                        folder,
                        creates: Vec::new(),
                    });
                }
                if !metadata.is_dir() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotADirectory,
                        "An ancestor of the path is a file",
                    ));
                }
                let folders = match kind {
                    TargetKind::File => tail.len() - 1,
                    TargetKind::Folder => tail.len(),
                };
                let mut creates = Vec::with_capacity(folders);
                let mut current = PathBuf::new();
                for name in tail.iter().rev().take(folders) {
                    current.push(name);
                    creates.push(current.clone());
                }
                return Ok(GrantTarget {
                    path: target,
                    folder: resolved,
                    creates,
                });
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let name = ancestor
                    .file_name()
                    .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
                tail.push(name.to_os_string());
                ancestor = ancestor
                    .parent()
                    .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
            }
            Err(error) => return Err(error),
        }
    }
}

pub type RemovableMounts = Arc<dyn Fn() -> Vec<PathBuf> + Send + Sync>;

#[derive(Clone)]
pub struct ForbiddenFolders {
    trees: Vec<PathBuf>,
    exact: Vec<PathBuf>,
    removable: RemovableMounts,
}

impl std::fmt::Debug for ForbiddenFolders {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ForbiddenFolders")
            .field("trees", &self.trees)
            .field("exact", &self.exact)
            .finish_non_exhaustive()
    }
}

fn settled(path: &Path) -> PathBuf {
    canonical(path).unwrap_or_else(|_| normalized(path).unwrap_or_else(|| path.to_path_buf()))
}

pub fn system_removable_mounts() -> Vec<PathBuf> {
    sysinfo::Disks::new_with_refreshed_list()
        .list()
        .iter()
        .filter(|disk| disk.is_removable())
        .map(|disk| disk.mount_point().to_path_buf())
        .collect()
}

impl ForbiddenFolders {
    pub fn new(trees: &[PathBuf], exact: &[PathBuf], removable: RemovableMounts) -> Self {
        Self {
            trees: trees.iter().map(|path| settled(path)).collect(),
            exact: exact.iter().map(|path| settled(path)).collect(),
            removable,
        }
    }

    pub fn from_environment(app_folders: &[PathBuf]) -> Self {
        let variable = |name: &str| std::env::var_os(name).map(PathBuf::from);
        let mut trees: Vec<PathBuf> = [
            "WINDIR",
            "SystemRoot",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramW6432",
            "ProgramData",
        ]
        .into_iter()
        .filter_map(variable)
        .collect();
        trees.extend(app_folders.iter().cloned());
        let exact: Vec<PathBuf> = ["USERPROFILE", "HOME"]
            .into_iter()
            .filter_map(variable)
            .collect();
        Self::new(&trees, &exact, Arc::new(system_removable_mounts))
    }

    pub fn refuses(&self, folder: &Path) -> bool {
        if is_network(folder) || is_drive_root(folder) {
            return true;
        }
        if self.trees.iter().any(|tree| covers(tree, folder)) {
            return true;
        }
        if self
            .trees
            .iter()
            .chain(&self.exact)
            .any(|forbidden| covers(folder, forbidden))
        {
            return true;
        }
        (self.removable)().iter().any(|mount| covers(mount, folder))
    }
}
