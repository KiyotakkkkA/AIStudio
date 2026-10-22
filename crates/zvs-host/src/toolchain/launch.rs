use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<PathBuf>,
}

pub fn prepend_path(directory: &Path, current: Option<&OsString>) -> String {
    let mut entries = vec![directory.to_path_buf()];
    if let Some(current) = current {
        entries.extend(
            std::env::split_paths(current)
                .filter(|entry| entry.as_os_str() != directory.as_os_str()),
        );
    }
    std::env::join_paths(entries.iter().filter(|entry| !entry.as_os_str().is_empty())).map_or_else(
        |_| directory.to_string_lossy().into_owned(),
        |joined| joined.to_string_lossy().into_owned(),
    )
}
