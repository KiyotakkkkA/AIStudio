use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use rusqlite::{Connection, Transaction};
use serde_json::json;
use zvs_contract::{AppError, AppErrorCode};

use crate::ipc::INTERNAL_MESSAGE;
use crate::platform::logger::{LogLevel, SharedLogger};

pub const BUSY_TIMEOUT: Duration = Duration::from_millis(5_000);

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error(transparent)]
    App(#[from] AppError),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct Database {
    connection: Mutex<Connection>,
    file: PathBuf,
    logger: SharedLogger,
}

impl Database {
    pub fn open(file: &Path, logger: SharedLogger) -> Result<Self, DataError> {
        if let Some(parent) = file.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(file)?;
        connection.query_row("PRAGMA journal_mode = WAL", [], |row| {
            row.get::<_, String>(0)
        })?;
        connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL;")?;
        connection.busy_timeout(BUSY_TIMEOUT)?;
        Ok(Self {
            connection: Mutex::new(connection),
            file: file.to_path_buf(),
            logger,
        })
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    pub fn with_connection<T>(
        &self,
        work: impl FnOnce(&Connection) -> Result<T, DataError>,
    ) -> Result<T, AppError> {
        let connection = self.lock();
        work(&connection).map_err(|error| self.fail(error))
    }

    pub fn transaction<T>(
        &self,
        work: impl FnOnce(&Transaction<'_>) -> Result<T, DataError>,
    ) -> Result<T, AppError> {
        let mut connection = self.lock();
        let outcome = connection
            .transaction()
            .map_err(DataError::from)
            .and_then(|transaction| {
                let value = work(&transaction)?;
                transaction.commit()?;
                Ok(value)
            });
        outcome.map_err(|error| self.fail(error))
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Connection> {
        self.connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn fail(&self, error: DataError) -> AppError {
        match error {
            DataError::App(error) => error,
            error @ (DataError::Sqlite(_) | DataError::Io(_)) => {
                self.logger.log(
                    LogLevel::Error,
                    "data",
                    "Database operation failed",
                    &[("error", json!(error.to_string()))],
                );
                AppError::new(AppErrorCode::Unknown, INTERNAL_MESSAGE)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::*;
    use crate::data::repositories::Repositories;
    use crate::testing::{
        MemoryLogger, TemporaryDatabaseOptions, temporary_database, temporary_database_with,
        unmigrated_database,
    };

    #[test]
    fn the_connection_is_opened_with_the_pragmas_the_data_layer_depends_on() {
        let database = unmigrated_database();
        database
            .with_connection(|connection| {
                let pragma = |name: &str| {
                    connection.query_row(&format!("PRAGMA {name}"), [], |row| {
                        row.get::<_, rusqlite::types::Value>(0)
                    })
                };
                assert_eq!(pragma("journal_mode")?, "wal".to_owned().into());
                assert_eq!(pragma("foreign_keys")?, 1_i64.into());
                assert_eq!(pragma("busy_timeout")?, 5_000_i64.into());
                assert_eq!(pragma("synchronous")?, 1_i64.into());
                Ok(())
            })
            .expect("pragmas");
    }

    #[test]
    fn a_transaction_rolls_the_whole_unit_of_work_back_when_it_fails() {
        let database = temporary_database();
        let failed = database.transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            repositories.settings.set("theme.name", "\"light\"", 2)?;
            repositories.settings.set("locale.name", "\"ru\"", 2)?;
            Err::<(), _>(AppError::new(AppErrorCode::Unknown, "halfway").into())
        });
        assert_eq!(failed.expect_err("fails").message, "halfway");
        let listed = database
            .with_connection(|connection| Ok(Repositories::new(connection).settings.list()?))
            .expect("lists");
        assert!(listed.is_empty());
    }

    #[test]
    fn a_panic_inside_a_transaction_rolls_back_and_leaves_the_database_usable() {
        let database = temporary_database();
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            database.transaction(|transaction| -> Result<(), DataError> {
                Repositories::new(transaction)
                    .settings
                    .set("theme.name", "\"dark\"", 1)?;
                panic!("the unit of work panicked")
            })
        }));
        assert!(outcome.is_err());
        let stored = database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).settings.get("theme.name")?)
            })
            .expect("reads");
        assert_eq!(stored, None);
    }

    #[test]
    fn a_sqlite_failure_becomes_an_internal_error_and_is_logged_host_side() {
        let logger = MemoryLogger::shared();
        let database = temporary_database_with(TemporaryDatabaseOptions {
            logger: logger.clone(),
            ..TemporaryDatabaseOptions::default()
        });
        let error = database
            .with_connection(|connection| {
                connection.execute("INSERT INTO missing_table VALUES (1)", [])?;
                Ok(())
            })
            .expect_err("fails");
        assert_eq!(error.code, AppErrorCode::Unknown);
        assert_eq!(error.message, INTERNAL_MESSAGE);
        assert_eq!(error.details, None);
        let logged = logger.matching("Database operation failed");
        assert_eq!(logged.len(), 1);
        assert!(
            logged[0].fields["error"]
                .as_str()
                .is_some_and(|text| text.contains("missing_table"))
        );
    }
}
