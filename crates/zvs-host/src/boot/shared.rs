use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{BootError, StateRootLock};
use crate::platform::paths::{SharedPaths, StudioLayout, StudioPaths};

const VERSION: u32 = 1;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedState {
    version: u32,
    pub desktop_dialogs: PathBuf,
    pub cli_dialogs: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desktop_executable: Option<PathBuf>,
}

pub fn initialize(layout: &StudioLayout) -> Result<(), BootError> {
    let _lock = StateRootLock::blocking_exclusive(&layout.shared.root).map_err(io)?;
    initialize_locked(layout)
}

pub(super) fn initialize_locked(layout: &StudioLayout) -> Result<(), BootError> {
    let SharedPaths {
        root,
        skills,
        package_cache,
        metadata,
    } = &layout.shared;
    let existing = read(&layout.shared)?;
    let expected = SharedState {
        version: VERSION,
        desktop_dialogs: StudioPaths::conversation_root(&layout.desktop),
        cli_dialogs: StudioPaths::conversation_root(&layout.cli),
        desktop_executable: existing
            .as_ref()
            .and_then(|state| state.desktop_executable.clone()),
    };
    for directory in [skills, package_cache, metadata] {
        std::fs::create_dir_all(directory).map_err(io)?;
    }
    if existing.as_ref() != Some(&expected) {
        write(root, &expected)?;
    }
    Ok(())
}

pub fn read(paths: &SharedPaths) -> Result<Option<SharedState>, BootError> {
    let file = paths.root.join("layout.json");
    match std::fs::read(&file) {
        Ok(bytes) => {
            let state: SharedState = serde_json::from_slice(&bytes).map_err(io)?;
            if state.version != VERSION {
                return Err(BootError::Mismatch(file.display().to_string()));
            }
            Ok(Some(state))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io(error)),
    }
}

pub fn register_desktop(paths: &SharedPaths, executable: &Path) -> Result<(), BootError> {
    let _lock = StateRootLock::blocking_exclusive(&paths.root).map_err(io)?;
    let mut state = read(paths)?.ok_or_else(|| io("Shared layout is not initialized"))?;
    if state.desktop_executable.as_deref() != Some(executable) {
        state.desktop_executable = Some(executable.to_path_buf());
        write(&paths.root, &state)?;
    }
    Ok(())
}

fn write(root: &Path, state: &SharedState) -> Result<(), BootError> {
    let staged = tempfile::NamedTempFile::new_in(root).map_err(io)?;
    serde_json::to_writer_pretty(staged.as_file(), state).map_err(io)?;
    staged.as_file().sync_all().map_err(io)?;
    staged.persist(root.join("layout.json")).map_err(io)?;
    Ok(())
}

pub(super) fn io(error: impl std::fmt::Display) -> BootError {
    BootError::Io(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_apps_can_initialize_the_same_shared_root_concurrently() {
        let home = tempfile::tempdir().expect("temporary local data");
        let layout = StudioLayout::resolve(home.path(), false);
        std::thread::scope(|threads| {
            let initializers: Vec<_> = (0..4)
                .map(|_| threads.spawn(|| initialize(&layout)))
                .collect();
            for initializer in initializers {
                initializer.join().expect("thread").expect("initializes");
            }
        });
        let executable = home.path().join("zvs-studio.exe");
        register_desktop(&layout.shared, &executable).expect("registers");
        initialize(&layout).expect("initializes again");
        let state = read(&layout.shared).expect("reads").expect("created");
        assert_eq!(
            state.desktop_dialogs,
            StudioPaths::conversation_root(&layout.desktop)
        );
        assert_eq!(
            state.cli_dialogs,
            StudioPaths::conversation_root(&layout.cli)
        );
        assert_eq!(state.desktop_executable, Some(executable));
        assert!(layout.shared.skills.is_dir());
        assert!(layout.shared.package_cache.is_dir());
        assert!(!layout.desktop.exists());
        assert!(!layout.cli.exists());
    }
}
