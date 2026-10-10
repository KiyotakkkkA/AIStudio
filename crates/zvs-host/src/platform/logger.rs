use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

use regex::Regex;
use serde_json::Value;

use super::clock::SharedClock;
use super::paths::log_file_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

pub type Field<'a> = (&'a str, Value);

pub trait Logger: Send + Sync {
    fn log(&self, level: LogLevel, scope: &str, message: &str, fields: &[Field<'_>]);

    fn close(&self) {}
}

pub type SharedLogger = Arc<dyn Logger>;

#[derive(Debug, Clone, Copy, Default)]
pub struct SilentLogger;

impl Logger for SilentLogger {
    fn log(&self, _level: LogLevel, _scope: &str, _message: &str, _fields: &[Field<'_>]) {}
}

pub fn silent_logger() -> SharedLogger {
    Arc::new(SilentLogger)
}

pub const REDACTED: &str = "[redacted]";

const MAX_REDACT_DEPTH: usize = 6;

static KEY_SHAPES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"(?-u:\b)sk-ant-api[0-9]{2}-[A-Za-z0-9_-]{16,}",
        r"(?-u:\b)sk-or-v1-[A-Za-z0-9_-]{16,}",
        r"(?-u:\b)sk-[A-Za-z0-9_-]{16,}",
        r"(?-u:\b)osk_(?:live|test)_[A-Za-z0-9]{8,}",
        r"(?-u:\b)(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}",
        r"(?-u:\b)xox[abposr]-[A-Za-z0-9-]{10,}",
        r"(?-u:\b)eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}",
        r"(?-u:\b)AKIA[0-9A-Z]{16}(?-u:\b)",
        r"(?i)(?-u:\b)Bearer\s+[A-Za-z0-9._~+/-]{16,}=*",
    ]
    .iter()
    .map(|shape| Regex::new(shape).expect("key shapes compile"))
    .collect()
});

pub fn redact_text(value: &str) -> String {
    KEY_SHAPES.iter().fold(value.to_owned(), |text, shape| {
        shape.replace_all(&text, REDACTED).into_owned()
    })
}

pub fn redact_secrets(value: &Value) -> Value {
    redact(value, 0)
}

fn redact(value: &Value, depth: usize) -> Value {
    match value {
        Value::String(text) => Value::String(redact_text(text)),
        _ if depth >= MAX_REDACT_DEPTH => value.clone(),
        Value::Array(entries) => entries
            .iter()
            .map(|entry| redact(entry, depth + 1))
            .collect(),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, entry)| (key.clone(), redact(entry, depth + 1)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

#[derive(Clone)]
pub struct LoggerOptions {
    pub directory: PathBuf,
    pub level: LogLevel,
    pub development: bool,
    pub max_bytes: u64,
    pub backups: u32,
    pub clock: SharedClock,
}

impl LoggerOptions {
    pub const DEFAULT_MAX_BYTES: u64 = 5 * 1024 * 1024;
    pub const DEFAULT_BACKUPS: u32 = 3;

    pub fn new(directory: PathBuf, level: LogLevel, clock: SharedClock) -> Self {
        Self {
            directory,
            level,
            development: false,
            max_bytes: Self::DEFAULT_MAX_BYTES,
            backups: Self::DEFAULT_BACKUPS,
            clock,
        }
    }
}

struct FileState {
    size: u64,
    closed: bool,
}

pub struct FileLogger {
    options: LoggerOptions,
    file: PathBuf,
    state: Mutex<FileState>,
}

impl FileLogger {
    pub fn open(options: LoggerOptions) -> io::Result<Self> {
        if options.max_bytes < 1 || options.backups < 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Logger rotation limits must be positive",
            ));
        }
        fs::create_dir_all(&options.directory)?;
        let file = log_file_path(&options.directory, 0);
        let size = fs::metadata(&file).map_or(0, |metadata| metadata.len());
        Ok(Self {
            options,
            file,
            state: Mutex::new(FileState {
                size,
                closed: false,
            }),
        })
    }

    pub fn file(&self) -> &PathBuf {
        &self.file
    }

    fn rotate(&self) {
        let directory = &self.options.directory;
        let _ = fs::remove_file(log_file_path(directory, self.options.backups));
        for index in (0..self.options.backups).rev() {
            let source = log_file_path(directory, index);
            if source.exists() {
                let _ = fs::rename(&source, log_file_path(directory, index + 1));
            }
        }
    }

    fn append(&self, line: &str) -> io::Result<()> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file)?
            .write_all(line.as_bytes())
    }
}

impl Logger for FileLogger {
    fn log(&self, level: LogLevel, scope: &str, message: &str, fields: &[Field<'_>]) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.closed || level < self.options.level {
            return;
        }
        let line = format_line(self.options.clock.now(), level, scope, message, fields);
        let bytes = line.len() as u64;
        if state.size > 0 && state.size + bytes > self.options.max_bytes {
            self.rotate();
            state.size = 0;
        }
        if self.append(&line).is_ok() {
            state.size += bytes;
        }
        if self.options.development {
            let _ = io::stdout().write_all(line.as_bytes());
        }
    }

    fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
        }
    }
}

pub fn format_line(
    timestamp: i64,
    level: LogLevel,
    scope: &str,
    message: &str,
    fields: &[Field<'_>],
) -> String {
    let mut entries: Vec<(String, Value)> = Vec::with_capacity(fields.len() + 5);
    let mut raw = None;
    for (key, value) in fields {
        if *key == "raw" {
            raw = Some(value);
        } else {
            upsert(&mut entries, key, redact_secrets(value));
        }
    }
    upsert(&mut entries, "timestamp", Value::from(timestamp));
    upsert(&mut entries, "level", Value::from(level.as_str()));
    upsert(&mut entries, "scope", Value::from(scope));
    upsert(&mut entries, "message", Value::String(redact_text(message)));
    let raw = match (level, raw) {
        (_, Some(raw)) => Some(redact_secrets(raw)),
        (LogLevel::Warn | LogLevel::Error, None) => Some(Value::String(redact_text(message))),
        _ => None,
    };
    if let Some(raw) = raw {
        upsert(&mut entries, "raw", raw);
    }
    let mut line = String::from("{");
    for (index, (key, value)) in entries.iter().enumerate() {
        if index > 0 {
            line.push(',');
        }
        line.push_str(&Value::String(key.clone()).to_string());
        line.push(':');
        line.push_str(&value.to_string());
    }
    line.push_str("}\n");
    line
}

fn upsert(entries: &mut Vec<(String, Value)>, key: &str, value: Value) {
    match entries.iter_mut().find(|(existing, _)| existing == key) {
        Some(entry) => entry.1 = value,
        None => entries.push((key.to_owned(), value)),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{Map, json};

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::testing::temporary_directory;

    fn rows(text: &str) -> Vec<Map<String, Value>> {
        text.lines()
            .map(|line| serde_json::from_str(line).expect("a JSON line"))
            .collect()
    }

    #[test]
    fn logger_filters_preserves_scope_rotates_resumes_and_closes() {
        let temp = temporary_directory("studio-logger-");
        let mut options = LoggerOptions::new(
            temp.path().to_path_buf(),
            LogLevel::Info,
            Arc::new(FakeClock::new(123)),
        );
        options.max_bytes = 100;
        options.backups = 2;
        let logger = FileLogger::open(options.clone()).expect("opens");
        logger.log(LogLevel::Debug, "test", "filtered", &[]);
        for index in 0..5 {
            logger.log(
                LogLevel::Info,
                "test",
                &format!("line {index}"),
                &[("scope", json!("spoof"))],
            );
        }
        assert_eq!(fs::read_dir(temp.path()).expect("lists").count(), 3);
        let current = fs::read_to_string(log_file_path(temp.path(), 0)).expect("reads");
        assert_eq!(
            Value::Object(rows(&current).remove(0)),
            json!({ "timestamp": 123, "level": "info", "scope": "test", "message": "line 4" })
        );
        logger.close();
        logger.log(LogLevel::Error, "test", "after close", &[]);
        let resumed = FileLogger::open(options).expect("reopens");
        resumed.log(LogLevel::Warn, "test", "resumed", &[]);
        resumed.close();
        let previous = fs::read_to_string(log_file_path(temp.path(), 1)).expect("reads");
        assert!(previous.contains("line 4"));
        let current = fs::read_to_string(log_file_path(temp.path(), 0)).expect("reads");
        assert!(current.contains("resumed"));
        assert!(!current.contains("after close"));
    }

    #[test]
    fn warning_and_error_logs_place_redacted_raw_text_immediately_after_message() {
        let temp = temporary_directory("studio-logger-raw-");
        let logger = FileLogger::open(LoggerOptions::new(
            temp.path().to_path_buf(),
            LogLevel::Debug,
            Arc::new(FakeClock::default()),
        ))
        .expect("opens");
        let raw = format!("Invalid key sk-{}\nVendor detail", "a".repeat(24));
        for level in [LogLevel::Warn, LogLevel::Error] {
            logger.log(
                level,
                "providers",
                "Probe did not succeed",
                &[("providerId", json!("provider-1")), ("raw", json!(raw))],
            );
        }
        logger.log(LogLevel::Warn, "ai", "Unknown response format", &[]);
        logger.log(LogLevel::Info, "providers", "Created a provider", &[]);
        logger.close();
        let text = fs::read_to_string(log_file_path(temp.path(), 0)).expect("reads");
        for line in text.lines().take(2) {
            assert!(line.contains("\"message\":\"Probe did not succeed\",\"raw\":"));
        }
        let parsed = rows(&text);
        for row in &parsed[..2] {
            assert_eq!(row["raw"], json!("Invalid key [redacted]\nVendor detail"));
        }
        assert_eq!(parsed[2]["raw"], json!("Unknown response format"));
        assert!(!parsed[3].contains_key("raw"));
    }

    #[test]
    fn the_line_keeps_the_typescript_key_order() {
        let line = format_line(
            7,
            LogLevel::Error,
            "ipc",
            "Handler failed",
            &[
                ("channel", json!("system.ping")),
                ("code", json!("UNKNOWN")),
            ],
        );
        assert_eq!(
            line,
            "{\"channel\":\"system.ping\",\"code\":\"UNKNOWN\",\"timestamp\":7,\"level\":\"error\",\"scope\":\"ipc\",\"message\":\"Handler failed\",\"raw\":\"Handler failed\"}\n"
        );
        let spoofed = format_line(1, LogLevel::Info, "real", "m", &[("scope", json!("spoof"))]);
        assert_eq!(
            spoofed,
            "{\"scope\":\"real\",\"timestamp\":1,\"level\":\"info\",\"message\":\"m\"}\n"
        );
    }

    #[test]
    fn secrets_are_redacted_in_messages_and_nested_fields() {
        let key = format!("sk-ant-api03-{}", "x".repeat(20));
        let bearer = format!("bearer {}", "y".repeat(20));
        let value = json!({ "a": [key, { "b": bearer }], "n": 1 });
        assert_eq!(
            redact_secrets(&value),
            json!({ "a": [REDACTED, { "b": REDACTED }], "n": 1 })
        );
        assert_eq!(redact_text("AKIAABCDEFGHIJKLMNOP!"), "[redacted]!");
        assert_eq!(
            redact_text("mask-abcdefghijklmnopqr"),
            "mask-abcdefghijklmnopqr"
        );
        let deep = json!([[[[[[[format!("ghp_{}", "z".repeat(20))]]]]]]]);
        assert_eq!(redact_secrets(&deep), deep);
    }

    #[test]
    fn rotation_limits_must_be_positive() {
        let temp = temporary_directory("studio-logger-limits-");
        let mut options = LoggerOptions::new(
            temp.path().to_path_buf(),
            LogLevel::Info,
            Arc::new(FakeClock::default()),
        );
        options.backups = 0;
        assert!(FileLogger::open(options).is_err());
    }
}
