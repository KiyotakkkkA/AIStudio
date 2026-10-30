pub mod input;
pub mod overlay;
pub mod palette;
pub mod prompt;
pub mod shortcuts;
pub mod transcript;

use std::collections::VecDeque;

use zvs_contract::{
    AllowRuleChangeDto, AllowRuleInput, AppError, BindFolderResultDto, ChatAnswerInput,
    ChatAttachDto, ChatContextDto, ChatSendInput, ChatSkipInput, CliSessionDto,
    ConversationDetailDto, ConversationDto, ConversationId, ExecMode, ExecPolicyDto,
    FolderAccessAnswer, FolderGrantScope, HostEvent, HostEventDraft, McpServerDto, McpServerId,
    McpServerStatus, PolicyRuleDto, ProviderDto, ProviderId, RunHandleDto, RunId, RunOutcomeStatus,
    SkillDto, SkillId, WorkingFolderAccess,
};

use self::input::Input;
use self::overlay::{Action, Overlay, Panel, View};
use self::palette::{Command, Palette, is_command_text};
use self::prompt::{ApprovalDetail, Choice, Prompt, PromptKind};
use self::shortcuts::{Chord, Keymap, SHORTCUTS, Shortcut};
use self::transcript::{ShellEnd, ShellRun, Tone, Transcript};
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
    Chord(Chord),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pointer {
    Press(u16, u16),
    Drag(u16, u16),
    Release(u16, u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: (u16, u16),
    pub cursor: (u16, u16),
}

impl Selection {
    pub fn ordered(&self) -> ((u16, u16), (u16, u16)) {
        let key = |(x, y): (u16, u16)| (y, x);
        if key(self.anchor) <= key(self.cursor) {
            (self.anchor, self.cursor)
        } else {
            (self.cursor, self.anchor)
        }
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        let ((start_x, start_y), (end_x, end_y)) = self.ordered();
        (y > start_y || (y == start_y && x >= start_x)) && (y < end_y || (y == end_y && x <= end_x))
    }
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
    Conversations,
    Load(ConversationId),
    Attach(ConversationId),
    Context {
        conversation: Option<ConversationId>,
        provider: ProviderId,
        model: String,
    },
    Skills,
    Servers,
    SetSkill {
        id: SkillId,
        enabled: bool,
    },
    StartServer(McpServerId),
    StopServer(McpServerId),
    SetPolicy(ExecPolicyDto),
    FocusVerification,
    Shell(String),
    StopShell,
    Copy(Selection),
    SaveShortcuts(serde_json::Value),
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
    Conversations(Result<Vec<ConversationDto>, AppError>),
    Loaded(Result<ConversationDetailDto, AppError>),
    Attached(Result<Option<ChatAttachDto>, AppError>),
    Context(Result<ChatContextDto, AppError>),
    Skills(Result<Vec<SkillDto>, AppError>),
    Servers(Result<Vec<McpServerDto>, AppError>),
    SkillChanged(Result<SkillDto, AppError>),
    ServerChanged(Result<McpServerDto, AppError>),
    PolicySet(Result<ExecPolicyDto, AppError>),
    Notice(zvs_contract::CliNotice),
    ShellOutput(String),
    ShellEnded(ShellEnd),
    Pointer(Pointer),
    Copied(bool),
    ShortcutsSaved(bool),
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
    pub policy: Option<ExecPolicyDto>,
    pub overlay: Option<Overlay>,
    pub verification: Option<String>,
    pub shell: Option<ShellRun>,
    pub selection: Option<Selection>,
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
    pub keymap: Keymap,
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
            policy: None,
            overlay: None,
            verification: None,
            shell: None,
            selection: None,
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
            keymap: Keymap::default(),
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
        self.prompts.is_empty()
            && !self.help
            && self.overlay.is_none()
            && is_command_text(self.input.text())
    }

    pub fn capturing(&self) -> bool {
        self.overlay
            .as_ref()
            .is_some_and(|overlay| overlay.capturing.is_some())
    }

    pub fn key_sections(&self) -> Vec<(&'static str, Vec<(String, String)>)> {
        vec![
            (
                copy::SHORTCUTS_TITLE,
                SHORTCUTS
                    .iter()
                    .map(|shortcut| {
                        (
                            self.keymap.label(*shortcut),
                            shortcut.description().to_owned(),
                        )
                    })
                    .collect(),
            ),
            (
                copy::KEYS_TITLE,
                copy::KEYS
                    .iter()
                    .map(|(keys, meaning)| ((*keys).to_owned(), (*meaning).to_owned()))
                    .collect(),
            ),
        ]
    }

    pub fn show_welcome(&self) -> bool {
        self.transcript.entries.is_empty() && self.transcript.live.is_none() && self.shell.is_none()
    }

    pub fn shell_mode(&self) -> bool {
        self.prompts.is_empty() && self.overlay.is_none() && self.input.text().starts_with('!')
    }

    fn submit_shell(&mut self, text: &str) -> Vec<Request> {
        let command = text.trim_start_matches('!').trim();
        if command.is_empty() {
            return Vec::new();
        }
        if self.shell.is_some() {
            self.input.set(text);
            self.status = Some((Tone::Info, copy::SHELL_BUSY.to_owned()));
            return Vec::new();
        }
        self.shell = Some(ShellRun::new(command));
        self.started = self.frame;
        self.scroll = 0;
        vec![Request::Shell(command.to_owned())]
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
            Msg::Pointer(pointer) => self.pointer(pointer),
            Msg::ShortcutsSaved(saved) => {
                if !saved {
                    self.status = Some((Tone::Error, copy::SHORTCUTS_SAVE_FAILED.to_owned()));
                }
                Vec::new()
            }
            Msg::Copied(copied) => {
                self.status = Some(if copied {
                    (Tone::Success, copy::COPIED.to_owned())
                } else {
                    (Tone::Error, copy::COPY_FAILED.to_owned())
                });
                Vec::new()
            }
            Msg::ShellOutput(line) => {
                if let Some(shell) = self.shell.as_mut() {
                    shell.push(line);
                }
                Vec::new()
            }
            Msg::ShellEnded(end) => {
                if let Some(mut shell) = self.shell.take() {
                    shell.end = Some(end);
                    self.transcript
                        .entries
                        .push(transcript::Entry::Shell(Box::new(shell)));
                }
                Vec::new()
            }
            Msg::Notice(zvs_contract::CliNotice::AccountVerification { origin, shown }) => {
                if shown {
                    self.verification = Some(origin);
                    return vec![Request::FocusVerification];
                }
                if self.verification.as_deref() == Some(origin.as_str()) {
                    self.verification = None;
                }
                Vec::new()
            }
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
                match result {
                    Ok(policy) => self.adopt_policy(policy),
                    Err(error) => self.panel_failed(&error),
                }
                Vec::new()
            }
            Msg::PolicySet(result) => {
                match result {
                    Ok(policy) => {
                        let judge_changed = self
                            .policy
                            .as_ref()
                            .is_some_and(|known| known.judge != policy.judge);
                        self.adopt_policy(policy);
                        let text = if judge_changed {
                            copy::JUDGE_SET
                        } else {
                            copy::MODE_SET
                        };
                        self.status = Some((Tone::Success, text.to_owned()));
                    }
                    Err(error) => self.status = Some((Tone::Error, error.message)),
                }
                Vec::new()
            }
            Msg::Conversations(result) => {
                match result {
                    Ok(list) => {
                        if let Some(overlay) = self.overlay_for(Panel::Resume) {
                            overlay.conversations = list;
                            overlay.loading = false;
                        }
                    }
                    Err(error) => self.panel_failed(&error),
                }
                Vec::new()
            }
            Msg::Context(result) => {
                match result {
                    Ok(context) => {
                        if let Some(overlay) = self.overlay_for(Panel::Context) {
                            overlay.context = Some(context);
                            overlay.loading = false;
                        }
                    }
                    Err(error) => self.panel_failed(&error),
                }
                Vec::new()
            }
            Msg::Skills(result) => {
                match result {
                    Ok(list) => {
                        if let Some(overlay) = self.overlay_for(Panel::Skills) {
                            overlay.skills = list;
                            overlay.loading = false;
                        }
                    }
                    Err(error) => self.panel_failed(&error),
                }
                Vec::new()
            }
            Msg::Servers(result) => {
                match result {
                    Ok(list) => {
                        if let Some(overlay) = self.overlay_for(Panel::Mcp) {
                            overlay.servers = list;
                            overlay.loading = false;
                        }
                    }
                    Err(error) => self.panel_failed(&error),
                }
                Vec::new()
            }
            Msg::SkillChanged(result) => {
                match result {
                    Ok(skill) => {
                        let text = if skill.enabled {
                            copy::SKILL_ENABLED
                        } else {
                            copy::SKILL_DISABLED
                        };
                        self.status = Some((Tone::Success, text.to_owned()));
                        if let Some(overlay) = self.overlay_for(Panel::Skills)
                            && let Some(known) =
                                overlay.skills.iter_mut().find(|known| known.id == skill.id)
                        {
                            *known = skill;
                        }
                    }
                    Err(error) => self.status = Some((Tone::Error, error.message)),
                }
                Vec::new()
            }
            Msg::ServerChanged(result) => {
                match result {
                    Ok(server) => {
                        let running = matches!(
                            server.status,
                            McpServerStatus::Connected | McpServerStatus::Starting
                        );
                        let text = if running {
                            copy::MCP_STARTED
                        } else {
                            copy::MCP_HALTED
                        };
                        self.status = Some((Tone::Success, text.to_owned()));
                        if let Some(overlay) = self.overlay_for(Panel::Mcp)
                            && let Some(known) = overlay
                                .servers
                                .iter_mut()
                                .find(|known| known.id == server.id)
                        {
                            *known = server;
                        }
                    }
                    Err(error) => self.status = Some((Tone::Error, error.message)),
                }
                Vec::new()
            }
            Msg::Loaded(result) => match result {
                Ok(detail) => {
                    let id = detail.conversation.id;
                    self.transcript = Transcript::from_messages(
                        &detail
                            .messages
                            .into_iter()
                            .filter(|message| !message.partial)
                            .collect::<Vec<_>>(),
                    );
                    self.conversation = Some(detail.conversation);
                    self.scroll = 0;
                    self.overlay = None;
                    self.status = Some((Tone::Success, copy::RESUMED.to_owned()));
                    let mut requests = vec![Request::Attach(id)];
                    if self.grant.is_some() {
                        requests.push(Request::Bind {
                            id,
                            grant: self.grant,
                        });
                    }
                    requests
                }
                Err(error) => {
                    self.panel_failed(&error);
                    Vec::new()
                }
            },
            Msg::Attached(result) => {
                match result {
                    Ok(Some(live)) => self.adopt_live(live),
                    Ok(None) => {}
                    Err(error) => self.fail(&error),
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
            Command::Resume => return self.open_panel(Panel::Resume),
            Command::Models => return self.open_panel(Panel::Models),
            Command::Context => return self.open_panel(Panel::Context),
            Command::Skills => return self.open_panel(Panel::Skills),
            Command::Mcp => return self.open_panel(Panel::Mcp),
            Command::Permissions => return self.open_panel(Panel::Permissions),
            Command::Shortcuts => return self.open_panel(Panel::Shortcuts),
        }
        Vec::new()
    }

    pub fn view(&self) -> View<'_> {
        View {
            providers: &self.providers,
            model: self.model.as_ref(),
            folder: &self.folder,
            policy: self.policy.as_ref(),
            mode: self.mode,
            keymap: &self.keymap,
        }
    }

    fn overlay_for(&mut self, panel: Panel) -> Option<&mut Overlay> {
        self.overlay
            .as_mut()
            .filter(|overlay| overlay.panel == panel)
    }

    fn panel_failed(&mut self, error: &AppError) {
        if let Some(overlay) = self.overlay.as_mut() {
            overlay.loading = false;
        }
        self.status = Some((Tone::Error, error.message.clone()));
    }

    fn adopt_policy(&mut self, policy: ExecPolicyDto) {
        self.mode = policy.mode;
        self.policy = Some(policy);
        if let Some(mut overlay) = self.overlay.take() {
            if overlay.panel == Panel::Permissions && overlay.loading {
                overlay.loading = false;
                overlay.select_current(&self.view());
            }
            self.overlay = Some(overlay);
        }
    }

    fn adopt_live(&mut self, live: ChatAttachDto) {
        let run = live.run.id;
        self.transcript.live = Some(transcript::Live::attached(
            live.content,
            live.reasoning,
            live.tool_calls,
        ));
        for request in &live.approvals {
            if let Some(prompt) = prompt::from_request(request, self.mode) {
                self.prompts.push_back(prompt);
            }
        }
        for request in live
            .questions
            .into_iter()
            .filter(|request| request.run_id == run)
        {
            self.prompts.push_back(prompt::question(request));
        }
        self.started = self.frame;
        self.run = Some(live.run);
    }

    fn open_panel(&mut self, panel: Panel) -> Vec<Request> {
        if panel == Panel::Resume && self.busy() {
            self.status = Some((Tone::Info, copy::BUSY.to_owned()));
            return Vec::new();
        }
        let requests = match panel {
            Panel::Models | Panel::Shortcuts => Vec::new(),
            Panel::Resume => vec![Request::Conversations],
            Panel::Skills => vec![Request::Skills],
            Panel::Mcp => vec![Request::Servers],
            Panel::Permissions => vec![Request::Policy],
            Panel::Context => match &self.model {
                Some(model) => vec![Request::Context {
                    conversation: self
                        .conversation
                        .as_ref()
                        .map(|conversation| conversation.id),
                    provider: model.provider,
                    model: model.model.clone(),
                }],
                None => {
                    self.status = Some((Tone::Info, copy::CONTEXT_NEEDS_MODEL.to_owned()));
                    return Vec::new();
                }
            },
        };
        let mut overlay = Overlay::new(panel);
        overlay.select_current(&self.view());
        self.overlay = Some(overlay);
        requests
    }

    fn act(&mut self, action: Action) -> Vec<Request> {
        match action {
            Action::SelectModel(choice) => {
                self.status = Some((
                    Tone::Success,
                    format!(
                        "{}: {} · {}",
                        copy::MODEL_SELECTED,
                        choice.provider_name,
                        choice.model_name
                    ),
                ));
                self.model = Some(choice);
                self.overlay = None;
                Vec::new()
            }
            Action::Resume(id) => vec![Request::Load(id)],
            Action::ToggleSkill { id, enabled } => vec![Request::SetSkill { id, enabled }],
            Action::ToggleServer { id, start } => vec![if start {
                Request::StartServer(id)
            } else {
                Request::StopServer(id)
            }],
            Action::SetMode(mode) => {
                if mode == ExecMode::NoRestrictions
                    && self.mode != mode
                    && let Some(overlay) = self.overlay.as_mut()
                    && !overlay.armed
                {
                    overlay.armed = true;
                    self.status = Some((Tone::Error, copy::NO_RESTRICTIONS_CONFIRM.to_owned()));
                    return Vec::new();
                }
                let judge = self.policy.as_ref().and_then(|policy| policy.judge.clone());
                vec![Request::SetPolicy(ExecPolicyDto { mode, judge })]
            }
            Action::SetJudge(judge) => vec![Request::SetPolicy(ExecPolicyDto {
                mode: self.mode,
                judge,
            })],
            Action::Rebind(shortcut) => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.capturing = Some(shortcut);
                }
                Vec::new()
            }
            Action::ResetShortcuts => {
                self.keymap.reset();
                self.status = Some((Tone::Success, copy::SHORTCUTS_RESTORED.to_owned()));
                vec![self.save_shortcuts()]
            }
        }
    }

    fn save_shortcuts(&self) -> Request {
        Request::SaveShortcuts(self.keymap.to_json())
    }

    fn stop_capture(&mut self) {
        if let Some(overlay) = self.overlay.as_mut() {
            overlay.capturing = None;
        }
    }

    fn capture(&mut self, target: Shortcut, key: Key) -> Vec<Request> {
        match key {
            Key::Esc => {
                self.stop_capture();
                Vec::new()
            }
            Key::Backspace | Key::Delete => {
                self.keymap.unbind(target);
                self.stop_capture();
                self.status = Some((Tone::Success, copy::SHORTCUT_REMOVED.to_owned()));
                vec![self.save_shortcuts()]
            }
            Key::Chord(chord) if chord.reserved() => {
                self.status = Some((Tone::Error, copy::SHORTCUT_RESERVED.to_owned()));
                Vec::new()
            }
            Key::Chord(chord) => {
                let moved = self.keymap.bind(target, chord);
                self.stop_capture();
                let label = chord.label();
                let mut text =
                    format!("{}: {label} — {}", copy::SHORTCUT_SET, target.description());
                if let Some(other) = moved {
                    text.push_str(&format!(
                        ". {label} {} «{}»",
                        copy::SHORTCUT_MOVED,
                        other.description()
                    ));
                }
                self.status = Some((Tone::Success, text));
                vec![self.save_shortcuts()]
            }
            _ => {
                self.status = Some((Tone::Info, copy::SHORTCUT_INVALID.to_owned()));
                Vec::new()
            }
        }
    }

    fn shortcut(&mut self, chord: Chord) -> Vec<Request> {
        let Some(shortcut) = self.keymap.action(chord) else {
            return Vec::new();
        };
        match shortcut {
            Shortcut::Details => self.expanded = !self.expanded,
            Shortcut::ClearInput => {
                if let Some(overlay) = self.overlay.as_mut() {
                    overlay.filter.clear();
                    overlay.selected = 0;
                } else if let Some(other) = self.question_input() {
                    other.clear();
                } else if self.prompts.is_empty() {
                    self.input.clear();
                    self.palette.reset();
                }
            }
            other => {
                if self.phase != Phase::Chat || !self.prompts.is_empty() {
                    return Vec::new();
                }
                let Some(command) = other.command() else {
                    return Vec::new();
                };
                self.help = false;
                self.overlay = None;
                return self.command(command);
            }
        }
        Vec::new()
    }

    fn overlay_key(&mut self, key: Key) -> Vec<Request> {
        if let Some(target) = self.overlay.as_ref().and_then(|overlay| overlay.capturing) {
            return self.capture(target, key);
        }
        let Some(overlay) = self.overlay.as_ref() else {
            return Vec::new();
        };
        let view = self.view();
        let rows = overlay.rows(&view);
        let tabs = overlay.tabs(&view).len();
        let Some(overlay) = self.overlay.as_mut() else {
            return Vec::new();
        };
        overlay.clamp(rows.len());
        match key {
            Key::Esc => {
                if overlay.filter.is_empty() {
                    self.overlay = None;
                } else {
                    overlay.filter.clear();
                    overlay.selected = 0;
                }
            }
            Key::Up => overlay.up(rows.len()),
            Key::Down => overlay.down(rows.len()),
            Key::Tab | Key::Right => overlay.next_tab(tabs, true),
            Key::Left => overlay.next_tab(tabs, false),
            Key::Char(' ') => {
                if let Some(row) = rows.get(overlay.selected) {
                    overlay.toggle(&row.key);
                }
            }
            Key::Char(value) => {
                overlay.filter.insert(&value.to_string());
                overlay.selected = 0;
            }
            Key::Backspace => {
                overlay.filter.backspace();
                overlay.selected = 0;
            }
            Key::Enter => {
                if let Some(action) = rows
                    .get(overlay.selected)
                    .and_then(|row| row.action.clone())
                {
                    return self.act(action);
                }
            }
            _ => {}
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
        if text.trim() == "?" {
            let sections = self.key_sections();
            self.transcript
                .entries
                .push(transcript::Entry::Keys(sections));
            self.scroll = 0;
            return Vec::new();
        }
        if text.starts_with('!') {
            return self.submit_shell(&text);
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

    fn pointer(&mut self, pointer: Pointer) -> Vec<Request> {
        match pointer {
            Pointer::Press(x, y) => {
                self.selection = Some(Selection {
                    anchor: (x, y),
                    cursor: (x, y),
                });
                Vec::new()
            }
            Pointer::Drag(x, y) => {
                if let Some(selection) = self.selection.as_mut() {
                    selection.cursor = (x, y);
                }
                Vec::new()
            }
            Pointer::Release(x, y) => {
                let Some(mut selection) = self.selection else {
                    return Vec::new();
                };
                selection.cursor = (x, y);
                if selection.anchor == selection.cursor {
                    self.selection = None;
                    return Vec::new();
                }
                self.selection = Some(selection);
                vec![Request::Copy(selection)]
            }
        }
    }

    fn key(&mut self, key: Key) -> Vec<Request> {
        self.selection = None;
        if key != Key::Esc {
            self.escape_armed = false;
        }
        self.status = None;
        if let Key::Chord(chord) = key
            && !self.capturing()
        {
            return self.shortcut(chord);
        }
        match key {
            Key::Interrupt => {
                self.quit(None);
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
        if self.overlay.is_some() {
            return self.overlay_key(key);
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
        if self.verification.is_some() && key == Key::Enter && self.input.is_empty() {
            return vec![Request::FocusVerification];
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
            Key::Tab | Key::PageUp | Key::PageDown | Key::Interrupt | Key::Chord(_) => {}
        }
        Vec::new()
    }

    fn escape(&mut self) -> Vec<Request> {
        if self.shell_mode() {
            self.input.clear();
            return Vec::new();
        }
        if self.shell.is_some() {
            return vec![Request::StopShell];
        }
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
