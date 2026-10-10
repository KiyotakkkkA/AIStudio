use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value};
use tempfile::TempDir;
use zvs_contract::HostEvent;

pub use crate::platform::clock::{CLOCK_ORIGIN, FakeClock};
use crate::platform::events::EventSink;
use crate::platform::logger::{Field, LogLevel, Logger};

fn normalise(path: &Path) -> String {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let text = resolved.to_string_lossy().into_owned();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

pub fn assert_temporary_path(path: &Path) -> &Path {
    let root = normalise(&std::env::temp_dir());
    let candidate = normalise(path);
    let separator = std::path::MAIN_SEPARATOR;
    assert!(
        candidate == root || candidate.starts_with(&format!("{root}{separator}")),
        "Refusing to use {}: tests may only write under {}",
        path.display(),
        std::env::temp_dir().display()
    );
    path
}

pub struct TemporaryDirectory(TempDir);

impl TemporaryDirectory {
    pub fn path(&self) -> &Path {
        self.0.path()
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.0.path().join(name)
    }
}

pub fn temporary_directory(prefix: &str) -> TemporaryDirectory {
    let directory = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir()
        .expect("a temporary directory");
    assert_temporary_path(directory.path());
    TemporaryDirectory(directory)
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub level: LogLevel,
    pub scope: String,
    pub message: String,
    pub fields: Map<String, Value>,
}

#[derive(Debug, Default)]
pub struct MemoryLogger(Mutex<Vec<LogEntry>>);

impl MemoryLogger {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn entries(&self) -> Vec<LogEntry> {
        self.0.lock().expect("log entries").clone()
    }

    pub fn matching(&self, needle: &str) -> Vec<LogEntry> {
        self.entries()
            .into_iter()
            .filter(|entry| entry.message.contains(needle))
            .collect()
    }
}

impl Logger for MemoryLogger {
    fn log(&self, level: LogLevel, scope: &str, message: &str, fields: &[Field<'_>]) {
        self.0.lock().expect("log entries").push(LogEntry {
            level,
            scope: scope.to_owned(),
            message: message.to_owned(),
            fields: fields
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
        });
    }
}

#[derive(Debug, Default)]
pub struct MemorySink(Mutex<Vec<HostEvent>>);

impl MemorySink {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn events(&self) -> Vec<HostEvent> {
        self.0.lock().expect("events").clone()
    }
}

impl EventSink for MemorySink {
    fn deliver(&self, event: &HostEvent) -> Result<(), String> {
        self.0.lock().expect("events").push(event.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_temporary_directory_lives_under_the_os_temp_dir_and_is_removed() {
        let directory = temporary_directory("studio-helper-");
        let path = directory.path().to_path_buf();
        assert!(path.is_dir());
        drop(directory);
        assert!(!path.exists());
    }

    #[test]
    #[should_panic(expected = "Refusing to use")]
    fn a_path_outside_the_temp_dir_is_refused() {
        let outside = std::env::current_dir().expect("a working directory");
        assert_temporary_path(&outside);
    }
}
