use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;
use zvs_contract::HostEvent;

use super::clock::SharedClock;
use super::events::EventRecorder;
use super::logger::{LogLevel, SharedLogger};
use super::paths::stream_file_path;

pub const RECORD_FLAG: &str = "ZVS_RECORD_EVENTS";

pub fn recording_enabled(value: Option<&str>) -> bool {
    value == Some("1")
}

pub fn recording_enabled_from_env() -> bool {
    recording_enabled(std::env::var(RECORD_FLAG).ok().as_deref())
}

pub struct FileEventRecorder {
    directory: PathBuf,
    logger: SharedLogger,
    clock: SharedClock,
    closed: AtomicBool,
}

impl FileEventRecorder {
    pub fn open(directory: &Path, logger: SharedLogger, clock: SharedClock) -> io::Result<Self> {
        fs::create_dir_all(directory)?;
        Ok(Self {
            directory: directory.to_path_buf(),
            logger,
            clock,
            closed: AtomicBool::new(false),
        })
    }

    fn write(&self, event: &HostEvent) -> io::Result<()> {
        let mut line = serde_json::to_string(event).map_err(io::Error::other)?;
        line.push('\n');
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(stream_file_path(&self.directory, self.clock.now()))?
            .write_all(line.as_bytes())
    }
}

impl EventRecorder for FileEventRecorder {
    fn append(&self, event: &HostEvent) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        if let Err(error) = self.write(event) {
            self.closed.store(true, Ordering::SeqCst);
            self.logger.log(
                LogLevel::Error,
                "events",
                "Stopped recording, the file is unwritable",
                &[
                    ("directory", json!(self.directory.display().to_string())),
                    ("error", json!(error.to_string())),
                ],
            );
        }
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use zvs_contract::{HostEventDraft, RunOutcomeDto, RunOutcomeStatus};

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::{EventBus, EventBusOptions};
    use crate::platform::logger::silent_logger;
    use crate::testing::{MemoryLogger, temporary_directory};

    #[test]
    fn a_recorded_session_reads_back_as_the_same_sequence() {
        let temp = temporary_directory("studio-streams-");
        let clock = Arc::new(FakeClock::default());
        let recorder =
            FileEventRecorder::open(temp.path(), silent_logger(), clock.clone()).expect("opens");
        let bus = EventBus::new(EventBusOptions {
            clock,
            recorder: Some(Box::new(recorder)),
            ..EventBusOptions::default()
        });
        let stream = bus.open_stream().expect("opens");
        for done in 1..=5 {
            stream.emit(HostEventDraft::Progress { done, total: 5 });
        }
        stream.end(RunOutcomeDto {
            status: RunOutcomeStatus::Ok,
            code: None,
            message: None,
        });
        bus.dispose();

        let files: Vec<String> = fs::read_dir(temp.path())
            .expect("lists")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(files, ["2023-11-14.jsonl"]);
        let text = fs::read_to_string(temp.join(&files[0])).expect("reads");
        let events: Vec<HostEvent> = text
            .lines()
            .map(|line| serde_json::from_str(line).expect("a host event"))
            .collect();
        assert_eq!(
            events.iter().map(|event| event.seq).collect::<Vec<_>>(),
            [0, 1, 2, 3, 4, 5]
        );
        assert_eq!(
            events
                .iter()
                .filter_map(|event| match event.body {
                    HostEventDraft::Progress { done, .. } => Some(done),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [1, 2, 3, 4, 5]
        );
        assert!(matches!(
            events.last().map(|event| &event.body),
            Some(HostEventDraft::End { .. })
        ));
        assert!(events.iter().all(|event| event.stream_id == stream.id()));
    }

    #[test]
    fn an_unwritable_directory_stops_recording_with_one_error() {
        let temp = temporary_directory("studio-streams-gone-");
        let logger = MemoryLogger::shared();
        let recorder = FileEventRecorder::open(
            &temp.join("streams"),
            logger.clone(),
            Arc::new(FakeClock::default()),
        )
        .expect("opens");
        fs::remove_dir(temp.join("streams")).expect("removes");
        fs::write(temp.join("streams"), b"a file, not a folder").expect("writes");
        let bus = EventBus::new(EventBusOptions {
            recorder: Some(Box::new(recorder)),
            ..EventBusOptions::default()
        });
        let stream = bus.open_stream().expect("opens");
        stream.emit(HostEventDraft::Progress { done: 1, total: 2 });
        stream.emit(HostEventDraft::Progress { done: 2, total: 2 });
        assert_eq!(logger.matching("Stopped recording").len(), 1);
    }

    #[test]
    fn recording_is_opt_in_through_the_environment_flag() {
        assert_eq!(RECORD_FLAG, "ZVS_RECORD_EVENTS");
        assert!(!recording_enabled(None));
        assert!(!recording_enabled(Some("0")));
        assert!(!recording_enabled(Some("true")));
        assert!(recording_enabled(Some("1")));
    }
}
