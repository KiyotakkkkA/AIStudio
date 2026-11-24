mod desktop_upgrade;
mod dialog_upgrade;
pub mod project_struct;
pub mod rewrite;
pub mod shared;
mod state_lock;
pub mod transfer;

pub use state_lock::{RESET_MARKER, StateRootLock};

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use zvs_contract::layout::{LegacyRecord, PROJECT_STRUCT, SCHEMAS, StateFile, expected_version};

use crate::data::dialogs::DialogJournal;
use crate::dialog_history::DialogHistory;
use crate::platform::paths::{DATABASE_FILE, StudioLayout, StudioPaths};

pub const LEGACY_RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BootError {
    #[error("Файл aistudio.toml повреждён: {0}")]
    Malformed(String),
    #[error(
        "Данные созданы более новой версией приложения: схема «{schema}» версии {found}, поддерживается версия {supported}"
    )]
    Newer {
        schema: String,
        found: u32,
        supported: u32,
    },
    #[error("Данные содержат неизвестные схемы: {0}")]
    Unknown(String),
    #[error("Не удалось перенести данные: {0}")]
    Io(String),
    #[error("Перенесённые данные не совпадают с исходными: {0}")]
    Mismatch(String),
    #[error("Не удалось проверить историю диалогов")]
    DialogHistory(String),
    #[error("В новом каталоге уже есть данные: {0}")]
    DestinationConflict(PathBuf),
    #[error(
        "Сброс данных ожидает завершения другого приложения AI Studio. Закройте его и запустите приложение снова."
    )]
    StateInUse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacySources {
    pub roaming: PathBuf,
    pub local: Option<PathBuf>,
}

impl LegacySources {
    pub fn present(&self) -> bool {
        self.roaming.join(DATABASE_FILE).is_file()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepProgress {
    pub done: u64,
    pub total: u64,
}

pub struct StepContext<'a> {
    pub root: &'a Path,
    pub legacy: Option<&'a LegacySources>,
    pub layout: Option<&'a StudioLayout>,
}

pub type StepRun = fn(&StepContext<'_>, &dyn Fn(StepProgress)) -> Result<(), BootError>;

#[derive(Clone, Copy)]
pub struct Step {
    pub schema: &'static str,
    pub from: u32,
    pub label: &'static str,
    pub run: StepRun,
}

impl std::fmt::Debug for Step {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Step")
            .field("schema", &self.schema)
            .field("from", &self.from)
            .finish_non_exhaustive()
    }
}

impl PartialEq for Step {
    fn eq(&self, other: &Self) -> bool {
        self.schema == other.schema && self.from == other.from
    }
}

pub const STEPS: &[Step] = &[
    Step {
        schema: PROJECT_STRUCT,
        from: 0,
        label: project_struct::LEGACY_LABEL,
        run: project_struct::migrate_legacy,
    },
    Step {
        schema: PROJECT_STRUCT,
        from: 1,
        label: "Перенос истории диалогов",
        run: dialog_upgrade::upgrade,
    },
    Step {
        schema: PROJECT_STRUCT,
        from: 2,
        label: "Перенос данных AI Studio Desktop",
        run: desktop_upgrade::upgrade,
    },
];

fn validate_dialogs(
    context: &StepContext<'_>,
    progress: &dyn Fn(StepProgress),
) -> Result<(), BootError> {
    let history = DialogHistory::new(StudioPaths::conversation_root(context.root));
    let io_error = |error: std::io::Error| BootError::Io(error.to_string());
    if history.dialogs().map_err(io_error)?.is_empty() {
        return Ok(());
    }
    progress(StepProgress { done: 0, total: 1 });
    validate_history(&history)?;
    progress(StepProgress { done: 1, total: 1 });
    Ok(())
}

fn validate_history(history: &DialogHistory) -> Result<(), BootError> {
    let io_error = |error: std::io::Error| BootError::Io(error.to_string());
    let staging = tempfile::tempdir().map_err(io_error)?;
    let mut connection = rusqlite::Connection::open(staging.path().join("dialogs.sqlite"))
        .map_err(|error| BootError::Io(error.to_string()))?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|error| BootError::Io(error.to_string()))?;
    let transaction = connection
        .transaction()
        .map_err(|error| BootError::Io(error.to_string()))?;
    DialogJournal::rebuild_projection(history, &transaction)
        .map_err(|error| BootError::DialogHistory(error.to_string()))?;
    transaction
        .commit()
        .map_err(|error| BootError::Io(error.to_string()))?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub enum BootDecision {
    Ready,
    Fresh,
    Migrate(Vec<Step>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootProgress {
    pub step: usize,
    pub steps: usize,
    pub label: &'static str,
    pub done: u64,
    pub total: u64,
}

pub struct Boot<'a> {
    pub root: &'a Path,
    pub legacy: Option<&'a LegacySources>,
    pub steps: &'a [Step],
    layout: Option<&'a StudioLayout>,
}

impl<'a> Boot<'a> {
    pub fn new(root: &'a Path, legacy: Option<&'a LegacySources>) -> Self {
        Self {
            root,
            legacy,
            steps: STEPS,
            layout: None,
        }
    }

    pub fn with_layout(mut self, layout: &'a StudioLayout) -> Self {
        self.layout = Some(layout);
        self
    }

    pub fn working_root(&self) -> Result<&Path, BootError> {
        if let Some(previous) = self
            .layout
            .and_then(|layout| layout.previous_desktop.as_deref())
            && previous.try_exists().map_err(shared::io)?
        {
            match std::fs::read_dir(self.root) {
                Ok(mut entries) => {
                    if entries.next().transpose().map_err(shared::io)?.is_some() {
                        return Err(BootError::DestinationConflict(self.root.to_path_buf()));
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(shared::io(error)),
            }
            return Ok(previous);
        }
        Ok(self.root)
    }

    fn pending(&self, file: &StateFile) -> Vec<Step> {
        let mut pending = Vec::new();
        for (schema, expected) in SCHEMAS {
            for from in file.version(schema)..expected {
                if let Some(step) = self
                    .steps
                    .iter()
                    .find(|step| step.schema == schema && step.from == from)
                {
                    pending.push(*step);
                }
            }
        }
        pending
    }

    fn legacy_present(&self) -> bool {
        self.legacy.is_some_and(LegacySources::present)
    }

    pub fn inspect(&self) -> Result<BootDecision, BootError> {
        let root = self.working_root()?;
        let file = StateFile::read(root).map_err(|error| match error {
            zvs_contract::layout::StateFileError::Malformed(reason) => BootError::Malformed(reason),
            zvs_contract::layout::StateFileError::Io(error) => BootError::Io(error.to_string()),
        })?;
        let Some(file) = file else {
            let adopted = root.join("data").join(DATABASE_FILE).is_file();
            if adopted {
                let mut file = StateFile::default();
                file.schemas.insert(PROJECT_STRUCT.to_owned(), 1);
                return Ok(BootDecision::Migrate(self.pending(&file)));
            }
            if self.legacy_present() {
                return Ok(BootDecision::Migrate(self.pending(&StateFile::default())));
            }
            if root != self.root {
                let mut file = StateFile::current();
                file.schemas.insert(PROJECT_STRUCT.to_owned(), 2);
                return Ok(BootDecision::Migrate(self.pending(&file)));
            }
            return Ok(BootDecision::Fresh);
        };
        let unknown = file.unknown_schemas();
        if !unknown.is_empty() {
            return Err(BootError::Unknown(unknown.join(", ")));
        }
        for (schema, found) in &file.schemas {
            let supported = expected_version(schema).unwrap_or_default();
            if *found > supported {
                return Err(BootError::Newer {
                    schema: schema.clone(),
                    found: *found,
                    supported,
                });
            }
        }
        let pending = self.pending(&file);
        if pending.is_empty() && file.matches_build() {
            Ok(BootDecision::Ready)
        } else {
            Ok(BootDecision::Migrate(pending))
        }
    }

    fn write(root: &Path, file: &StateFile) -> Result<(), BootError> {
        file.write(root)
            .map_err(|error| BootError::Io(error.to_string()))
    }

    pub fn run(
        &self,
        decision: &BootDecision,
        now: i64,
        progress: &dyn Fn(BootProgress),
    ) -> Result<(), BootError> {
        match decision {
            BootDecision::Ready => self.layout.map_or(Ok(()), shared::initialize),
            BootDecision::Fresh => {
                if let Some(layout) = self.layout {
                    shared::initialize(layout)?;
                }
                let context = StepContext {
                    root: self.root,
                    legacy: self.legacy,
                    layout: self.layout,
                };
                validate_dialogs(&context, &|_| {})?;
                Self::write(self.root, &StateFile::current())
            }
            BootDecision::Migrate(steps) => {
                let mut root = self.working_root()?;
                let _previous_lock = if root != self.root {
                    Some(StateRootLock::exclusive(root).map_err(|error| {
                        if error.kind() == std::io::ErrorKind::WouldBlock {
                            BootError::StateInUse
                        } else {
                            shared::io(error)
                        }
                    })?)
                } else {
                    None
                };
                let mut file = StateFile::read(root)
                    .map_err(shared::io)?
                    .unwrap_or_default();
                for (index, step) in steps.iter().enumerate() {
                    let report = |current: StepProgress| {
                        progress(BootProgress {
                            step: index + 1,
                            steps: steps.len(),
                            label: step.label,
                            done: current.done,
                            total: current.total,
                        });
                    };
                    let context = StepContext {
                        root,
                        legacy: self.legacy,
                        layout: self.layout,
                    };
                    if step.schema == PROJECT_STRUCT && step.from == 2 {
                        file.schemas.insert(PROJECT_STRUCT.to_owned(), 2);
                        Self::write(root, &file)?;
                        (step.run)(&context, &report)?;
                        root = self.root;
                    } else {
                        (step.run)(&context, &report)?;
                    }
                    file.schemas.insert(step.schema.to_owned(), step.from + 1);
                    if step.schema == PROJECT_STRUCT
                        && step.from == 0
                        && let Some(legacy) = self.legacy
                        && legacy.present()
                    {
                        file.legacy = Some(LegacyRecord {
                            source: legacy.roaming.to_string_lossy().into_owned(),
                            migrated_at: now,
                        });
                    }
                    Self::write(root, &file)?;
                }
                for (schema, version) in SCHEMAS {
                    file.schemas.entry(schema.to_owned()).or_insert(version);
                }
                Self::write(root, &file)
            }
        }
    }
}

pub fn sweep_temp(directories: &[PathBuf]) -> usize {
    let mut removed = 0;
    for directory in directories {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let gone = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            removed += usize::from(gone.is_ok());
        }
    }
    removed
}

fn modified_after(path: &Path, instant: SystemTime) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink() {
        return false;
    }
    if metadata.is_file() {
        return metadata.modified().is_ok_and(|modified| modified > instant);
    }
    std::fs::read_dir(path).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| modified_after(&entry.path(), instant))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyCleanup {
    Absent,
    Retained,
    InUse,
    Removed,
}

pub fn sweep_legacy(root: &Path, legacy: &LegacySources, now: i64) -> LegacyCleanup {
    let Ok(Some(mut file)) = StateFile::read(root) else {
        return LegacyCleanup::Absent;
    };
    let Some(record) = file.legacy.clone() else {
        return LegacyCleanup::Absent;
    };
    if Path::new(&record.source) != legacy.roaming {
        return LegacyCleanup::Absent;
    }
    let retention = i64::try_from(LEGACY_RETENTION.as_millis()).unwrap_or(i64::MAX);
    if now - record.migrated_at < retention {
        return LegacyCleanup::Retained;
    }
    let migrated = SystemTime::UNIX_EPOCH
        + Duration::from_millis(u64::try_from(record.migrated_at).unwrap_or_default());
    if modified_after(&legacy.roaming, migrated) {
        return LegacyCleanup::InUse;
    }
    let _ = std::fs::remove_dir_all(&legacy.roaming);
    if let Some(local) = &legacy.local {
        let _ = std::fs::remove_dir_all(local.join(project_struct::SHELL_PROFILE));
        let _ = std::fs::remove_dir(local);
    }
    file.legacy = None;
    let _ = file.write(root);
    LegacyCleanup::Removed
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn legacy_at(home: &Path) -> LegacySources {
        let roaming = home.join("Roaming").join("com.zvs.aistudio");
        std::fs::create_dir_all(&roaming).expect("creates");
        std::fs::write(roaming.join(DATABASE_FILE), b"").expect("writes");
        LegacySources {
            roaming,
            local: None,
        }
    }

    fn counted(_: &StepContext<'_>, progress: &dyn Fn(StepProgress)) -> Result<(), BootError> {
        progress(StepProgress { done: 1, total: 1 });
        Ok(())
    }

    fn failing(_: &StepContext<'_>, _: &dyn Fn(StepProgress)) -> Result<(), BootError> {
        Err(BootError::Io("disk".to_owned()))
    }

    #[test]
    fn an_empty_root_without_a_legacy_root_starts_fresh_and_then_is_ready() {
        let home = tempfile::tempdir().expect("a folder");
        let root = home.path().join("AIStudio");
        let boot = Boot::new(&root, None);
        assert_eq!(boot.inspect().expect("inspects"), BootDecision::Fresh);
        boot.run(&BootDecision::Fresh, 1, &|_| {}).expect("runs");
        assert_eq!(
            StateFile::read(&root).expect("reads"),
            Some(StateFile::current())
        );
        assert_eq!(boot.inspect().expect("inspects"), BootDecision::Ready);
    }

    #[test]
    fn a_legacy_root_migrates_every_step_and_records_its_source() {
        let home = tempfile::tempdir().expect("a folder");
        let root = home.path().join("AIStudio");
        let legacy = legacy_at(home.path());
        let steps = [
            Step {
                schema: PROJECT_STRUCT,
                from: 0,
                label: "a",
                run: counted,
            },
            Step {
                schema: PROJECT_STRUCT,
                from: 1,
                label: "b",
                run: counted,
            },
            Step {
                schema: PROJECT_STRUCT,
                from: 2,
                label: "c",
                run: counted,
            },
        ];
        let boot = Boot {
            root: &root,
            legacy: Some(&legacy),
            steps: &steps,
            layout: None,
        };
        let decision = boot.inspect().expect("inspects");
        assert_eq!(decision, BootDecision::Migrate(steps.to_vec()));
        let seen = Mutex::new(Vec::new());
        boot.run(&decision, 42, &|progress| {
            seen.lock().expect("locks").push(progress);
        })
        .expect("runs");
        assert_eq!(
            seen.into_inner().expect("unlocks"),
            steps
                .iter()
                .enumerate()
                .map(|(index, step)| BootProgress {
                    step: index + 1,
                    steps: steps.len(),
                    label: step.label,
                    done: 1,
                    total: 1,
                })
                .collect::<Vec<_>>()
        );
        let file = StateFile::read(&root).expect("reads").expect("a file");
        assert!(file.matches_build());
        assert_eq!(
            file.legacy,
            Some(LegacyRecord {
                source: legacy.roaming.to_string_lossy().into_owned(),
                migrated_at: 42
            })
        );
        assert_eq!(boot.inspect().expect("inspects"), BootDecision::Ready);
    }

    #[test]
    fn a_failed_step_keeps_the_recorded_version_so_the_next_start_resumes() {
        let home = tempfile::tempdir().expect("a folder");
        let root = home.path().join("AIStudio");
        let legacy = legacy_at(home.path());
        let steps = [Step {
            schema: PROJECT_STRUCT,
            from: 0,
            label: "a",
            run: failing,
        }];
        let boot = Boot {
            root: &root,
            legacy: Some(&legacy),
            steps: &steps,
            layout: None,
        };
        let decision = boot.inspect().expect("inspects");
        assert_eq!(
            boot.run(&decision, 1, &|_| {}),
            Err(BootError::Io("disk".to_owned()))
        );
        assert!(StateFile::read(&root).expect("reads").is_none());
        assert_eq!(boot.inspect().expect("inspects"), decision);
    }

    #[test]
    fn newer_unknown_and_malformed_files_refuse_to_start() {
        let home = tempfile::tempdir().expect("a folder");
        let root = home.path().join("AIStudio");
        std::fs::create_dir_all(&root).expect("creates");
        let boot = Boot::new(&root, None);
        std::fs::write(
            root.join("aistudio.toml"),
            "[schemas]\nproject_struct = 9\n",
        )
        .expect("writes");
        assert_eq!(
            boot.inspect(),
            Err(BootError::Newer {
                schema: PROJECT_STRUCT.to_owned(),
                found: 9,
                supported: 3
            })
        );
        std::fs::write(
            root.join("aistudio.toml"),
            "[schemas]\nproject_struct = 1\nprofiles = 1\n",
        )
        .expect("writes");
        assert_eq!(
            boot.inspect(),
            Err(BootError::Unknown("profiles".to_owned()))
        );
        std::fs::write(root.join("aistudio.toml"), "[schemas\n").expect("writes");
        assert!(matches!(boot.inspect(), Err(BootError::Malformed(_))));
    }

    #[test]
    fn a_root_that_already_holds_data_is_adopted_instead_of_migrated_again() {
        let home = tempfile::tempdir().expect("a folder");
        let root = home.path().join("AIStudio");
        let legacy = legacy_at(home.path());
        std::fs::create_dir_all(root.join("data")).expect("creates");
        std::fs::write(root.join("data").join(DATABASE_FILE), b"").expect("writes");
        let boot = Boot::new(&root, Some(&legacy));
        assert!(
            matches!(boot.inspect().expect("inspects"), BootDecision::Migrate(steps) if steps.iter().map(|step| step.from).eq([1, 2]))
        );
    }

    #[test]
    fn invalid_dialog_history_leaves_the_layout_pending_and_can_be_retried() {
        let home = tempfile::tempdir().expect("a folder");
        let root = home.path().join("AIStudio");
        let folder = StudioPaths::conversation_root(&root).join("dialog");
        std::fs::create_dir_all(&folder).expect("creates owned folder");
        std::fs::write(
            root.join("aistudio.toml"),
            "[schemas]\nproject_struct = 1\n",
        )
        .expect("writes previous layout");
        let log = folder.join(crate::dialog_history::HISTORY_FILE);
        std::fs::write(&log, b"{}\n").expect("writes corrupt commit");
        let boot = Boot::new(&root, None);
        let pending = boot.inspect().expect("inspects");
        assert!(
            matches!(&pending, BootDecision::Migrate(steps) if steps.iter().map(|step| step.from).eq([1, 2]))
        );
        assert!(matches!(
            boot.run(&pending, 1, &|_| {}),
            Err(BootError::DialogHistory(_))
        ));
        assert_eq!(boot.inspect().expect("remains pending"), pending);
        std::fs::write(log, b"{\"interrupted\":").expect("leaves an unfinished append");
        boot.run(&pending, 1, &|_| {}).expect("resumes validation");
        assert_eq!(
            boot.inspect().expect("inspects current layout"),
            BootDecision::Ready
        );
    }

    #[test]
    fn temporary_roots_are_emptied_but_kept() {
        let home = tempfile::tempdir().expect("a folder");
        let runs = home.path().join("temp").join("runs");
        let staging = home.path().join("temp").join("staging");
        std::fs::create_dir_all(runs.join("r1")).expect("creates");
        std::fs::write(runs.join("r1").join("out.txt"), b"x").expect("writes");
        std::fs::create_dir_all(&staging).expect("creates");
        std::fs::write(staging.join("loose.bin"), b"x").expect("writes");
        let missing = home.path().join("absent");
        assert_eq!(sweep_temp(&[runs.clone(), staging.clone(), missing]), 2);
        assert!(runs.is_dir() && staging.is_dir());
        assert_eq!(std::fs::read_dir(&runs).expect("lists").count(), 0);
    }

    #[test]
    fn the_legacy_root_is_removed_only_after_retention_and_while_unused() {
        let home = tempfile::tempdir().expect("a folder");
        let root = home.path().join("AIStudio");
        let legacy = legacy_at(home.path());
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("after the epoch")
            .as_millis() as i64;
        let retention = LEGACY_RETENTION.as_millis() as i64;
        assert_eq!(sweep_legacy(&root, &legacy, now), LegacyCleanup::Absent);
        let mut file = StateFile::current();
        file.legacy = Some(LegacyRecord {
            source: legacy.roaming.to_string_lossy().into_owned(),
            migrated_at: now - 1_000,
        });
        file.write(&root).expect("writes");
        assert_eq!(sweep_legacy(&root, &legacy, now), LegacyCleanup::Retained);
        assert_eq!(
            sweep_legacy(&root, &legacy, now + retention),
            LegacyCleanup::InUse
        );
        file.legacy = Some(LegacyRecord {
            source: legacy.roaming.to_string_lossy().into_owned(),
            migrated_at: now + 60_000,
        });
        file.write(&root).expect("writes");
        assert_eq!(
            sweep_legacy(&root, &legacy, now + 60_000 + retention),
            LegacyCleanup::Removed
        );
        assert!(!legacy.roaming.exists());
        assert!(
            StateFile::read(&root)
                .expect("reads")
                .expect("a file")
                .legacy
                .is_none()
        );
    }
}
