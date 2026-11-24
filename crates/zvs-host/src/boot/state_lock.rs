use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

pub const RESET_MARKER: &str = ".reset-requested";

pub struct StateRootLock {
    _file: File,
}

impl StateRootLock {
    pub(crate) fn blocking_exclusive(root: &Path) -> io::Result<Self> {
        let file = Self::open(root)?;
        file.lock()?;
        Ok(Self { _file: file })
    }

    pub fn shared(root: &Path) -> io::Result<Self> {
        let file = Self::open(root)?;
        file.lock_shared()?;
        if root.join(RESET_MARKER).try_exists()? {
            return Err(io::Error::from(io::ErrorKind::WouldBlock));
        }
        Ok(Self { _file: file })
    }

    pub(crate) fn exclusive(root: &Path) -> io::Result<Self> {
        let file = Self::open(root)?;
        file.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => io::Error::from(io::ErrorKind::WouldBlock),
            TryLockError::Error(error) => error,
        })?;
        Ok(Self { _file: file })
    }

    fn open(root: &Path) -> io::Result<File> {
        let root = canonical_root(root)?;
        let mut lock = root.file_name().ok_or_else(invalid_root)?.to_os_string();
        lock.push(".lock");
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.with_file_name(lock))
    }
}

fn canonical_root(root: &Path) -> io::Result<PathBuf> {
    match root.canonicalize() {
        Ok(root) => Ok(root),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let name = root.file_name().ok_or_else(invalid_root)?;
            let parent = root
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            std::fs::create_dir_all(parent)?;
            Ok(parent.canonicalize()?.join(name))
        }
        Err(error) => Err(error),
    }
}

fn invalid_root() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "Invalid state root")
}
