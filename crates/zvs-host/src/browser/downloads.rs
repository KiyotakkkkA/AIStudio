use std::path::{Path, PathBuf};
use std::sync::Arc;

use zvs_contract::browser_data::DOWNLOAD_HISTORY_MAX;
use zvs_contract::{
    AppError, AppErrorCode, BrowserDownloadDto, BrowserDownloadState, BrowserDownloadsDto,
    Timestamp,
};

use crate::data::schema::BrowserDownloadRow;
use crate::data::{Database, Repositories};
use crate::platform::clock::SharedClock;
use crate::platform::external::{SharedFileLauncher, reveal_in_file_manager};
use crate::platform::ids::new_id;

pub const DOWNLOAD_NOT_FOUND: &str = "Загрузка не найдена в истории";
pub const DOWNLOAD_FILE_MISSING: &str = "Файл загрузки больше не существует";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRecord {
    pub url: String,
    pub file_name: String,
    pub path: Option<PathBuf>,
    pub bytes: u64,
    pub state: BrowserDownloadState,
    pub agent: bool,
}

fn dto(row: BrowserDownloadRow) -> Option<BrowserDownloadDto> {
    Some(BrowserDownloadDto {
        id: row.id,
        url: row.url,
        file_name: row.file_name,
        path: row.path,
        bytes: u64::try_from(row.bytes).unwrap_or(0),
        state: BrowserDownloadState::parse(&row.state)?,
        agent: row.agent,
        finished_at: row.finished_at,
    })
}

#[derive(Clone)]
pub struct BrowserDownloads {
    database: Arc<Database>,
    clock: SharedClock,
    launcher: Option<SharedFileLauncher>,
}

impl BrowserDownloads {
    pub fn new(database: Arc<Database>, clock: SharedClock) -> Self {
        Self {
            database,
            clock,
            launcher: None,
        }
    }

    #[must_use]
    pub fn with_launcher(mut self, launcher: SharedFileLauncher) -> Self {
        self.launcher = Some(launcher);
        self
    }

    pub fn record(&self, record: &DownloadRecord) -> Result<BrowserDownloadDto, AppError> {
        let row = BrowserDownloadRow {
            id: new_id().hyphenated().to_string(),
            url: record.url.clone(),
            file_name: record.file_name.clone(),
            path: record
                .path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            bytes: i64::try_from(record.bytes).unwrap_or(i64::MAX),
            state: record.state.as_str().to_owned(),
            agent: record.agent,
            finished_at: self.now(),
        };
        self.database.with_connection(|connection| {
            let repository = Repositories::new(connection).browser_downloads;
            repository.insert(&row)?;
            repository.keep_newest(DOWNLOAD_HISTORY_MAX)?;
            Ok(())
        })?;
        dto(row).ok_or_else(|| AppError::new(AppErrorCode::Unknown, DOWNLOAD_NOT_FOUND))
    }

    pub fn list(&self) -> Result<BrowserDownloadsDto, AppError> {
        let rows = self.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .browser_downloads
                .list(DOWNLOAD_HISTORY_MAX)?)
        })?;
        Ok(BrowserDownloadsDto {
            items: rows.into_iter().filter_map(dto).collect(),
        })
    }

    pub fn remove(&self, id: &str) -> Result<BrowserDownloadsDto, AppError> {
        self.database.with_connection(|connection| {
            Ok(Repositories::new(connection).browser_downloads.remove(id)?)
        })?;
        self.list()
    }

    pub fn clear(&self) -> Result<BrowserDownloadsDto, AppError> {
        self.database.with_connection(|connection| {
            Ok(Repositories::new(connection).browser_downloads.clear()?)
        })?;
        self.list()
    }

    fn existing_path(&self, id: &str) -> Result<PathBuf, AppError> {
        let row = self
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).browser_downloads.find(id)?)
            })?
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, DOWNLOAD_NOT_FOUND))?;
        row.path
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, DOWNLOAD_FILE_MISSING))
    }

    pub fn open(&self, id: &str) -> Result<(), AppError> {
        let path = self.existing_path(id)?;
        match &self.launcher {
            Some(launcher) => launcher.open(&path),
            None => crate::platform::external::system_file_launcher().open(&path),
        }
    }

    pub fn reveal(&self, id: &str) -> Result<(), AppError> {
        reveal_in_file_manager(&self.existing_path(id)?)
    }

    fn now(&self) -> Timestamp {
        self.clock.now()
    }
}

pub fn unique_path(folder: &Path, name: &str) -> PathBuf {
    let candidate = folder.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path.file_stem().map_or_else(
        || name.to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    let extension = path
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    (1..)
        .map(|index| folder.join(format!("{stem} ({index}){extension}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or(candidate)
}

pub fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    std::fs::remove_file(from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::testing::temporary_database;

    fn record(name: &str, path: Option<PathBuf>) -> DownloadRecord {
        DownloadRecord {
            url: format!("https://files.example/{name}"),
            file_name: name.to_owned(),
            path,
            bytes: 12,
            state: BrowserDownloadState::Done,
            agent: false,
        }
    }

    #[test]
    fn history_lists_newest_first_and_forgets_without_touching_files() {
        let database = temporary_database();
        let clock = Arc::new(FakeClock::default());
        let downloads = BrowserDownloads::new(database.shared(), clock.clone());
        let folder = tempfile::tempdir().expect("a folder");
        let file = folder.path().join("a.txt");
        std::fs::write(&file, "a").expect("writes");
        let first = downloads
            .record(&record("a.txt", Some(file.clone())))
            .expect("records");
        clock.advance(1000);
        let second = downloads
            .record(&DownloadRecord {
                state: BrowserDownloadState::Cancelled,
                ..record("b.txt", None)
            })
            .expect("records");
        let listed = downloads.list().expect("lists");
        assert_eq!(
            listed
                .items
                .iter()
                .map(|item| item.id.clone())
                .collect::<Vec<_>>(),
            [second.id.clone(), first.id.clone()]
        );
        assert_eq!(
            downloads.reveal(&second.id).expect_err("no file").message,
            DOWNLOAD_FILE_MISSING
        );
        assert_eq!(downloads.remove(&first.id).expect("removes").items.len(), 1);
        assert!(file.is_file());
        assert!(downloads.clear().expect("clears").items.is_empty());
        assert_eq!(
            downloads.open(&first.id).expect_err("gone").code,
            AppErrorCode::NotFound
        );
    }

    #[test]
    fn a_taken_name_gets_a_counter_and_moves_keep_the_bytes() {
        let folder = tempfile::tempdir().expect("a folder");
        std::fs::write(folder.path().join("report.pdf"), "x").expect("writes");
        std::fs::write(folder.path().join("report (1).pdf"), "x").expect("writes");
        assert_eq!(
            unique_path(folder.path(), "report.pdf"),
            folder.path().join("report (2).pdf")
        );
        let source = folder.path().join("pending.bin");
        std::fs::write(&source, "payload").expect("writes");
        let target = folder.path().join("kept").join("file.bin");
        move_file(&source, &target).expect("moves");
        assert!(!source.exists());
        assert_eq!(std::fs::read_to_string(target).expect("reads"), "payload");
    }
}
