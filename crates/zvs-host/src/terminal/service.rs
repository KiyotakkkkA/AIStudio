use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use serde_json::Value;
use zvs_contract::validate::invalid;
use zvs_contract::{
    AppError, AppErrorCode, ConversationId, HostEventDraft, RunOutcomeDto, RunOutcomeStatus,
    TERMINAL_SCROLLBACK_MAX, TERMINAL_SESSIONS_MAX, TERMINAL_STEP_DOMAIN, TerminalAttachDto,
    TerminalCreateInput, TerminalId, TerminalSessionDto, TerminalShell, TerminalStatus,
    TerminalStatusDto, Timestamp,
};

use super::pty::{PtyChild, PtyControl, PtySpawner, PtySpec};
use super::shells::{SHELL_ORDER, ShellLocator, shell_title};
use crate::platform::clock::SharedClock;
use crate::platform::events::{EventBus, StreamHandle};
use crate::platform::ids::new_id;
use crate::toolchain::CommandLauncher;
use crate::toolchain::process::joined_path;
use crate::workspace::FolderAccessService;

pub const SESSION_MISSING: &str = "Сеанс терминала не найден";
pub const SESSION_EXITED: &str = "Сеанс терминала завершён";
pub const SESSIONS_LIMIT: &str = "Открыто максимальное число сеансов терминала для этого диалога";
pub const NO_SHELL: &str = "Командная оболочка не найдена";
pub const SHELL_FAILED: &str = "Не удалось запустить командную оболочку";

const READ_CHUNK: usize = 8192;
const CURSOR_QUERY: &str = "\u{1b}[6n";
const CURSOR_REPLY: &[u8] = b"\x1b[1;1R";
const TERMINAL_ENV: [(&str, &str); 2] = [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];

pub struct TerminalServiceOptions {
    pub folders: FolderAccessService,
    pub events: EventBus,
    pub clock: SharedClock,
    pub spawner: Arc<dyn PtySpawner>,
    pub shells: Arc<dyn ShellLocator>,
    pub launcher: Option<Arc<dyn CommandLauncher>>,
    pub home: PathBuf,
}

#[derive(Clone)]
pub struct TerminalService {
    inner: Arc<Inner>,
}

struct Inner {
    options: TerminalServiceOptions,
    sessions: Mutex<BTreeMap<TerminalId, Arc<Session>>>,
}

struct Session {
    id: TerminalId,
    conversation_id: ConversationId,
    shell: TerminalShell,
    cwd: PathBuf,
    created_at: Timestamp,
    stream: StreamHandle,
    state: Mutex<SessionState>,
    writer: Mutex<Option<Box<dyn Write + Send>>>,
}

struct SessionState {
    title: String,
    status: TerminalStatus,
    exit_code: Option<i32>,
    cols: u16,
    rows: u16,
    scrollback: String,
    next_seq: u64,
    generation: u64,
    control: Option<Arc<dyn PtyControl>>,
    closed: bool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn missing() -> AppError {
    AppError::new(AppErrorCode::NotFound, SESSION_MISSING)
}

fn incomplete_tail(bytes: &[u8]) -> usize {
    for back in 1..=bytes.len().min(3) {
        let byte = bytes[bytes.len() - back];
        if byte & 0b1100_0000 == 0b1000_0000 {
            continue;
        }
        let needed = match byte {
            0b1111_0000..=0b1111_0111 => 4,
            0b1110_0000..=0b1110_1111 => 3,
            0b1100_0000..=0b1101_1111 => 2,
            _ => 1,
        };
        return if needed > back { back } else { 0 };
    }
    0
}

pub fn decode(pending: &mut Vec<u8>) -> String {
    let keep = incomplete_tail(pending);
    let tail = pending.split_off(pending.len() - keep);
    let text = String::from_utf8_lossy(pending).into_owned();
    *pending = tail;
    text
}

pub fn trim_scrollback(scrollback: &mut String, max: usize) {
    if scrollback.len() <= max {
        return;
    }
    let mut cut = scrollback.len() - max;
    while !scrollback.is_char_boundary(cut) {
        cut += 1;
    }
    if let Some(line) = scrollback[cut..].find('\n') {
        cut += line + 1;
    }
    scrollback.drain(..cut);
}

impl Session {
    fn dto(&self, state: &SessionState) -> TerminalSessionDto {
        TerminalSessionDto {
            id: self.id,
            conversation_id: self.conversation_id,
            shell: self.shell,
            title: state.title.clone(),
            cwd: path_text(&self.cwd),
            status: state.status,
            exit_code: state.exit_code,
            stream_id: self.stream.id(),
            cols: state.cols,
            rows: state.rows,
            created_at: self.created_at,
        }
    }

    fn emit(&self, state: &mut SessionState, draft: HostEventDraft) {
        if state.closed {
            return;
        }
        self.stream.emit(draft);
        state.next_seq += 1;
    }

    fn emit_status(&self, state: &mut SessionState) {
        let dto = TerminalStatusDto {
            domain: TERMINAL_STEP_DOMAIN.to_owned(),
            id: self.id,
            status: state.status,
            exit_code: state.exit_code,
        };
        if let Ok(Value::Object(step)) = serde_json::to_value(&dto) {
            self.emit(state, HostEventDraft::Step { step });
        }
    }

    fn output(&self, generation: u64, text: String) {
        if text.is_empty() {
            return;
        }
        let mut state = lock(&self.state);
        if state.generation != generation || state.closed {
            return;
        }
        state.scrollback.push_str(&text);
        trim_scrollback(&mut state.scrollback, TERMINAL_SCROLLBACK_MAX);
        self.emit(
            &mut state,
            HostEventDraft::Token {
                delta: text,
                kind: None,
            },
        );
    }

    fn exited(&self, generation: u64, code: Option<i32>) {
        let mut state = lock(&self.state);
        if state.generation != generation || state.closed {
            return;
        }
        state.status = TerminalStatus::Exited;
        state.exit_code = code;
        state.control = None;
        lock(&self.writer).take();
        self.emit_status(&mut state);
    }

    fn reply(&self, generation: u64, bytes: &[u8]) {
        if lock(&self.state).generation != generation {
            return;
        }
        if let Some(writer) = lock(&self.writer).as_mut() {
            let _ = writer.write_all(bytes).and_then(|()| writer.flush());
        }
    }
}

fn read_loop(session: Arc<Session>, generation: u64, mut reader: Box<dyn Read + Send>) {
    let mut buffer = [0_u8; READ_CHUNK];
    let mut pending = Vec::new();
    let mut answered = false;
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                pending.extend_from_slice(&buffer[..read]);
                let mut text = decode(&mut pending);
                if !answered && text.contains(CURSOR_QUERY) {
                    answered = true;
                    text = text.replacen(CURSOR_QUERY, "", 1);
                    session.reply(generation, CURSOR_REPLY);
                }
                session.output(generation, text);
            }
        }
    }
    if !pending.is_empty() {
        session.output(generation, String::from_utf8_lossy(&pending).into_owned());
    }
}

fn wait_loop(
    session: Arc<Session>,
    generation: u64,
    mut child: Box<dyn PtyChild>,
    control: Arc<dyn PtyControl>,
    reader: JoinHandle<()>,
) {
    let code = child.wait();
    control.release();
    let _ = reader.join();
    session.exited(generation, code);
}

impl TerminalService {
    pub fn new(options: TerminalServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                sessions: Mutex::new(BTreeMap::new()),
            }),
        }
    }

    fn sessions(&self) -> MutexGuard<'_, BTreeMap<TerminalId, Arc<Session>>> {
        lock(&self.inner.sessions)
    }

    fn session(&self, id: &TerminalId) -> Result<Arc<Session>, AppError> {
        self.sessions().get(id).cloned().ok_or_else(missing)
    }

    pub fn shells(&self) -> Vec<TerminalShell> {
        SHELL_ORDER
            .into_iter()
            .filter(|shell| self.inner.options.shells.locate(*shell).is_some())
            .collect()
    }

    pub fn list(&self, conversation: &ConversationId) -> Vec<TerminalSessionDto> {
        let mut sessions: Vec<Arc<Session>> = self
            .sessions()
            .values()
            .filter(|session| session.conversation_id == *conversation)
            .cloned()
            .collect();
        sessions.sort_by_key(|session| (session.created_at, session.id));
        sessions
            .iter()
            .map(|session| session.dto(&lock(&session.state)))
            .collect()
    }

    fn start_dir(&self, conversation: &ConversationId) -> Result<PathBuf, AppError> {
        let working = self
            .inner
            .options
            .folders
            .working_folder_of(&conversation.to_string())?;
        let home = &self.inner.options.home;
        Ok(working
            .filter(|folder| folder.is_dir())
            .or_else(|| home.is_dir().then(|| home.clone()))
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default())
    }

    fn spec(
        &self,
        shell: TerminalShell,
        cwd: &Path,
        cols: u16,
        rows: u16,
    ) -> Result<PtySpec, AppError> {
        let command = self
            .inner
            .options
            .shells
            .locate(shell)
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, NO_SHELL))?;
        let mut env: Vec<(String, String)> = TERMINAL_ENV
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        if let Some(launcher) = &self.inner.options.launcher {
            env.push(("PATH".to_owned(), joined_path(&launcher.program_dirs())));
        }
        Ok(PtySpec {
            program: command.program,
            args: command.args,
            cwd: cwd.to_path_buf(),
            env,
            cols,
            rows,
        })
    }

    fn launch(&self, session: &Arc<Session>, state: &mut SessionState) -> Result<(), AppError> {
        let spec = self.spec(session.shell, &session.cwd, state.cols, state.rows)?;
        let parts = self.inner.options.spawner.spawn(&spec).map_err(|error| {
            AppError::new(AppErrorCode::Unknown, SHELL_FAILED)
                .with_detail("program", path_text(&spec.program))
                .with_detail("reason", error.to_string())
        })?;
        let generation = state.generation;
        let control: Arc<dyn PtyControl> = Arc::from(parts.control);
        *lock(&session.writer) = Some(parts.writer);
        state.control = Some(Arc::clone(&control));
        state.status = TerminalStatus::Running;
        state.exit_code = None;
        let reader = {
            let session = Arc::clone(session);
            let reader = parts.reader;
            std::thread::spawn(move || read_loop(session, generation, reader))
        };
        let session = Arc::clone(session);
        let child = parts.child;
        std::thread::spawn(move || wait_loop(session, generation, child, control, reader));
        Ok(())
    }

    fn default_shell(&self) -> Result<TerminalShell, AppError> {
        self.shells()
            .into_iter()
            .next()
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, NO_SHELL))
    }

    fn title_for(&self, conversation: &ConversationId, shell: TerminalShell) -> String {
        let base = shell_title(shell);
        let taken: Vec<String> = self
            .list(conversation)
            .into_iter()
            .map(|session| session.title)
            .collect();
        if !taken.iter().any(|title| title == base) {
            return base.to_owned();
        }
        (2..)
            .map(|index| format!("{base} {index}"))
            .find(|title| !taken.contains(title))
            .unwrap_or_else(|| base.to_owned())
    }

    pub fn create(&self, input: &TerminalCreateInput) -> Result<TerminalSessionDto, AppError> {
        let conversation = input.conversation_id;
        if self.list(&conversation).len() >= TERMINAL_SESSIONS_MAX {
            return Err(AppError::new(AppErrorCode::Conflict, SESSIONS_LIMIT));
        }
        let shell = match input.shell {
            Some(shell) => shell,
            None => self.default_shell()?,
        };
        let cwd = self.start_dir(&conversation)?;
        let stream = self.inner.options.events.open_stream()?;
        let session = Arc::new(Session {
            id: TerminalId::new(new_id()),
            conversation_id: conversation,
            shell,
            cwd,
            created_at: self.inner.options.clock.now(),
            stream,
            state: Mutex::new(SessionState {
                title: self.title_for(&conversation, shell),
                status: TerminalStatus::Running,
                exit_code: None,
                cols: input.cols,
                rows: input.rows,
                scrollback: String::new(),
                next_seq: 0,
                generation: 0,
                control: None,
                closed: false,
            }),
            writer: Mutex::new(None),
        });
        let dto = {
            let mut state = lock(&session.state);
            if let Err(error) = self.launch(&session, &mut state) {
                state.closed = true;
                drop(state);
                session.stream.end(RunOutcomeDto {
                    status: RunOutcomeStatus::Failed,
                    code: Some(error.code),
                    message: Some(error.message.clone()),
                });
                return Err(error);
            }
            session.dto(&state)
        };
        self.sessions().insert(session.id, session);
        Ok(dto)
    }

    pub fn attach(&self, id: &TerminalId) -> Result<TerminalAttachDto, AppError> {
        let session = self.session(id)?;
        let state = lock(&session.state);
        Ok(TerminalAttachDto {
            session: session.dto(&state),
            scrollback: state.scrollback.clone(),
            next_seq: state.next_seq,
        })
    }

    pub fn write(&self, id: &TerminalId, data: &str) -> Result<(), AppError> {
        let session = self.session(id)?;
        let mut writer = lock(&session.writer);
        let Some(writer) = writer.as_mut() else {
            return Err(AppError::new(AppErrorCode::Conflict, SESSION_EXITED));
        };
        writer
            .write_all(data.as_bytes())
            .and_then(|()| writer.flush())
            .map_err(|error| {
                AppError::new(AppErrorCode::Conflict, SESSION_EXITED)
                    .with_detail("reason", error.to_string())
            })
    }

    pub fn resize(&self, id: &TerminalId, cols: u16, rows: u16) -> Result<(), AppError> {
        let session = self.session(id)?;
        let mut state = lock(&session.state);
        state.cols = cols;
        state.rows = rows;
        if let Some(control) = &state.control {
            let _ = control.resize(cols, rows);
        }
        Ok(())
    }

    pub fn rename(&self, id: &TerminalId, title: &str) -> Result<TerminalSessionDto, AppError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(invalid("title", "nonEmpty"));
        }
        let session = self.session(id)?;
        let mut state = lock(&session.state);
        title.clone_into(&mut state.title);
        Ok(session.dto(&state))
    }

    pub fn clear(&self, id: &TerminalId) -> Result<(), AppError> {
        let session = self.session(id)?;
        lock(&session.state).scrollback.clear();
        Ok(())
    }

    pub fn restart(&self, id: &TerminalId) -> Result<TerminalSessionDto, AppError> {
        let session = self.session(id)?;
        let mut state = lock(&session.state);
        if let Some(control) = state.control.take() {
            control.kill();
        }
        lock(&session.writer).take();
        state.generation += 1;
        state.scrollback.clear();
        if let Err(error) = self.launch(&session, &mut state) {
            state.status = TerminalStatus::Exited;
            state.exit_code = None;
            session.emit_status(&mut state);
            return Err(error);
        }
        session.emit_status(&mut state);
        Ok(session.dto(&state))
    }

    fn shut(session: &Session) {
        let mut state = lock(&session.state);
        if state.closed {
            return;
        }
        state.generation += 1;
        if let Some(control) = state.control.take() {
            control.kill();
        }
        lock(&session.writer).take();
        state.closed = true;
        session.stream.end(RunOutcomeDto {
            status: RunOutcomeStatus::Ok,
            code: None,
            message: None,
        });
    }

    pub fn close(&self, id: &TerminalId) -> Result<(), AppError> {
        let session = self.sessions().remove(id).ok_or_else(missing)?;
        Self::shut(&session);
        Ok(())
    }

    pub fn close_conversation(&self, conversation: &ConversationId) {
        let removed: Vec<Arc<Session>> = {
            let mut sessions = self.sessions();
            let ids: Vec<TerminalId> = sessions
                .values()
                .filter(|session| session.conversation_id == *conversation)
                .map(|session| session.id)
                .collect();
            ids.iter().filter_map(|id| sessions.remove(id)).collect()
        };
        for session in removed {
            Self::shut(&session);
        }
    }

    pub fn dispose(&self) {
        let removed: Vec<Arc<Session>> = std::mem::take(&mut *self.sessions())
            .into_values()
            .collect();
        for session in removed {
            Self::shut(&session);
        }
    }

    pub fn open(&self) -> usize {
        self.sessions().len()
    }
}
