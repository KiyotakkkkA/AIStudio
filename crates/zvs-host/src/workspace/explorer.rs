use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use notify::event::{ModifyKind, RenameMode};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::Value;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tokio::time::{Instant, sleep_until};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, AppErrorCode, ConversationId, EXPLORER_CHANGES_MAX, EXPLORER_LIST_MAX,
    EXPLORER_STEP_DOMAIN, EXPLORER_TEXT_MAX, ExplorerChangeDto, ExplorerChangeKind,
    ExplorerChangesDto, ExplorerEntryDto, ExplorerEntryKind, ExplorerFileDto, ExplorerListingDto,
    ExplorerWatchDto, HostEventDraft, RunOutcomeDto, RunOutcomeStatus,
};

use super::paths::{canonical, covers, relative_to};
use super::service::{FOLDER_ACCESS_DENIED, FolderAccessService, NOT_A_FOLDER};
use crate::files::text::{Decoded, decode};
use crate::files::walk::{GIT_FOLDER, IgnoreRules};
use crate::indexing::walk::locale_order;
use crate::platform::events::{EventBus, StreamHandle};
use crate::platform::long_path::extended;

pub const EXPLORER_DEBOUNCE: Duration = Duration::from_millis(300);
pub const EXPLORER_MAX_WAIT: Duration = Duration::from_secs(2);
pub const NO_WORKING_FOLDER_BOUND: &str = "У диалога нет рабочей папки";
pub const OUTSIDE_WORKING_FOLDER: &str = "Путь выходит за пределы рабочей папки";
pub const ENTRY_NOT_FOUND: &str = "Файл или папка не найдены";
pub const NOT_A_FILE: &str = "Путь не является файлом";
pub const ENTRY_UNREADABLE: &str = "Не удалось прочитать файл или папку";
pub const WATCH_FAILED: &str = "Не удалось отслеживать изменения в рабочей папке";

struct Active {
    cancel: CancellationToken,
    _watcher: RecommendedWatcher,
}

struct Watch {
    stream: Arc<StreamHandle>,
    subscribers: usize,
    active: Option<Active>,
}

struct Inner {
    folders: FolderAccessService,
    events: EventBus,
    debounce: Duration,
    watches: Mutex<HashMap<ConversationId, Watch>>,
}

#[derive(Clone)]
pub struct ExplorerService {
    inner: Arc<Inner>,
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn outside(relative: &str) -> AppError {
    AppError::new(AppErrorCode::PermissionDenied, OUTSIDE_WORKING_FOLDER)
        .with_detail("path", relative.to_owned())
}

fn io_failure(relative: &str, error: &std::io::Error) -> AppError {
    let (code, message) = match error.kind() {
        std::io::ErrorKind::NotFound => (AppErrorCode::NotFound, ENTRY_NOT_FOUND),
        std::io::ErrorKind::PermissionDenied => (AppErrorCode::PermissionDenied, ENTRY_UNREADABLE),
        _ => (AppErrorCode::ValidationFailed, ENTRY_UNREADABLE),
    };
    AppError::new(code, message).with_detail("path", relative.to_owned())
}

fn watch_failed(error: &notify::Error) -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, WATCH_FAILED)
        .with_detail("reason", error.to_string())
}

fn parts(relative: &str) -> Result<Vec<&str>, AppError> {
    if relative.is_empty() {
        return Ok(Vec::new());
    }
    relative
        .split(['/', '\\'])
        .map(|part| {
            if part.is_empty() || part == "." || part == ".." || part.contains(':') {
                Err(outside(relative))
            } else {
                Ok(part)
            }
        })
        .collect()
}

fn joined(parts: &[&str], name: &str) -> String {
    let mut text = parts.join("/");
    if !text.is_empty() {
        text.push('/');
    }
    text.push_str(name);
    text
}

fn millis(time: std::io::Result<SystemTime>) -> Option<i64> {
    time.ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
}

fn is_git(name: &str) -> bool {
    name.eq_ignore_ascii_case(GIT_FOLDER)
}

fn relative_of(root: &Path, path: &Path) -> Option<String> {
    let relative = relative_to(root, path)?;
    let mut names: Vec<String> = Vec::new();
    for component in relative.components() {
        let name = component.as_os_str().to_string_lossy().into_owned();
        if is_git(&name) {
            return None;
        }
        names.push(name);
    }
    Some(names.join("/"))
}

fn merged(previous: ExplorerChangeKind, next: ExplorerChangeKind) -> Option<ExplorerChangeKind> {
    use ExplorerChangeKind::{Created, Modified, Removed};
    match (previous, next) {
        (Created, Removed) => None,
        (Created, _) => Some(Created),
        (Removed, Created | Modified) => Some(Modified),
        (_, next) => Some(next),
    }
}

#[derive(Default)]
struct Batch {
    order: Vec<String>,
    kinds: HashMap<String, ExplorerChangeKind>,
}

impl Batch {
    fn add(&mut self, path: String, kind: ExplorerChangeKind) {
        match self.kinds.get(&path).copied() {
            Some(previous) => match merged(previous, kind) {
                Some(kind) => {
                    self.kinds.insert(path, kind);
                }
                None => {
                    self.kinds.remove(&path);
                    self.order.retain(|known| known != &path);
                }
            },
            None => {
                self.order.push(path.clone());
                self.kinds.insert(path, kind);
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    fn root_removed(&self) -> bool {
        self.kinds.get("") == Some(&ExplorerChangeKind::Removed)
    }

    fn take(&mut self) -> Vec<ExplorerChangeDto> {
        let kinds = std::mem::take(&mut self.kinds);
        let changes: Vec<ExplorerChangeDto> = std::mem::take(&mut self.order)
            .into_iter()
            .filter_map(|path| {
                kinds
                    .get(&path)
                    .map(|kind| ExplorerChangeDto { path, kind: *kind })
            })
            .collect();
        if changes.len() > EXPLORER_CHANGES_MAX {
            return vec![ExplorerChangeDto {
                path: String::new(),
                kind: ExplorerChangeKind::Modified,
            }];
        }
        changes
    }
}

fn classify(root: &Path, event: &notify::Event) -> Vec<(String, ExplorerChangeKind)> {
    let present = |path: &Path| std::fs::symlink_metadata(extended(path)).is_ok();
    let changes: Vec<(&PathBuf, ExplorerChangeKind)> = match event.kind {
        EventKind::Access(_) => Vec::new(),
        EventKind::Create(_) => event
            .paths
            .iter()
            .map(|path| (path, ExplorerChangeKind::Created))
            .collect(),
        EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => event
            .paths
            .iter()
            .map(|path| (path, ExplorerChangeKind::Removed))
            .collect(),
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => event
            .paths
            .iter()
            .map(|path| (path, ExplorerChangeKind::Created))
            .collect(),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => event
            .paths
            .iter()
            .enumerate()
            .map(|(index, path)| {
                if index == 0 {
                    (path, ExplorerChangeKind::Removed)
                } else {
                    (path, ExplorerChangeKind::Created)
                }
            })
            .collect(),
        EventKind::Modify(ModifyKind::Name(_)) => event
            .paths
            .iter()
            .map(|path| {
                if present(path) {
                    (path, ExplorerChangeKind::Created)
                } else {
                    (path, ExplorerChangeKind::Removed)
                }
            })
            .collect(),
        EventKind::Modify(_) | EventKind::Any | EventKind::Other => event
            .paths
            .iter()
            .filter(|path| !extended(path).is_dir())
            .map(|path| {
                if present(path) {
                    (path, ExplorerChangeKind::Modified)
                } else {
                    (path, ExplorerChangeKind::Removed)
                }
            })
            .collect(),
    };
    changes
        .into_iter()
        .filter_map(|(path, kind)| relative_of(root, path).map(|relative| (relative, kind)))
        .filter(|(relative, kind)| !relative.is_empty() || *kind == ExplorerChangeKind::Removed)
        .collect()
}

fn emit(stream: &StreamHandle, conversation: ConversationId, changes: Vec<ExplorerChangeDto>) {
    if changes.is_empty() || stream.closed() {
        return;
    }
    let dto = ExplorerChangesDto {
        domain: EXPLORER_STEP_DOMAIN.to_owned(),
        conversation_id: conversation,
        changes,
    };
    if let Ok(Value::Object(step)) = serde_json::to_value(&dto) {
        stream.emit(HostEventDraft::Step { step });
    }
}

fn root_change(kind: ExplorerChangeKind) -> Vec<ExplorerChangeDto> {
    vec![ExplorerChangeDto {
        path: String::new(),
        kind,
    }]
}

async fn pump(
    mut receiver: UnboundedReceiver<notify::Result<notify::Event>>,
    root: PathBuf,
    conversation: ConversationId,
    stream: Arc<StreamHandle>,
    debounce: Duration,
    cancel: CancellationToken,
) {
    let mut batch = Batch::default();
    let mut first: Option<Instant> = None;
    let mut deadline: Option<Instant> = None;
    loop {
        let received = if let Some(at) = deadline {
            tokio::select! {
                () = cancel.cancelled() => return,
                () = sleep_until(at) => {
                    emit(&stream, conversation, batch.take());
                    first = None;
                    deadline = None;
                    continue;
                }
                received = receiver.recv() => received,
            }
        } else {
            tokio::select! {
                () = cancel.cancelled() => return,
                received = receiver.recv() => received,
            }
        };
        let Some(received) = received else {
            return;
        };
        match received {
            Ok(event) => {
                for (path, kind) in classify(&root, &event) {
                    batch.add(path, kind);
                }
            }
            Err(_) => batch.add(String::new(), ExplorerChangeKind::Removed),
        }
        if !extended(&root).is_dir() {
            batch.add(String::new(), ExplorerChangeKind::Removed);
        }
        if batch.root_removed() {
            emit(
                &stream,
                conversation,
                root_change(ExplorerChangeKind::Removed),
            );
            return;
        }
        if batch.is_empty() {
            first = None;
            deadline = None;
            continue;
        }
        let now = Instant::now();
        let started = *first.get_or_insert(now);
        deadline = Some((now + debounce).min(started + EXPLORER_MAX_WAIT));
    }
}

impl ExplorerService {
    pub fn new(folders: FolderAccessService, events: EventBus) -> Self {
        Self::with_debounce(folders, events, EXPLORER_DEBOUNCE)
    }

    pub fn with_debounce(
        folders: FolderAccessService,
        events: EventBus,
        debounce: Duration,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                folders,
                events,
                debounce,
                watches: Mutex::new(HashMap::new()),
            }),
        }
    }

    fn watches(&self) -> std::sync::MutexGuard<'_, HashMap<ConversationId, Watch>> {
        self.inner
            .watches
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub fn root(&self, conversation: &ConversationId) -> Result<PathBuf, AppError> {
        let id = conversation.to_string();
        let folder = self
            .inner
            .folders
            .working_folder_of(&id)?
            .ok_or_else(|| AppError::new(AppErrorCode::Conflict, NO_WORKING_FOLDER_BOUND))?;
        let folder = self.inner.folders.working_folder(&folder)?;
        if !self.inner.folders.covered(&id, &folder)? {
            return Err(
                AppError::new(AppErrorCode::PermissionDenied, FOLDER_ACCESS_DENIED)
                    .with_detail("path", path_text(&folder)),
            );
        }
        Ok(folder)
    }

    fn inside(root: &Path, relative: &str) -> Result<(PathBuf, Vec<String>), AppError> {
        let names = parts(relative)?;
        let mut path = root.to_path_buf();
        for name in &names {
            path.push(name);
        }
        let resolved = canonical(&path).map_err(|error| io_failure(relative, &error))?;
        if !covers(root, &resolved) {
            return Err(outside(relative));
        }
        Ok((resolved, names.into_iter().map(str::to_owned).collect()))
    }

    pub fn list(
        &self,
        conversation: &ConversationId,
        relative: &str,
    ) -> Result<ExplorerListingDto, AppError> {
        let root = self.root(conversation)?;
        let (folder, names) = Self::inside(&root, relative)?;
        if !extended(&folder).is_dir() {
            return Err(AppError::new(AppErrorCode::ValidationFailed, NOT_A_FOLDER)
                .with_detail("path", relative.to_owned()));
        }
        let mut rules = IgnoreRules::for_root(&root);
        let mut inherited = false;
        let mut chain = root.clone();
        rules.load(&chain);
        for name in &names {
            chain.push(name);
            inherited = inherited || rules.ignored(&chain, true);
            rules.load(&chain);
        }
        let children =
            std::fs::read_dir(extended(&folder)).map_err(|error| io_failure(relative, &error))?;
        let prefix: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut entries: Vec<ExplorerEntryDto> = children
            .filter_map(Result::ok)
            .filter_map(|child| {
                let name = child.file_name().to_string_lossy().into_owned();
                if is_git(&name) {
                    return None;
                }
                let path = folder.join(&name);
                let metadata = std::fs::metadata(extended(&path)).ok()?;
                let is_folder = metadata.is_dir();
                Some(ExplorerEntryDto {
                    path: joined(&prefix, &name),
                    kind: if is_folder {
                        ExplorerEntryKind::Folder
                    } else {
                        ExplorerEntryKind::File
                    },
                    size: if is_folder { 0 } else { metadata.len() },
                    modified_at: millis(metadata.modified()),
                    ignored: inherited || rules.ignored(&path, is_folder),
                    name,
                })
            })
            .collect();
        entries.sort_by(|left, right| {
            (left.kind != ExplorerEntryKind::Folder)
                .cmp(&(right.kind != ExplorerEntryKind::Folder))
                .then_with(|| locale_order(&left.name, &right.name))
        });
        let truncated = entries.len() > EXPLORER_LIST_MAX;
        entries.truncate(EXPLORER_LIST_MAX);
        Ok(ExplorerListingDto {
            path: prefix.join("/"),
            entries,
            truncated,
        })
    }

    pub fn read(
        &self,
        conversation: &ConversationId,
        relative: &str,
    ) -> Result<ExplorerFileDto, AppError> {
        let root = self.root(conversation)?;
        let (path, names) = Self::inside(&root, relative)?;
        let metadata =
            std::fs::metadata(extended(&path)).map_err(|error| io_failure(relative, &error))?;
        if !metadata.is_file() {
            return Err(AppError::new(AppErrorCode::ValidationFailed, NOT_A_FILE)
                .with_detail("path", relative.to_owned()));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(extended(&path))
            .and_then(|file| {
                file.take(EXPLORER_TEXT_MAX as u64 + 1)
                    .read_to_end(&mut bytes)
            })
            .map_err(|error| io_failure(relative, &error))?;
        let truncated = bytes.len() > EXPLORER_TEXT_MAX;
        if truncated {
            bytes.truncate(EXPLORER_TEXT_MAX);
            match bytes.iter().rposition(|byte| *byte == b'\n') {
                Some(end) => bytes.truncate(end + 1),
                None => {
                    if let Err(error) = std::str::from_utf8(&bytes)
                        && error.error_len().is_none()
                    {
                        bytes.truncate(error.valid_up_to());
                    }
                }
            }
        }
        let text = match decode(&bytes) {
            Decoded::Text { text, .. } => Some(text),
            Decoded::Binary => None,
        };
        Ok(ExplorerFileDto {
            path: names.join("/"),
            size: metadata.len(),
            modified_at: millis(metadata.modified()),
            truncated: truncated && text.is_some(),
            text,
        })
    }

    fn start(
        &self,
        conversation: ConversationId,
        root: PathBuf,
        stream: &Arc<StreamHandle>,
    ) -> Result<Active, AppError> {
        let (sender, receiver) = unbounded_channel();
        let mut watcher = notify::recommended_watcher(move |result| {
            let _ = sender.send(result);
        })
        .map_err(|error| watch_failed(&error))?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|error| watch_failed(&error))?;
        let cancel = CancellationToken::new();
        tokio::spawn(pump(
            receiver,
            root,
            conversation,
            Arc::clone(stream),
            self.inner.debounce,
            cancel.clone(),
        ));
        Ok(Active {
            cancel,
            _watcher: watcher,
        })
    }

    pub fn watch(&self, conversation: &ConversationId) -> Result<ExplorerWatchDto, AppError> {
        let mut watches = self.watches();
        if let Some(watch) = watches.get_mut(conversation) {
            if watch.active.is_none() {
                let root = self.root(conversation)?;
                watch.active = Some(self.start(*conversation, root, &watch.stream)?);
            }
            watch.subscribers += 1;
            return Ok(ExplorerWatchDto {
                stream_id: watch.stream.id(),
            });
        }
        let root = self.root(conversation)?;
        let stream = Arc::new(self.inner.events.open_stream()?);
        let active = self.start(*conversation, root, &stream)?;
        let stream_id = stream.id();
        watches.insert(
            *conversation,
            Watch {
                stream,
                subscribers: 1,
                active: Some(active),
            },
        );
        Ok(ExplorerWatchDto { stream_id })
    }

    pub fn unwatch(&self, conversation: &ConversationId) {
        let mut watches = self.watches();
        let Some(watch) = watches.get_mut(conversation) else {
            return;
        };
        watch.subscribers = watch.subscribers.saturating_sub(1);
        if watch.subscribers > 0 {
            return;
        }
        if let Some(watch) = watches.remove(conversation) {
            Self::close(watch);
        }
    }

    fn close(watch: Watch) {
        if let Some(active) = watch.active {
            active.cancel.cancel();
        }
        watch.stream.end(RunOutcomeDto {
            status: RunOutcomeStatus::Ok,
            code: None,
            message: None,
        });
    }

    pub fn rebind(&self, conversation: &ConversationId) {
        let mut watches = self.watches();
        let Some(watch) = watches.get_mut(conversation) else {
            return;
        };
        if let Some(active) = watch.active.take() {
            active.cancel.cancel();
        }
        let started = self
            .root(conversation)
            .and_then(|root| self.start(*conversation, root, &watch.stream));
        match started {
            Ok(active) => {
                watch.active = Some(active);
                emit(
                    &watch.stream,
                    *conversation,
                    root_change(ExplorerChangeKind::Modified),
                );
            }
            Err(_) => emit(
                &watch.stream,
                *conversation,
                root_change(ExplorerChangeKind::Removed),
            ),
        }
    }

    pub fn watching(&self, conversation: &ConversationId) -> usize {
        self.watches()
            .get(conversation)
            .map_or(0, |watch| watch.subscribers)
    }

    pub fn dispose(&self) {
        let drained: Vec<Watch> = self.watches().drain().map(|(_, watch)| watch).collect();
        for watch in drained {
            Self::close(watch);
        }
    }
}
