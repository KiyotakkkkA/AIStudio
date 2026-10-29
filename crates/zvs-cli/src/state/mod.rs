pub mod input;
pub mod palette;
pub mod prompt;
pub mod transcript;

use std::collections::VecDeque;

use zvs_contract::{
    AllowRuleChangeDto, AllowRuleInput, AppError, BindFolderResultDto, ChatAnswerInput,
    ChatSendInput, ChatSkipInput, CliSessionDto, ConversationDto, ConversationId, ExecMode,
    ExecPolicyDto, FolderAccessAnswer, FolderGrantScope, HostEvent, HostEventDraft, PolicyRuleDto,
    ProviderDto, ProviderId, RunHandleDto, RunId, RunOutcomeStatus, WorkingFolderAccess,
};

use self::input::Input;
use self::palette::{Command, Palette, is_command_text};
use self::prompt::{ApprovalDetail, Choice, Prompt, PromptKind};
use self::transcript::{Tone, Transcript};
use crate::copy;

pub const SCROLL_STEP: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Newline,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Tab,
    Esc,
    PageUp,
    PageDown,
    Interrupt,
    Details,
    ClearLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Connecting(&'static str),
    Access,
    Chat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
    Quiet,
    Message(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelChoice {
    pub provider: ProviderId,
    pub provider_name: String,
    pub model: String,
    pub model_name: String,
    pub context_window: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Open,
    Providers,
    Policy,
    Create {
        provider: ProviderId,
        model: String,
    },
    Bind {
        id: ConversationId,
        grant: Option<FolderGrantScope>,
    },
    Send(Box<ChatSendInput>),
    Cancel(RunId),
    Approve {
        run: RunId,
        approval: String,
    },
    Deny {
        run: RunId,
        approval: String,
    },
    AllowRule(AllowRuleInput),
    AnswerFolder {
        run: RunId,
        approval: String,
        answer: FolderAccessAnswer,
    },
    Answer(ChatAnswerInput),
    Skip(ChatSkipInput),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Key(Key),
    Paste(String),
    Scroll(isize),
    Tick,
    Connecting(&'static str),
    Connected,
    Unreachable(&'static str),
    Event(Box<HostEvent>),
    Closed,
    Opened(Result<CliSessionDto, AppError>),
    Providers(Result<Vec<ProviderDto>, AppError>),
    Policy(Result<ExecPolicyDto, AppError>),
    Created(Result<ConversationDto, AppError>),
    Bound(Result<BindFolderResultDto, AppError>),
    Sent(Result<RunHandleDto, AppError>),
    Answered(Result<(), AppError>),
    RuleAdded(Result<PolicyRuleDto, AppError>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct App {
    pub phase: Phase,
    pub folder: String,
    pub grant: Option<FolderGrantScope>,
    pub conversation: Option<ConversationDto>,
    pub providers: Vec<ProviderDto>,
    pub model: Option<ModelChoice>,
    pub mode: ExecMode,
    pub transcript: Transcript,
    pub input: Input,
    pub palette: Palette,
    pub prompts: VecDeque<Prompt>,
    pub run: Option<RunHandleDto>,
    pub awaiting: bool,
    pub held: Vec<HostEvent>,
    pub cancelling: bool,
    pub pending: Option<String>,
    pub expanded: bool,
    pub scroll: usize,
    pub escape_armed: bool,
    pub help: bool,
    pub status: Option<(Tone, String)>,
    pub frame: u64,
    pub started: u64,
    pub exit: Option<Exit>,
}

pub fn default_model(providers: &[ProviderDto]) -> Option<ModelChoice> {
    providers.iter().find_map(|provider| {
        let models = &provider.models;
        let model = models
            .iter()
            .find(|model| {
                model.available && Some(&model.id) == provider.summary.default_model_id.as_ref()
            })
            .or_else(|| models.iter().find(|model| model.available))?;
        Some(ModelChoice {
            provider: provider.summary.id,
            provider_name: provider.summary.name.clone(),
            model: model.external_id.clone(),
            model_name: model.display_name.clone(),
            context_window: model.context_window,
        })
    })
}

impl App {
    pub fn new(folder: &str) -> Self {
        Self {
            phase: Phase::Connecting(copy::CONNECTING),
            folder: folder.to_owned(),
            grant: None,
            conversation: None,
            providers: Vec::new(),
            model: None,
            mode: ExecMode::AlwaysAsk,
            transcript: Transcript::default(),
            input: Input::default(),
            palette: Palette::default(),
            prompts: VecDeque::new(),
            run: None,
            awaiting: false,
            held: Vec::new(),
            cancelling: false,
            pending: None,
            expanded: false,
            scroll: 0,
            escape_armed: false,
            help: false,
            status: None,
            frame: 0,
            started: 0,
            exit: None,
        }
    }

    pub const fn busy(&self) -> bool {
        self.run.is_some() || self.awaiting
    }

    pub fn palette_open(&self) -> bool {
        self.prompts.is_empty() && !self.help && is_command_text(self.input.text())
    }

    pub fn show_welcome(&self) -> bool {
        self.transcript.entries.is_empty() && self.transcript.live.is_none()
    }

    fn fail(&mut self, error: &AppError) {
        self.transcript.notice(Tone::Error, error.message.clone());
    }

    fn quit(&mut self, message: Option<&str>) {
        self.exit = Some(message.map_or(Exit::Quiet, |text| Exit::Message(text.to_owned())));
    }

    pub fn update(&mut self, message: Msg) -> Vec<Request> {
        match message {
            Msg::Key(key) => self.key(key),
            Msg::Paste(text) => {
                if let Some(other) = self.question_input() {
                    other.insert(&text);
                } else if self.prompts.is_empty() {
                    self.input.insert(&text);
                }
                Vec::new()
            }
            Msg::Scroll(delta) => {
                self.scroll = self.scroll.saturating_add_signed(delta);
                Vec::new()
            }
            Msg::Tick => {
                self.frame = self.frame.wrapping_add(1);
                Vec::new()
            }
            Msg::Connecting(message) => {
                if matches!(self.phase, Phase::Connecting(_)) {
                    self.phase = Phase::Connecting(message);
                }
                Vec::new()
            }
            Msg::Connected => vec![Request::Open, Request::Providers, Request::Policy],
            Msg::Unreachable(message) => {
                self.quit(Some(message));
                Vec::new()
            }
            Msg::Closed => {
                self.quit(Some(copy::DISCONNECTED));
                Vec::new()
            }
            Msg::Opened(result) => {
                match result {
                    Ok(session) => {
                        self.folder = session.folder;
                        if session.access == WorkingFolderAccess::Granted {
                            self.phase = Phase::Chat;
                        } else {
                            self.phase = Phase::Access;
                            self.prompts.push_back(Prompt::access(&self.folder));
                        }
                    }
                    Err(error) => self.quit(Some(&error.message)),
                }
                Vec::new()
            }
            Msg::Providers(result) => {
                match result {
                    Ok(providers) => {
                        self.model = default_model(&providers);
                        self.providers = providers;
                    }
                    Err(error) => self.fail(&error),
                }
                Vec::new()
            }
            Msg::Policy(result) => {
                if let Ok(policy) = result {
                    self.mode = policy.mode;
                }
                Vec::new()
            }
            Msg::Created(result) => match result {
                Ok(conversation) => {
                    let id = conversation.id;
                    self.conversation = Some(conversation);
                    vec![Request::Bind {
                        id,
                        grant: self.grant,
                    }]
                }
                Err(error) => self.abandon(&error),
            },
            Msg::Bound(result) => match result {
                Ok(BindFolderResultDto::Bound { conversation }) => {
                    let id = conversation.id;
                    self.conversation = Some(*conversation);
                    self.grant = None;
                    self.pending
                        .take()
                        .map(|text| self.send(id, text))
                        .into_iter()
                        .collect()
                }
                Ok(BindFolderResultDto::NeedsGrant { .. }) => {
                    self.pending = None;
                    self.quit(Some(copy::ACCESS_DENIED));
                    Vec::new()
                }
                Err(error) => self.abandon(&error),
            },
            Msg::Sent(result) => {
                self.awaiting = false;
                match result {
                    Ok(handle) => {
                        let stream = handle.stream_id;
                        self.run = Some(handle);
                        let held = std::mem::take(&mut self.held);
                        held.into_iter()
                            .filter(|event| event.stream_id == stream)
                            .flat_map(|event| self.event(event))
                            .collect()
                    }
                    Err(error) => {
                        self.transcript.live = None;
                        self.held.clear();
                        self.fail(&error);
                        Vec::new()
                    }
                }
            }
            Msg::Event(event) => {
                if self.awaiting && self.run.is_none() {
                    self.held.push(*event);
                    return Vec::new();
                }
                self.event(*event)
            }
            Msg::Answered(result) => {
                if let Err(error) = result {
                    self.fail(&error);
                }
                Vec::new()
            }
            Msg::RuleAdded(result) => {
                match result {
                    Ok(_) => self.transcript.notice(Tone::Success, copy::RULE_ADDED),
                    Err(error) => self.fail(&error),
                }
                Vec::new()
            }
        }
    }

    fn abandon(&mut self, error: &AppError) -> Vec<Request> {
        self.awaiting = false;
        self.pending = None;
        self.transcript.live = None;
        self.fail(error);
        Vec::new()
    }

    fn event(&mut self, event: HostEvent) -> Vec<Request> {
        let Some(run) = self.run.as_ref() else {
            return Vec::new();
        };
        if event.stream_id != run.stream_id {
            return Vec::new();
        }
        let run_id = run.id;
        match &event.body {
            HostEventDraft::Approval { request } => {
                if let Some(prompt) = prompt::from_request(request, self.mode) {
                    self.prompts.push_back(prompt);
                }
            }
            HostEventDraft::Question { request } if request.run_id == run_id => {
                self.prompts.push_back(prompt::question(request.clone()));
            }
            HostEventDraft::End { outcome } => {
                self.transcript.finish();
                match outcome.status {
                    RunOutcomeStatus::Ok => {}
                    RunOutcomeStatus::Cancelled => {
                        self.transcript.notice(Tone::Info, copy::CANCELLED);
                    }
                    RunOutcomeStatus::Failed => self.transcript.notice(
                        Tone::Error,
                        outcome
                            .message
                            .clone()
                            .unwrap_or_else(|| copy::FAILED.to_owned()),
                    ),
                }
                self.run = None;
                self.cancelling = false;
                self.prompts
                    .retain(|prompt| matches!(prompt.kind, PromptKind::Access { .. }));
            }
            body => self.transcript.absorb(body),
        }
        Vec::new()
    }

    fn send(&mut self, id: ConversationId, text: String) -> Request {
        self.awaiting = true;
        self.transcript.start();
        let mut input = ChatSendInput::new(id, text);
        if let Some(model) = &self.model {
            input.provider_id = Some(model.provider);
            input.model_id = Some(model.model.clone());
        }
        Request::Send(Box::new(input))
    }

    fn submit_message(&mut self, text: String) -> Vec<Request> {
        if self.busy() {
            self.input.set(&text);
            self.status = Some((Tone::Info, copy::BUSY.to_owned()));
            return Vec::new();
        }
        let Some(model) = self.model.clone() else {
            self.input.set(&text);
            self.transcript.notice(Tone::Error, copy::NO_PROVIDER);
            return Vec::new();
        };
        self.transcript.push_user(&text);
        self.scroll = 0;
        self.started = self.frame;
        if let Some(id) = self
            .conversation
            .as_ref()
            .map(|conversation| conversation.id)
        {
            return vec![self.send(id, text)];
        }
        self.awaiting = true;
        self.pending = Some(text);
        vec![Request::Create {
            provider: model.provider,
            model: model.model,
        }]
    }

    fn command(&mut self, command: Command) -> Vec<Request> {
        self.palette.reset();
        match command {
            Command::Help => self.help = true,
            Command::Exit => self.quit(None),
            Command::Clear => {
                self.transcript.clear();
                self.scroll = 0;
                self.status = Some((Tone::Info, copy::CHAT_CLEARED.to_owned()));
            }
            Command::New => {
                if self.busy() {
                    self.status = Some((Tone::Info, copy::BUSY.to_owned()));
                } else {
                    self.conversation = None;
                    self.transcript = Transcript::default();
                    self.scroll = 0;
                    self.status = Some((Tone::Success, copy::NEW_CHAT.to_owned()));
                }
            }
            Command::Resume
            | Command::Models
            | Command::Context
            | Command::Skills
            | Command::Mcp
            | Command::Permissions => {
                self.status = Some((Tone::Info, copy::NOT_YET.to_owned()));
            }
        }
        Vec::new()
    }

    fn submit(&mut self) -> Vec<Request> {
        if self.palette_open()
            && let Some(command) = self.palette.current(self.input.text())
        {
            self.input.take();
            return self.command(command);
        }
        let text = self.input.take();
        if text.is_empty() {
            return Vec::new();
        }
        if text.starts_with('/') && !text.starts_with("//") {
            return match Command::parse(&text) {
                Some(command) => self.command(command),
                None => {
                    self.status = Some((Tone::Error, format!("{}: {text}", copy::UNKNOWN_COMMAND)));
                    Vec::new()
                }
            };
        }
        self.submit_message(text)
    }

    fn question_input(&mut self) -> Option<&mut Input> {
        match self.prompts.front_mut().map(|prompt| &mut prompt.kind) {
            Some(PromptKind::Question(state)) => state.other.as_mut(),
            _ => None,
        }
    }

    fn key(&mut self, key: Key) -> Vec<Request> {
        if key != Key::Esc {
            self.escape_armed = false;
        }
        self.status = None;
        match key {
            Key::Interrupt => {
                self.quit(None);
                return Vec::new();
            }
            Key::Details => {
                self.expanded = !self.expanded;
                return Vec::new();
            }
            Key::PageUp => {
                self.scroll = self.scroll.saturating_add(SCROLL_STEP);
                return Vec::new();
            }
            Key::PageDown => {
                self.scroll = self.scroll.saturating_sub(SCROLL_STEP);
                return Vec::new();
            }
            _ => {}
        }
        if matches!(self.phase, Phase::Connecting(_)) {
            if key == Key::Esc {
                self.quit(None);
            }
            return Vec::new();
        }
        if !self.prompts.is_empty() {
            return self.prompt_key(key);
        }
        if self.help {
            if matches!(key, Key::Esc | Key::Enter) {
                self.help = false;
            }
            return Vec::new();
        }
        if self.palette_open() {
            let text = self.input.text().to_owned();
            match key {
                Key::Up => {
                    self.palette.up(&text);
                    return Vec::new();
                }
                Key::Down => {
                    self.palette.down(&text);
                    return Vec::new();
                }
                Key::Tab => {
                    if let Some(command) = self.palette.current(&text) {
                        self.input.set(&format!("/{}", command.name()));
                    }
                    return Vec::new();
                }
                Key::Esc => {
                    self.input.clear();
                    self.palette.reset();
                    return Vec::new();
                }
                _ => {}
            }
        }
        match key {
            Key::Enter => return self.submit(),
            Key::Esc => return self.escape(),
            Key::Newline => self.input.newline(),
            Key::Char(value) => {
                self.input.insert(&value.to_string());
                self.palette.reset();
            }
            Key::Backspace => {
                self.input.backspace();
                self.palette.reset();
            }
            Key::Delete => self.input.delete(),
            Key::Left => self.input.left(),
            Key::Right => self.input.right(),
            Key::Home => self.input.home(),
            Key::End => self.input.end(),
            Key::ClearLine => self.input.clear(),
            Key::Up => {
                if !self.input.up() {
                    self.input.history_back();
                }
            }
            Key::Down => {
                if !self.input.down() {
                    self.input.history_forward();
                }
            }
            Key::Tab | Key::PageUp | Key::PageDown | Key::Interrupt | Key::Details => {}
        }
        Vec::new()
    }

    fn escape(&mut self) -> Vec<Request> {
        if let Some(run) = &self.run {
            if self.cancelling {
                return Vec::new();
            }
            self.cancelling = true;
            return vec![Request::Cancel(run.id)];
        }
        if !self.input.is_empty() {
            self.input.clear();
            return Vec::new();
        }
        if self.escape_armed {
            self.quit(None);
        } else {
            self.escape_armed = true;
            self.status = Some((Tone::Info, copy::EXIT_AGAIN.to_owned()));
        }
        Vec::new()
    }

    fn prompt_key(&mut self, key: Key) -> Vec<Request> {
        let Some(prompt) = self.prompts.front_mut() else {
            return Vec::new();
        };
        if let PromptKind::Question(state) = &mut prompt.kind {
            let run = self.run.as_ref().map(|run| run.id);
            let request_id = state.request.id.clone();
            if key == Key::Esc {
                self.prompts.pop_front();
                self.transcript.notice(Tone::Info, copy::QUESTION_SKIPPED);
                return run
                    .map(|id| Request::Skip(ChatSkipInput { id, request_id }))
                    .into_iter()
                    .collect();
            }
            if let Some(other) = state.other.as_mut() {
                match key {
                    Key::Char(value) => other.insert(&value.to_string()),
                    Key::Backspace => other.backspace(),
                    Key::Left => other.left(),
                    Key::Right => other.right(),
                    Key::Newline => other.newline(),
                    _ => {}
                }
                if key != Key::Enter {
                    return Vec::new();
                }
            }
            match key {
                Key::Up => prompt.up(),
                Key::Down | Key::Tab => prompt.down(),
                Key::Char(' ') => state.toggle(prompt.selected),
                Key::Enter => {
                    if state.commit(prompt.selected) {
                        let answers = state.answers.clone();
                        self.prompts.pop_front();
                        return run
                            .map(|id| {
                                Request::Answer(ChatAnswerInput {
                                    id,
                                    request_id,
                                    answers,
                                })
                            })
                            .into_iter()
                            .collect();
                    }
                    if state.other.is_none() {
                        prompt.selected = 0;
                    }
                }
                _ => {}
            }
            return Vec::new();
        }
        match key {
            Key::Up => prompt.up(),
            Key::Down | Key::Tab => prompt.down(),
            Key::Enter => {
                let choice = prompt.choice();
                return choice.map(|choice| self.choose(choice)).unwrap_or_default();
            }
            Key::Esc => {
                let last = prompt.choices().last().copied();
                return last.map(|choice| self.choose(choice)).unwrap_or_default();
            }
            _ => {}
        }
        Vec::new()
    }

    fn choose(&mut self, choice: Choice) -> Vec<Request> {
        let Some(prompt) = self.prompts.pop_front() else {
            return Vec::new();
        };
        match prompt.kind {
            PromptKind::Access { .. } => {
                match choice {
                    Choice::AccessConversation => self.grant = Some(FolderGrantScope::Conversation),
                    Choice::AccessAlways => self.grant = Some(FolderGrantScope::Always),
                    _ => self.quit(Some(copy::ACCESS_DENIED)),
                }
                self.phase = Phase::Chat;
                Vec::new()
            }
            PromptKind::Approval {
                run, id, detail, ..
            } => match choice {
                Choice::Approve => vec![Request::Approve { run, approval: id }],
                Choice::AlwaysHere | Choice::AlwaysEverywhere => {
                    let folder = (choice == Choice::AlwaysHere).then(|| self.folder.clone());
                    let mut requests = Vec::new();
                    if let Some(rule) = allow_rule(&detail, folder) {
                        requests.push(Request::AllowRule(rule));
                    }
                    requests.push(Request::Approve { run, approval: id });
                    requests
                }
                _ => vec![Request::Deny { run, approval: id }],
            },
            PromptKind::Folder { run, id, .. } => {
                let answer = match choice {
                    Choice::FolderOnce => FolderAccessAnswer::Once,
                    Choice::FolderConversation => FolderAccessAnswer::Conversation,
                    Choice::FolderAlways => FolderAccessAnswer::Always,
                    _ => FolderAccessAnswer::Deny,
                };
                vec![Request::AnswerFolder {
                    run,
                    approval: id,
                    answer,
                }]
            }
            PromptKind::Question(_) => Vec::new(),
        }
    }
}

fn allow_rule(detail: &ApprovalDetail, folder: Option<String>) -> Option<AllowRuleInput> {
    match detail {
        ApprovalDetail::Plain => None,
        ApprovalDetail::Process { argv, cwd, .. } => Some(AllowRuleInput::Process {
            argv: argv.clone(),
            cwd: cwd.clone(),
            folder,
        }),
        ApprovalDetail::Files { changes } => Some(AllowRuleInput::Files {
            changes: changes
                .iter()
                .map(|change| AllowRuleChangeDto {
                    path: change.path.clone(),
                    kind: change.kind,
                })
                .collect(),
            folder,
        }),
    }
}

#[cfg(test)]
mod tests;
