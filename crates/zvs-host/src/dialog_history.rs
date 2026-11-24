use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zvs_contract::ChatToolCallDto;

use crate::conversations::ConversationFolders;
use crate::data::schema::{CompactionRow, ConversationRow, MessageRow};
use crate::platform::long_path::extended;

pub const HISTORY_FILE: &str = "history.jsonl";
pub const HISTORY_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DialogChange {
    Metadata(Box<ConversationRow>),
    Message(Box<MessageRow>),
    StreamDelta {
        id: String,
        content: String,
        reasoning: String,
        tokens_out: i64,
        duration_ms: i64,
    },
    ToolCall {
        id: String,
        index: u32,
        call: Box<ChatToolCallDto>,
    },
    RemoveMessage {
        id: String,
    },
    Compaction(Box<CompactionRow>),
    RemoveCompaction {
        id: String,
    },
    Delete,
}

#[derive(Serialize, Deserialize)]
struct Commit<T> {
    version: u32,
    dialog: String,
    changes: T,
}

pub struct DialogHistory {
    folders: ConversationFolders,
}

impl DialogHistory {
    pub fn new(root: PathBuf) -> Self {
        Self {
            folders: ConversationFolders::new(root),
        }
    }

    pub fn replay(
        &self,
        id: &str,
        visit: impl FnMut(Vec<DialogChange>) -> io::Result<()>,
    ) -> io::Result<u64> {
        let path = self.folder(id)?.join(HISTORY_FILE);
        let file = match File::open(extended(&path)) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error),
        };
        replay(file, id, visit)
    }

    pub fn dialogs(&self) -> io::Result<Vec<String>> {
        let entries = match std::fs::read_dir(extended(self.folders.root())) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut ids = Vec::new();
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && let Ok(id) = entry.file_name().into_string()
                && crate::conversations::is_owner(&id)
                && entry.path().join(HISTORY_FILE).try_exists()?
            {
                ids.push(id);
            }
        }
        ids.sort_unstable();
        Ok(ids)
    }

    pub fn writer(&self, id: &str) -> io::Result<DialogWriter> {
        let folder = self.folder(id)?;
        let locks = self.folders.root().join(".locks");
        std::fs::create_dir_all(extended(&locks))?;
        let lock = open(&locks.join(id))?;
        lock.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => io::Error::from(io::ErrorKind::WouldBlock),
            TryLockError::Error(error) => error,
        })?;
        Ok(DialogWriter {
            id: id.to_owned(),
            folder,
            _lock: lock,
        })
    }

    fn folder(&self, id: &str) -> io::Result<PathBuf> {
        self.folders
            .folder(id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid dialog ID"))
    }
}

pub struct DialogWriter {
    id: String,
    folder: PathBuf,
    _lock: File,
}

impl DialogWriter {
    pub(crate) fn clean_deleted_folder(&self) {
        let _ = crate::conversations::remove_dialog_folder(&self.folder);
    }

    pub(crate) fn rollback(&self, from: u64, end: u64) -> io::Result<()> {
        let file = open(&self.folder.join(HISTORY_FILE))?;
        if file.metadata()?.len() != end {
            return Err(stale_index());
        }
        file.set_len(from)?;
        file.sync_all()
    }

    pub fn append(&self, offset: u64, changes: &[DialogChange]) -> io::Result<u64> {
        if changes.is_empty() {
            return Ok(offset);
        }
        let mut line = serde_json::to_vec(&Commit {
            version: HISTORY_VERSION,
            dialog: self.id.clone(),
            changes,
        })?;
        line.push(b'\n');
        std::fs::create_dir_all(extended(&self.folder))?;
        let mut file = open(&self.folder.join(HISTORY_FILE))?;
        if file.metadata()?.len() < offset {
            return Err(stale_index());
        }
        if offset > 0 {
            file.seek(SeekFrom::Start(offset - 1))?;
            let mut boundary = [0];
            file.read_exact(&mut boundary)?;
            if boundary[0] != b'\n' {
                return Err(stale_index());
            }
        }
        if file.metadata()?.len() != offset {
            file.seek(SeekFrom::Start(offset))?;
            let mut reader = BufReader::new(&mut file);
            let mut tail = Vec::new();
            reader.read_until(b'\n', &mut tail)?;
            if tail.last() == Some(&b'\n') {
                return Err(stale_index());
            }
            file.set_len(offset)?;
        }
        file.seek(SeekFrom::Start(offset))?;
        if let Err(error) = file.write_all(&line).and_then(|()| file.sync_all()) {
            file.set_len(offset)?;
            file.sync_all()?;
            return Err(error);
        }
        Ok(offset + line.len() as u64)
    }
}

fn replay(
    file: File,
    id: &str,
    mut visit: impl FnMut(Vec<DialogChange>) -> io::Result<()>,
) -> io::Result<u64> {
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    let mut offset = 0;
    while reader.read_until(b'\n', &mut line)? > 0 && line.last() == Some(&b'\n') {
        let commit: Commit<Vec<DialogChange>> = serde_json::from_slice(&line)?;
        if commit.version != HISTORY_VERSION || commit.dialog != id || commit.changes.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid dialog commit",
            ));
        }
        visit(commit.changes)?;
        offset += line.len() as u64;
        line.clear();
    }
    Ok(offset)
}

fn open(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(extended(path))
}

fn stale_index() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Dialog index is stale")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::temporary_directory;

    #[test]
    fn readers_see_complete_commits_and_writers_are_exclusive_per_dialog() {
        let root = temporary_directory("dialog-history");
        let first = DialogHistory::new(root.path().to_path_buf());
        let second = DialogHistory::new(root.path().to_path_buf());
        let writer = first.writer("a").expect("opens writer");
        assert!(
            matches!(second.writer("a"), Err(error) if error.kind() == io::ErrorKind::WouldBlock)
        );
        let other = second.writer("b").expect("unrelated dialog");
        let offset = writer.append(0, &[DialogChange::Delete]).expect("appends");
        let other_offset = other.append(0, &[DialogChange::Delete]).expect("appends");
        let path = root.path().join("a").join(HISTORY_FILE);
        let mut file = OpenOptions::new()
            .append(true)
            .open(path)
            .expect("opens log");
        file.write_all(b"{\"interrupted\":").expect("writes tail");
        drop(file);
        let mut seen = 0;
        assert_eq!(
            second
                .replay("a", |changes| {
                    assert!(matches!(changes.as_slice(), [DialogChange::Delete]));
                    seen += 1;
                    Ok(())
                })
                .expect("replays"),
            offset
        );
        assert_eq!(seen, 1);
        let recovered = writer
            .append(offset, &[DialogChange::Delete])
            .expect("replaces tail");
        assert_eq!(first.replay("a", |_| Ok(())).expect("replays"), recovered);
        assert_eq!(
            first.replay("b", |_| Ok(())).expect("replays"),
            other_offset
        );
        assert!(writer.append(offset, &[DialogChange::Delete]).is_err());
        drop(writer);
        assert!(second.writer("a").is_ok());
    }

    #[test]
    fn a_corrupt_complete_record_is_reported() {
        let root = temporary_directory("dialog-history");
        let history = DialogHistory::new(root.path().to_path_buf());
        let folder = root.path().join("a");
        std::fs::create_dir(&folder).expect("creates folder");
        std::fs::write(folder.join(HISTORY_FILE), b"{}\n").expect("writes invalid record");
        assert!(history.replay("a", |_| Ok(())).is_err());
    }
}
