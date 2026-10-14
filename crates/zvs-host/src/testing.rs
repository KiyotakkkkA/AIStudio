pub mod fake_driver;

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Map, Value};
use tempfile::TempDir;
use zvs_contract::HostEvent;

pub use self::fake_driver::{FAKE_CAPABILITIES, FakeDriver, FakeDriverSource, FakeScript};
use crate::data::{BACKUPS_KEPT, Database, MIGRATIONS, MigrateOptions, migrate};
pub use crate::platform::clock::{CLOCK_ORIGIN, FakeClock};
use crate::platform::events::EventSink;
use crate::platform::logger::{Field, LogLevel, Logger, SharedLogger, silent_logger};
use crate::platform::theme::{ThemePort, ThemeSource};
use crate::services::crypto::BackendFailure;
use crate::services::{
    CryptoBackend, CryptoService, HealthCheckOptions, HealthCheckService, Plaintext,
    ProviderService, ProviderServiceOptions, SecretService, Services, SettingService,
    SystemService,
};

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

pub struct TemporaryDatabase {
    database: Arc<Database>,
    directory: TemporaryDirectory,
}

impl TemporaryDatabase {
    pub fn shared(&self) -> Arc<Database> {
        Arc::clone(&self.database)
    }

    pub fn directory(&self) -> &Path {
        self.directory.path()
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.directory.join("backups")
    }
}

impl Deref for TemporaryDatabase {
    type Target = Database;

    fn deref(&self) -> &Database {
        &self.database
    }
}

pub struct TemporaryDatabaseOptions {
    pub migrate: bool,
    pub logger: SharedLogger,
}

impl Default for TemporaryDatabaseOptions {
    fn default() -> Self {
        Self {
            migrate: true,
            logger: silent_logger(),
        }
    }
}

pub fn temporary_database_with(options: TemporaryDatabaseOptions) -> TemporaryDatabase {
    let directory = temporary_directory("studio-db-");
    let database = Database::open(&directory.join("studio.sqlite"), options.logger.clone())
        .expect("a temporary database");
    if options.migrate {
        migrate(
            &database,
            MIGRATIONS,
            &MigrateOptions {
                backups_dir: &directory.join("backups"),
                logger: options.logger,
                clock: Arc::new(FakeClock::default()),
                keep: BACKUPS_KEPT,
            },
        )
        .expect("the bundled migrations apply");
    }
    TemporaryDatabase {
        database: Arc::new(database),
        directory,
    }
}

pub fn temporary_database() -> TemporaryDatabase {
    temporary_database_with(TemporaryDatabaseOptions::default())
}

pub fn unmigrated_database() -> TemporaryDatabase {
    temporary_database_with(TemporaryDatabaseOptions {
        migrate: false,
        ..TemporaryDatabaseOptions::default()
    })
}

pub struct TestServices {
    services: Arc<Services>,
    pub theme: Arc<RecordingTheme>,
    pub clock: Arc<FakeClock>,
    pub crypto: Arc<FakeCrypto>,
    pub drivers: Arc<FakeDriverSource>,
    pub database: TemporaryDatabase,
}

impl TestServices {
    pub fn shared(&self) -> Arc<Services> {
        Arc::clone(&self.services)
    }
}

impl Deref for TestServices {
    type Target = Services;

    fn deref(&self) -> &Services {
        &self.services
    }
}

pub fn test_services(system: SystemService) -> TestServices {
    let database = temporary_database();
    let clock = Arc::new(FakeClock::default());
    let theme = RecordingTheme::shared();
    let crypto = FakeCrypto::shared();
    let drivers = FakeDriverSource::shared(FakeDriver::with_models(Vec::new()));
    let settings = SettingService::new(database.shared(), clock.clone());
    let secrets = Arc::new(SecretService::new(
        database.shared(),
        Arc::new(CryptoService::new(crypto.clone())),
        clock.clone(),
        silent_logger(),
    ));
    let providers = ProviderService::new(ProviderServiceOptions::new(
        database.shared(),
        drivers.clone(),
        Arc::clone(&secrets),
        silent_logger(),
        clock.clone(),
    ));
    let health = HealthCheckService::new(HealthCheckOptions {
        providers: providers.clone(),
        settings: settings.clone(),
        logger: silent_logger(),
        clock: clock.clone(),
        events: None,
        runtime: None,
    });
    TestServices {
        services: Arc::new(Services {
            system,
            settings,
            secrets,
            providers,
            health,
            theme: theme.clone(),
        }),
        theme,
        clock,
        crypto,
        drivers,
        database,
    }
}

const FAKE_CRYPTO_MASK: u8 = 0x5a;

#[derive(Debug)]
pub struct FakeCrypto {
    available: AtomicBool,
    fail_next: AtomicBool,
    encrypt_calls: AtomicUsize,
    decrypt_calls: AtomicUsize,
}

impl Default for FakeCrypto {
    fn default() -> Self {
        Self {
            available: AtomicBool::new(true),
            fail_next: AtomicBool::new(false),
            encrypt_calls: AtomicUsize::new(0),
            decrypt_calls: AtomicUsize::new(0),
        }
    }
}

impl FakeCrypto {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn set_available(&self, available: bool) {
        self.available.store(available, Ordering::SeqCst);
    }

    pub fn fail_next(&self) {
        self.fail_next.store(true, Ordering::SeqCst);
    }

    pub fn encrypt_calls(&self) -> usize {
        self.encrypt_calls.load(Ordering::SeqCst)
    }

    pub fn decrypt_calls(&self) -> usize {
        self.decrypt_calls.load(Ordering::SeqCst)
    }

    fn failing(&self, direction: &str) -> Result<(), BackendFailure> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            Err(BackendFailure::new(format!(
                "keychain refused the {direction}"
            )))
        } else {
            Ok(())
        }
    }

    fn mask(bytes: &[u8]) -> Vec<u8> {
        bytes.iter().map(|byte| byte ^ FAKE_CRYPTO_MASK).collect()
    }
}

impl CryptoBackend for FakeCrypto {
    fn is_available(&self) -> bool {
        self.available.load(Ordering::SeqCst)
    }

    fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, BackendFailure> {
        self.encrypt_calls.fetch_add(1, Ordering::SeqCst);
        self.failing("write")?;
        Ok(Self::mask(plaintext.as_bytes()))
    }

    fn decrypt(&self, cipher: &[u8]) -> Result<Plaintext, BackendFailure> {
        self.decrypt_calls.fetch_add(1, Ordering::SeqCst);
        self.failing("read")?;
        String::from_utf8(Self::mask(cipher))
            .map(Plaintext::new)
            .map_err(|_| BackendFailure::new("not UTF-8"))
    }
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

#[derive(Debug, Default)]
pub struct RecordingTheme(Mutex<Vec<ThemeSource>>);

impl RecordingTheme {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn applied(&self) -> Vec<ThemeSource> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl ThemePort for RecordingTheme {
    fn apply(&self, source: ThemeSource) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(source);
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

    #[test]
    fn a_temporary_database_is_migrated_and_removed_on_drop() {
        let database = temporary_database();
        let directory = database.directory().to_path_buf();
        assert!(directory.join("studio.sqlite").exists());
        let tables = database
            .with_connection(|connection| {
                Ok(connection.query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'setting'",
                    [],
                    |row| row.get::<_, i64>(0),
                )?)
            })
            .expect("counts");
        assert_eq!(tables, 1);
        drop(database);
        assert!(!directory.exists());
    }
}
