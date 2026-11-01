pub mod input;
pub mod overlay;
pub mod palette;
pub mod prompt;
pub mod shortcuts;
pub mod transcript;

use std::collections::VecDeque;

use zvs_contract::{
    AccountFamily, AllowRuleChangeDto, AllowRuleInput, AppError, BackgroundShellDto,
    BackgroundShellId, BackgroundShellStatusDto, BindFolderResultDto, ChatAnswerInput,
    ChatAttachDto, ChatCommand, ChatContextDto, ChatGoalDto, ChatGoalStepDto, ChatSendInput,
    ChatSkipInput, ChatWakeStepDto, CliSessionDto, CliTheme, ConversationDetailDto,
    ConversationDto, ConversationId, CreateMcpServerInput, ExecJudgeDto, ExecMode, ExecPolicyDto,
    FolderAccessAnswer, FolderGrantScope, GOAL_STEP_DOMAIN, GoalBudgetDto, GoalReason, GoalStatus,
    GoalUsageDto, HostEvent, HostEventDraft, McpOAuthState, McpServerDetailDto, McpServerDto,
    McpServerId, McpServerStatus, PolicyRuleDto, ProviderDto, ProviderId, ProviderUsageDto,
    RunHandleDto, RunId, RunOutcomeStatus, SHELL_STEP_DOMAIN, ShellOutputDto, SkillDto, SkillId,
    ThinkingEffort, WAKE_STEP_DOMAIN, WorkingFolderAccess,
};

use self::input::Input;
use self::overlay::{Action, Overlay, Panel, ProcessOutput, View};
use self::palette::{Arg, Command, Item, Palette, Pick, Sources, Sub, arg_slot, is_command_text};
use self::prompt::{ApprovalDetail, Choice, Prompt, PromptKind};
use self::shortcuts::{Chord, Keymap, SHORTCUTS, Shortcut};
use self::transcript::{ShellEnd, ShellRun, Tone, Transcript};
use crate::copy;

pub const SCROLL_STEP: usize = 10;
pub const OUTPUT_POLL_TICKS: u64 = 10;

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
pub enum PolicyChange {
    Mode(ExecMode),
    Judge(Option<ExecJudgeDto>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoalState {
    pub status: GoalStatus,
    pub reason: Option<GoalReason>,
    pub usage: GoalUsageDto,
    pub budget: GoalBudgetDto,
    pub seen: u64,
}

impl GoalState {
    pub const fn of(goal: &ChatGoalDto, seen: u64) -> Self {
        Self {
            status: goal.status,
            reason: goal.reason,
            usage: goal.usage,
            budget: goal.budget,
            seen,
        }
    }

    pub const fn started(seen: u64) -> Self {
        Self {
            status: GoalStatus::Active,
            reason: None,
            usage: GoalUsageDto {
                rounds: 0,
                tokens: 0,
                elapsed_ms: 0,
            },
            budget: GoalBudgetDto {
                rounds: zvs_contract::GOAL_DEFAULT_ROUNDS,
                tokens: zvs_contract::GOAL_DEFAULT_TOKENS,
                minutes: zvs_contract::GOAL_DEFAULT_MINUTES,
            },
            seen,
        }
    }

    pub const fn active(&self) -> bool {
        matches!(self.status, GoalStatus::Active)
    }

    pub const fn shown(&self) -> bool {
        matches!(self.status, GoalStatus::Active | GoalStatus::Paused)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    LinkAccount(AccountFamily),
    CancelLink(AccountFamily),
    OpenDesktop(ConversationId),
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
    StopSubagent(RunId),
    StopGoal(ConversationId),
    ContinueGoal(ConversationId),
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
    Usage(ProviderId),
    SetSkill {
        id: SkillId,
        enabled: bool,
    },
    StartServer(McpServerId),
    StopServer(McpServerId),
    AddServer(Box<CreateMcpServerInput>),
    InspectServer(String),
    SignIn(McpServerId),
    CancelSignIn(McpServerId),
    SetPolicy(PolicyChange),
    FocusVerification,
    Shell(String),
    StopShell,
    Copy(Selection),
    SaveShortcuts(serde_json::Value),
    SaveTheme(CliTheme),
    Processes(ConversationId),
    ProcessOutput {
        id: BackgroundShellId,
        since: Option<u64>,
    },
    KillProcess(BackgroundShellId),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    AccountLinked(AccountFamily, Result<Vec<ProviderDto>, AppError>),
    DesktopOpened(bool),
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
    GoalStopped(Result<ConversationDto, AppError>),
    Answered(Result<(), AppError>),
    RuleAdded(Result<PolicyRuleDto, AppError>),
    Conversations(Result<Vec<ConversationDto>, AppError>),
    Loaded(Result<ConversationDetailDto, AppError>),
    Attached(Result<Option<ChatAttachDto>, AppError>),
    Context(Result<ChatContextDto, AppError>),
    Usage(Result<Option<ProviderUsageDto>, AppError>),
    Skills(Result<Vec<SkillDto>, AppError>),
    Servers(Result<Vec<McpServerDto>, AppError>),
    SkillChanged(Result<SkillDto, AppError>),
    ServerChanged(Result<McpServerDto, AppError>),
    ServerAdded(Result<McpServerDto, AppError>),
    Inspected(Result<McpServerDetailDto, AppError>),
    SignedIn(Result<McpServerDto, AppError>),
    PolicySet(PolicyChange, Result<ExecPolicyDto, AppError>),
    Notice(zvs_contract::CliNotice),
    ShellOutput(String),
    ShellEnded(ShellEnd),
    Pointer(Pointer),
    Copied(bool),
    ShortcutsSaved(bool),
    ThemeSaved(bool),
    ThemeChanged(CliTheme),
    Processes(ConversationId, Result<Vec<BackgroundShellDto>, AppError>),
    ProcessOutput(Result<ShellOutputDto, AppError>),
    ProcessKilled(Result<BackgroundShellDto, AppError>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct App {
    pub phase: Phase,
    pub folder: String,
    pub grant: Option<FolderGrantScope>,
    pub conversation: Option<ConversationDto>,
    pub providers: Vec<ProviderDto>,
    pub model: Option<ModelChoice>,
    pub effort: ThinkingEffort,
    pub linking: Option<AccountFamily>,
    pub joining: Option<McpServerId>,
    pub signing: Option<McpServerId>,
    pub lookup_arg: Option<Arg>,
    pub lookup_servers: Option<Vec<McpServerDto>>,
    pub lookup_conversations: Option<Vec<ConversationDto>>,
    pub picked_load: bool,
    pub resume: Option<ConversationId>,
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
    pub pending_command: Option<ChatCommand>,
    pub goal: Option<GoalState>,
    pub goal_stop_armed: bool,
    pub expanded: bool,
    pub scroll: usize,
    pub escape_armed: bool,
    pub help: bool,
    pub keys: bool,
    pub keymap: Keymap,
    pub theme: CliTheme,
    pub status: Option<(Tone, String)>,
    pub processes: Vec<BackgroundShellDto>,
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
            effort: ThinkingEffort::Auto,
            linking: None,
            joining: None,
            signing: None,
            lookup_arg: None,
            lookup_servers: None,
            lookup_conversations: None,
            picked_load: false,
            resume: None,
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
            pending_command: None,
            goal: None,
            goal_stop_armed: false,
            expanded: false,
            scroll: 0,
            escape_armed: false,
            help: false,
            keys: false,
            keymap: Keymap::default(),
            theme: CliTheme::default(),
            status: None,
            processes: Vec::new(),
            frame: 0,
            started: 0,
            exit: None,
        }
    }

    pub const fn busy(&self) -> bool {
        self.run.is_some() || self.awaiting
    }

    pub fn active_theme(&self) -> CliTheme {
        self.overlay
            .as_ref()
            .filter(|overlay| overlay.panel == Panel::Theme)
            .and_then(|overlay| {
                let rows = overlay.rows(&self.view());
                rows.get(overlay.selected.min(rows.len().saturating_sub(1)))
                    .and_then(|row| CliTheme::parse(&row.key))
            })
            .unwrap_or(self.theme)
    }

    pub fn keys_open(&self) -> bool {
        self.keys && self.prompts.is_empty() && self.overlay.is_none() && !self.help
    }

    pub fn key_columns(&self) -> Vec<(&'static str, Vec<(String, &'static str)>)> {
        let bound = |shortcut: Shortcut| self.keymap.label(shortcut);
        let fixed = |keys: &str| keys.to_owned();
        vec![
            (
                copy::KEYS_COMPOSE,
                vec![
                    (fixed("/"), copy::KEY_COMMANDS),
                    (fixed("!"), copy::KEY_SHELL),
                    (fixed("Alt+Enter"), copy::KEY_NEWLINE),
                    (fixed("↑ / ↓"), copy::KEY_HISTORY),
                    (fixed("Tab"), copy::KEY_COMPLETE),
                    (bound(Shortcut::ClearInput), copy::KEY_CLEAR_INPUT),
                ],
            ),
            (
                copy::KEYS_SESSION,
                vec![
                    (fixed("Enter"), copy::KEY_SEND),
                    (fixed("Esc"), copy::KEY_STOP),
                    (bound(Shortcut::NewChat), copy::KEY_NEW_CHAT),
                    (bound(Shortcut::Models), copy::KEY_MODELS),
                    (bound(Shortcut::Effort), copy::KEY_EFFORT),
                    (bound(Shortcut::Resume), copy::KEY_RESUME),
                    (bound(Shortcut::Permissions), copy::KEY_PERMISSIONS),
                    (fixed("Esc Esc / Ctrl+C"), copy::KEY_QUIT),
                ],
            ),
            (
                copy::KEYS_TRANSCRIPT,
                vec![
                    (bound(Shortcut::Details), copy::KEY_DETAILS),
                    (fixed("PgUp / PgDn"), copy::KEY_SCROLL),
                    (bound(Shortcut::ClearScreen), copy::KEY_CLEAR_SCREEN),
                    (bound(Shortcut::Context), copy::KEY_CONTEXT),
                    (fixed(copy::KEY_MOUSE), copy::KEY_COPY),
                ],
            ),
        ]
    }

    fn palette_allowed(&self) -> bool {
        self.prompts.is_empty() && !self.help && self.overlay.is_none()
    }

    pub fn sources(&self) -> Sources<'_> {
        Sources {
            servers: self.lookup_servers.as_deref(),
            conversations: self.lookup_conversations.as_deref(),
            folder: &self.folder,
            goal: self.goal.map(|goal| goal.status),
        }
    }

    pub fn goal_elapsed_ms(&self) -> u64 {
        self.goal.map_or(0, |goal| {
            let running = if goal.active() {
                self.frame.saturating_sub(goal.seen) * 1000
                    / crate::ui::transcript::TICKS_PER_SECOND
            } else {
                0
            };
            goal.usage.elapsed_ms.saturating_add(running)
        })
    }

    pub fn palette_open(&self) -> bool {
        let text = self.input.text();
        self.palette_allowed()
            && is_command_text(text)
            && (palette::loading(text, &self.sources())
                || !palette::matches(text, &self.sources()).is_empty())
    }

    fn lookup(&mut self) -> Vec<Request> {
        let wanted = if self.palette_allowed() {
            arg_slot(self.input.text()).map(|slot| slot.arg)
        } else {
            None
        };
        if wanted == self.lookup_arg {
            return Vec::new();
        }
        self.lookup_arg = wanted;
        match wanted {
            Some(Arg::Server) => {
                self.lookup_servers = None;
                vec![Request::Servers]
            }
            Some(Arg::Conversation) => {
                self.lookup_conversations = None;
                vec![Request::Conversations]
            }
            Some(Arg::Theme) | None => Vec::new(),
        }
    }

    fn pick(&mut self, pick: Pick) -> Vec<Request> {
        match pick {
            Pick::Server(id) => {
                self.input.take();
                vec![Request::InspectServer(id.to_string())]
            }
            Pick::Theme(theme) => {
                self.input.take();
                self.act(Action::SetTheme(theme))
            }
            Pick::Conversation(id) => {
                if self.busy() {
                    self.status = Some((Tone::Info, copy::BUSY.to_owned()));
                    return Vec::new();
                }
                self.input.take();
                self.picked_load = true;
                vec![Request::Load(id)]
            }
        }
    }

    fn typed_argument(&mut self, command: Command, argument: &str) -> Vec<Request> {
        let wanted = argument.to_lowercase();
        match command {
            Command::Theme => {
                let found = CliTheme::ALL.iter().copied().find(|theme| {
                    theme.as_str() == wanted
                        || overlay::theme_label(*theme).to_lowercase() == wanted
                });
                match found {
                    Some(theme) => self.pick(Pick::Theme(theme)),
                    None => {
                        self.status = Some((
                            Tone::Error,
                            format!("{}: {argument}", copy::THEME_NOT_FOUND),
                        ));
                        Vec::new()
                    }
                }
            }
            Command::Resume => {
                let found = self.lookup_conversations.as_deref().and_then(|known| {
                    overlay::in_folder(known, &self.folder)
                        .into_iter()
                        .find(|conversation| conversation.title.to_lowercase() == wanted)
                        .map(|conversation| conversation.id)
                });
                match found {
                    Some(id) => self.pick(Pick::Conversation(id)),
                    None => {
                        self.status = Some((
                            Tone::Error,
                            format!("{}: {argument}", copy::RESUME_NOT_FOUND),
                        ));
                        Vec::new()
                    }
                }
            }
            other => self.command(other),
        }
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
        let typed = matches!(message, Msg::Key(_) | Msg::Paste(_));
        let mut requests = self.apply(message);
        if typed && self.exit.is_none() {
            requests.extend(self.lookup());
        }
        requests
    }

    fn apply(&mut self, message: Msg) -> Vec<Request> {
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
                self.poll_output()
            }
            Msg::Processes(owner, result) => {
                self.adopt_processes(owner, result);
                Vec::new()
            }
            Msg::ProcessOutput(result) => {
                self.adopt_output(result);
                Vec::new()
            }
            Msg::ProcessKilled(result) => {
                match result {
                    Ok(shell) => {
                        self.replace_process(shell);
                        self.status = Some((Tone::Success, copy::PROCESS_STOPPED.to_owned()));
                    }
                    Err(error) => self.status = Some((Tone::Error, error.message)),
                }
                Vec::new()
            }
            Msg::Connecting(message) => {
                if matches!(self.phase, Phase::Connecting(_)) {
                    self.phase = Phase::Connecting(message);
                }
                Vec::new()
            }
            Msg::Connected => vec![Request::Open, Request::Providers, Request::Policy],
            Msg::AccountLinked(family, result) => {
                self.linking = None;
                match result {
                    Ok(providers) => {
                        let linked: Vec<_> = providers
                            .iter()
                            .filter(|provider| {
                                provider.summary.adapter.account_family() == Some(family)
                                    && provider.summary.enabled
                            })
                            .cloned()
                            .collect();
                        self.model = default_model(&linked).or_else(|| self.model.clone());
                        self.providers = providers;
                        self.effort = ThinkingEffort::Auto;
                        self.status = Some((
                            Tone::Success,
                            format!(
                                "{} {}",
                                overlay::account_label(family),
                                copy::CONNECT_LINKED
                            ),
                        ));
                    }
                    Err(error) => self.fail(&error),
                }
                Vec::new()
            }
            Msg::DesktopOpened(opened) => {
                self.status = Some(if opened {
                    (Tone::Success, copy::DESKTOP_OPENED.to_owned())
                } else {
                    (Tone::Error, copy::DESKTOP_FAILED.to_owned())
                });
                Vec::new()
            }
            Msg::Pointer(pointer) => self.pointer(pointer),
            Msg::ShortcutsSaved(saved) => {
                if !saved {
                    self.status = Some((Tone::Error, copy::SHORTCUTS_SAVE_FAILED.to_owned()));
                }
                Vec::new()
            }
            Msg::ThemeSaved(saved) => {
                if !saved {
                    self.status = Some((Tone::Error, copy::THEME_SAVE_FAILED.to_owned()));
                }
                Vec::new()
            }
            Msg::ThemeChanged(theme) => {
                self.theme = theme;
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
                            return self.pending_resume();
                        }
                        self.phase = Phase::Access;
                        self.prompts.push_back(Prompt::access(&self.folder));
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
            Msg::PolicySet(change, result) => {
                match result {
                    Ok(policy) => {
                        self.adopt_policy(policy);
                        let text = match change {
                            PolicyChange::Judge(_) => copy::JUDGE_SET,
                            PolicyChange::Mode(_) => copy::MODE_SET,
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
                        if self.lookup_arg == Some(Arg::Conversation) {
                            self.lookup_conversations = Some(list.clone());
                        }
                        if let Some(overlay) = self.overlay_for(Panel::Resume) {
                            overlay.conversations = list;
                            overlay.loading = false;
                        }
                    }
                    Err(error) => {
                        self.lookup_conversations.get_or_insert_with(Vec::new);
                        self.panel_failed(&error);
                    }
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
            Msg::Usage(result) => {
                if let (Ok(usage), Some(overlay)) = (result, self.overlay_for(Panel::Context)) {
                    overlay.usage = usage;
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
                        if self.lookup_arg == Some(Arg::Server) {
                            self.lookup_servers = Some(list.clone());
                        }
                        if let Some(overlay) = self.overlay_for(Panel::Mcp) {
                            overlay.servers = list;
                            overlay.loading = false;
                        }
                    }
                    Err(error) => {
                        self.lookup_servers.get_or_insert_with(Vec::new);
                        self.panel_failed(&error);
                    }
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
                    Ok(server) if self.joining == Some(server.id) => {
                        self.joining = None;
                        self.merge_server(&server);
                        return self.joined(&server);
                    }
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
                    Err(error) => {
                        self.joining = None;
                        self.status = Some((Tone::Error, error.message));
                    }
                }
                Vec::new()
            }
            Msg::ServerAdded(result) => match result {
                Ok(server) => {
                    self.joining = Some(server.id);
                    self.status = Some((
                        Tone::Info,
                        format!("{} «{}»…", copy::MCP_STARTING_SERVER, server.name),
                    ));
                    if let Some(overlay) = self.overlay_for(Panel::Mcp) {
                        overlay.servers.push(server.clone());
                    }
                    vec![Request::StartServer(server.id)]
                }
                Err(error) => {
                    self.status = Some((Tone::Error, error.message));
                    Vec::new()
                }
            },
            Msg::Inspected(result) => {
                match result {
                    Ok(detail) => {
                        self.transcript
                            .entries
                            .push(transcript::Entry::Server(Box::new(detail)));
                        self.scroll = 0;
                    }
                    Err(error) => self.status = Some((Tone::Error, error.message)),
                }
                Vec::new()
            }
            Msg::SignedIn(result) => {
                self.signing = None;
                match result {
                    Ok(server) => {
                        self.merge_server(&server);
                        self.status = Some(server_outcome(&server, Some(copy::MCP_SIGNED_IN)));
                    }
                    Err(error) => self.status = Some((Tone::Error, error.message)),
                }
                Vec::new()
            }
            Msg::Loaded(result) => match result {
                Ok(detail) => {
                    let id = detail.conversation.id;
                    self.goal = detail
                        .conversation
                        .goal
                        .as_ref()
                        .map(|goal| GoalState::of(goal, self.frame))
                        .filter(GoalState::shown);
                    self.goal_stop_armed = false;
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
                    self.processes.clear();
                    let mut requests = vec![Request::Attach(id), Request::Processes(id)];
                    if self.grant.is_some() {
                        requests.push(Request::Bind {
                            id,
                            grant: self.grant,
                        });
                    }
                    requests
                }
                Err(error) => {
                    if std::mem::take(&mut self.picked_load) {
                        self.status = Some((Tone::Error, error.message));
                    } else if self.overlay.is_none() {
                        self.quit(Some(&error.message.clone()));
                    } else {
                        self.panel_failed(&error);
                    }
                    Vec::new()
                }
            },
            Msg::Attached(result) => {
                let woken = self.awaiting && self.run.is_none();
                let held = if woken {
                    self.awaiting = false;
                    std::mem::take(&mut self.held)
                } else {
                    Vec::new()
                };
                match result {
                    Ok(Some(live)) => {
                        let stream = live.run.stream_id;
                        let seen = live.seq;
                        self.adopt_live(live);
                        return held
                            .into_iter()
                            .filter(|event| {
                                event.stream_id == stream
                                    && seen.is_none_or(|seen| event.seq > seen)
                            })
                            .flat_map(|event| self.event(event))
                            .collect();
                    }
                    Ok(None) => {
                        if woken {
                            self.transcript.live = None;
                        }
                    }
                    Err(error) => {
                        if woken {
                            self.transcript.live = None;
                        }
                        self.fail(&error);
                    }
                }
                Vec::new()
            }
            Msg::Created(result) => match result {
                Ok(conversation) => {
                    let id = conversation.id;
                    self.conversation = Some(conversation);
                    self.processes.clear();
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
                    let command = self.pending_command.take();
                    self.pending
                        .take()
                        .map(|text| self.send(id, text, command))
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
            Msg::GoalStopped(result) => {
                match result {
                    Ok(conversation) => {
                        self.goal = conversation
                            .goal
                            .as_ref()
                            .map(|goal| GoalState::of(goal, self.frame));
                        self.conversation = Some(conversation);
                        self.transcript.notice(Tone::Info, copy::GOAL_STOPPED);
                    }
                    Err(error) => self.fail(&error),
                }
                Vec::new()
            }
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
                        if self
                            .goal
                            .is_some_and(|goal| goal.active() && goal.usage.rounds == 0)
                        {
                            self.goal = None;
                        }
                        self.fail(&error);
                        Vec::new()
                    }
                }
            }
            Msg::Event(event) => {
                if let Some(requests) = self.process_event(&event) {
                    return requests;
                }
                if let Some(requests) = self.wake_event(&event) {
                    return requests;
                }
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

    pub fn running_processes(&self) -> usize {
        self.processes
            .iter()
            .filter(|shell| shell.status.running())
            .count()
    }

    fn wake_event(&mut self, event: &HostEvent) -> Option<Vec<Request>> {
        let HostEventDraft::Step { step } = &event.body else {
            return None;
        };
        if step.get("domain").and_then(|domain| domain.as_str()) != Some(WAKE_STEP_DOMAIN) {
            return None;
        }
        let Ok(wake) =
            serde_json::from_value::<ChatWakeStepDto>(serde_json::Value::Object(step.clone()))
        else {
            return Some(Vec::new());
        };
        let conversation = self
            .conversation
            .as_ref()
            .map(|conversation| conversation.id);
        if conversation != Some(wake.message.conversation_id) || self.busy() {
            return Some(Vec::new());
        }
        if let Some(result) = &wake.message.event {
            self.transcript
                .notice(Tone::Info, copy::process_event(result));
        }
        self.transcript.start();
        self.started = self.frame;
        self.awaiting = true;
        Some(vec![Request::Attach(wake.message.conversation_id)])
    }

    fn process_event(&mut self, event: &HostEvent) -> Option<Vec<Request>> {
        let HostEventDraft::Step { step } = &event.body else {
            return None;
        };
        if step.get("domain").and_then(|domain| domain.as_str()) != Some(SHELL_STEP_DOMAIN) {
            return None;
        }
        let Ok(status) = serde_json::from_value::<BackgroundShellStatusDto>(
            serde_json::Value::Object(step.clone()),
        ) else {
            return Some(Vec::new());
        };
        let Some(conversation) = self
            .conversation
            .as_ref()
            .map(|conversation| conversation.id)
        else {
            return Some(Vec::new());
        };
        if status.owner_id != conversation.to_string() {
            return Some(Vec::new());
        }
        let Some(known) = self
            .processes
            .iter_mut()
            .find(|shell| shell.id == status.id)
        else {
            return Some(vec![Request::Processes(conversation)]);
        };
        known.status = status.status;
        known.exit_code = status.exit_code;
        if !status.status.running() {
            known.ended_at = known.ended_at.or(Some(event.ts));
        }
        let shell = known.clone();
        self.replace_process(shell);
        Some(Vec::new())
    }

    fn adopt_processes(
        &mut self,
        owner: ConversationId,
        result: Result<Vec<BackgroundShellDto>, AppError>,
    ) {
        if self
            .conversation
            .as_ref()
            .map(|conversation| conversation.id)
            != Some(owner)
        {
            return;
        }
        match result {
            Ok(list) => {
                self.processes.clone_from(&list);
                if let Some(overlay) = self.overlay_for(Panel::Processes) {
                    overlay.processes = list;
                    overlay.loading = false;
                }
            }
            Err(error) => {
                if self.overlay_for(Panel::Processes).is_some() {
                    self.panel_failed(&error);
                }
            }
        }
    }

    fn replace_process(&mut self, shell: BackgroundShellDto) {
        if let Some(known) = self.processes.iter_mut().find(|known| known.id == shell.id) {
            known.clone_from(&shell);
        }
        if let Some(overlay) = self.overlay.as_mut() {
            if let Some(known) = overlay
                .processes
                .iter_mut()
                .find(|known| known.id == shell.id)
            {
                known.clone_from(&shell);
            }
            if let Some(output) = overlay
                .output
                .as_mut()
                .filter(|output| output.id == shell.id)
            {
                output.running = shell.status.running();
            }
        }
    }

    fn adopt_output(&mut self, result: Result<ShellOutputDto, AppError>) {
        let output = match result {
            Ok(output) => output,
            Err(error) => {
                if self.overlay_for(Panel::ProcessOutput).is_some() {
                    self.panel_failed(&error);
                }
                return;
            }
        };
        let shell = output.shell.clone();
        if let Some(overlay) = self.overlay_for(Panel::ProcessOutput)
            && let Some(view) = overlay.output.as_mut().filter(|view| view.id == shell.id)
        {
            view.absorb(&output.text, output.next_seq, output.truncated);
            view.running = shell.status.running();
            let follow = view.follow;
            let count = view.shown().len() + usize::from(view.truncated);
            overlay.loading = false;
            if follow {
                overlay.selected = count.saturating_sub(1);
            }
        }
        self.replace_process(shell);
    }

    fn poll_output(&self) -> Vec<Request> {
        if !self.frame.is_multiple_of(OUTPUT_POLL_TICKS) {
            return Vec::new();
        }
        self.overlay
            .as_ref()
            .filter(|overlay| overlay.panel == Panel::ProcessOutput && !overlay.loading)
            .and_then(|overlay| overlay.output.as_ref())
            .filter(|output| output.running)
            .map(|output| {
                vec![Request::ProcessOutput {
                    id: output.id,
                    since: output.next_seq,
                }]
            })
            .unwrap_or_default()
    }

    fn pending_resume(&mut self) -> Vec<Request> {
        self.resume
            .take()
            .map(|id| vec![Request::Load(id)])
            .unwrap_or_default()
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
            HostEventDraft::Step { step }
                if step.get("domain").and_then(serde_json::Value::as_str)
                    == Some(GOAL_STEP_DOMAIN) =>
            {
                self.goal_step(step);
            }
            HostEventDraft::End { outcome } => {
                self.transcript.finish();
                if let Some(goal) = self.goal.as_mut().filter(|goal| goal.active()) {
                    match outcome.status {
                        RunOutcomeStatus::Cancelled => {
                            goal.status = GoalStatus::Stopped;
                            goal.reason = Some(GoalReason::User);
                        }
                        RunOutcomeStatus::Failed | RunOutcomeStatus::Ok => {
                            goal.status = GoalStatus::Failed;
                            goal.reason = Some(GoalReason::Error);
                            self.transcript.notice(Tone::Error, copy::GOAL_FAILED);
                        }
                    }
                    self.goal_stop_armed = false;
                }
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

    fn goal_step(&mut self, step: &serde_json::Map<String, serde_json::Value>) {
        let Ok(progress) =
            serde_json::from_value::<ChatGoalStepDto>(serde_json::Value::Object(step.clone()))
        else {
            return;
        };
        if self
            .conversation
            .as_ref()
            .map(|conversation| conversation.id)
            != Some(progress.conversation_id)
        {
            return;
        }
        let before = self.goal.map(|goal| goal.status);
        self.goal = Some(GoalState {
            status: progress.status,
            reason: progress.reason,
            usage: progress.usage,
            budget: progress.budget,
            seen: self.frame,
        });
        if before == Some(progress.status) {
            return;
        }
        match progress.status {
            GoalStatus::Paused => self.transcript.notice(
                Tone::Info,
                crate::format::with_reason(copy::GOAL_PAUSED, progress.reason),
            ),
            GoalStatus::Stopped => self.transcript.notice(
                Tone::Info,
                crate::format::with_reason(copy::GOAL_STOPPED, progress.reason),
            ),
            GoalStatus::Active | GoalStatus::Done | GoalStatus::Failed => {}
        }
        if !progress.status.eq(&GoalStatus::Active) {
            self.goal_stop_armed = false;
        }
    }

    fn goal_command(&mut self, argument: &str) -> Vec<Request> {
        let word = argument.to_lowercase();
        if word == Sub::GoalStop.name() {
            return self.goal_sub(Sub::GoalStop);
        }
        if word == Sub::GoalContinue.name() {
            return self.goal_sub(Sub::GoalContinue);
        }
        if argument.is_empty() {
            self.input.set("/goal ");
            self.status = Some((Tone::Info, copy::GOAL_HINT.to_owned()));
            return Vec::new();
        }
        if self.goal.is_some_and(|goal| goal.active()) {
            self.status = Some((Tone::Info, copy::GOAL_RUNNING.to_owned()));
            return Vec::new();
        }
        self.submit_with(argument.to_owned(), Some(ChatCommand::Goal))
    }

    fn goal_sub(&mut self, sub: Sub) -> Vec<Request> {
        self.palette.reset();
        let id = self
            .conversation
            .as_ref()
            .map(|conversation| conversation.id);
        let status = self.goal.map(|goal| goal.status);
        match (sub, id) {
            (Sub::GoalStop, Some(id)) if sub.offered(status) => {
                self.goal_stop_armed = false;
                self.status = Some((Tone::Info, copy::GOAL_STOPPING.to_owned()));
                vec![Request::StopGoal(id)]
            }
            (Sub::GoalContinue, Some(id)) if sub.offered(status) => {
                if self.busy() {
                    self.status = Some((Tone::Info, copy::BUSY.to_owned()));
                    return Vec::new();
                }
                self.awaiting = true;
                self.transcript.notice(Tone::Info, copy::GOAL_CONTINUED);
                self.transcript.start();
                self.scroll = 0;
                self.started = self.frame;
                if let Some(goal) = self.goal.as_mut() {
                    goal.status = GoalStatus::Active;
                    goal.reason = None;
                    goal.seen = self.frame;
                }
                vec![Request::ContinueGoal(id)]
            }
            (Sub::GoalStop, _) => {
                self.status = Some((Tone::Info, copy::GOAL_NOT_STOPPABLE.to_owned()));
                Vec::new()
            }
            _ => {
                self.status = Some((Tone::Info, copy::GOAL_NOT_PAUSED.to_owned()));
                Vec::new()
            }
        }
    }

    fn send(&mut self, id: ConversationId, text: String, command: Option<ChatCommand>) -> Request {
        self.awaiting = true;
        self.transcript.start();
        if command == Some(ChatCommand::Goal) {
            self.goal = Some(GoalState::started(self.frame));
            self.goal_stop_armed = false;
        }
        let mut input = ChatSendInput::new(id, text);
        input.command = command;
        input.vendor.effort = Some(self.effort);
        if let Some(model) = &self.model
            && self
                .providers
                .iter()
                .find(|provider| provider.summary.id == model.provider)
                .is_some_and(|provider| {
                    provider.summary.adapter == zvs_contract::AdapterFamily::Copilot
                        && !provider.models.iter().any(|known| {
                            known.external_id == model.model
                                && known
                                    .capabilities
                                    .contains(&zvs_contract::ModelCapability::Reasoning)
                        })
                })
        {
            input.vendor.effort = Some(ThinkingEffort::Auto);
        }
        if let Some(model) = &self.model {
            input.provider_id = Some(model.provider);
            input.model_id = Some(model.model.clone());
        }
        Request::Send(Box::new(input))
    }

    fn submit_message(&mut self, text: String) -> Vec<Request> {
        if self.goal.is_some_and(|goal| goal.active()) {
            self.input.set(&text);
            self.status = Some((Tone::Info, copy::GOAL_RUNNING.to_owned()));
            return Vec::new();
        }
        self.submit_with(text, None)
    }

    fn submit_with(&mut self, text: String, command: Option<ChatCommand>) -> Vec<Request> {
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
        self.transcript.push_user(&match command {
            Some(command) => format!("/{} {text}", command.as_str()),
            None => text.clone(),
        });
        self.scroll = 0;
        self.started = self.frame;
        if let Some(id) = self
            .conversation
            .as_ref()
            .map(|conversation| conversation.id)
        {
            return vec![self.send(id, text, command), Request::Policy];
        }
        self.awaiting = true;
        self.pending = Some(text);
        self.pending_command = command;
        if command == Some(ChatCommand::Goal) {
            self.goal = Some(GoalState::started(self.frame));
        }
        vec![
            Request::Create {
                provider: model.provider,
                model: model.model,
            },
            Request::Policy,
        ]
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
                    self.goal = None;
                    self.goal_stop_armed = false;
                    self.transcript = Transcript::default();
                    self.scroll = 0;
                    self.status = Some((Tone::Success, copy::NEW_CHAT.to_owned()));
                }
            }
            Command::Resume => return self.open_panel(Panel::Resume),
            Command::Models => return self.open_panel(Panel::Models),
            Command::Effort => return self.open_panel(Panel::Effort),
            Command::Connect => {
                if self.busy() || self.linking.is_some() || self.signing.is_some() {
                    self.status = Some((Tone::Info, copy::BUSY.to_owned()));
                } else {
                    return self.open_panel(Panel::Connect);
                }
            }
            Command::Desktop => match &self.conversation {
                Some(conversation) => return vec![Request::OpenDesktop(conversation.id)],
                None => {
                    self.status = Some((Tone::Info, copy::DESKTOP_NO_CONVERSATION.to_owned()));
                }
            },
            Command::Context => return self.open_panel(Panel::Context),
            Command::Goal => return self.goal_command(""),
            Command::Skills => return self.open_panel(Panel::Skills),
            Command::Mcp => return self.open_panel(Panel::Mcp),
            Command::Processes => return self.open_panel(Panel::Processes),
            Command::Permissions => return self.open_panel(Panel::Permissions),
            Command::Shortcuts => return self.open_panel(Panel::Shortcuts),
            Command::Theme => return self.open_panel(Panel::Theme),
            Command::StopSubagent => match self.transcript.newest_subagent() {
                Some(run) => {
                    self.status = Some((Tone::Info, copy::SUBAGENT_STOPPING.to_owned()));
                    return vec![Request::StopSubagent(run)];
                }
                None => self.status = Some((Tone::Info, copy::SUBAGENT_NONE.to_owned())),
            },
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
            effort: self.effort,
            keymap: &self.keymap,
            theme: self.theme,
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
            Panel::Connect
            | Panel::Models
            | Panel::Effort
            | Panel::Shortcuts
            | Panel::Theme
            | Panel::ProcessOutput => Vec::new(),
            Panel::Processes => match &self.conversation {
                Some(conversation) => vec![Request::Processes(conversation.id)],
                None => {
                    self.status = Some((Tone::Info, copy::PROCESSES_NO_CONVERSATION.to_owned()));
                    return Vec::new();
                }
            },
            Panel::Resume => vec![Request::Conversations],
            Panel::Skills => vec![Request::Skills],
            Panel::Mcp => vec![Request::Servers],
            Panel::Permissions => vec![Request::Policy],
            Panel::Context => match &self.model {
                Some(model) => vec![
                    Request::Context {
                        conversation: self
                            .conversation
                            .as_ref()
                            .map(|conversation| conversation.id),
                        provider: model.provider,
                        model: model.model.clone(),
                    },
                    Request::Usage(model.provider),
                ],
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
            Action::Connect(family) => {
                if self.busy() {
                    self.status = Some((Tone::Info, copy::BUSY.to_owned()));
                    return Vec::new();
                }
                self.overlay = None;
                self.linking = Some(family);
                self.status = Some((
                    Tone::Info,
                    format!(
                        "{} {}… {}",
                        copy::CONNECT_LINKING,
                        overlay::account_label(family),
                        copy::CONNECT_CANCEL_HINT
                    ),
                ));
                vec![Request::LinkAccount(family)]
            }
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
            Action::SetEffort(effort) => {
                self.effort = effort;
                self.status = Some((
                    Tone::Success,
                    format!("{}: {}", copy::EFFORT_SET, overlay::effort_label(effort)),
                ));
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
            Action::SignIn(id) => {
                let server = self
                    .overlay
                    .as_ref()
                    .and_then(|overlay| overlay.servers.iter().find(|known| known.id == id))
                    .cloned();
                server.map_or_else(Vec::new, |server| self.sign_in(&server))
            }
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
                vec![Request::SetPolicy(PolicyChange::Mode(mode))]
            }
            Action::SetJudge(judge) => vec![Request::SetPolicy(PolicyChange::Judge(judge))],
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
            Action::SetTheme(theme) => {
                self.theme = theme;
                self.overlay = None;
                self.status = Some((
                    Tone::Success,
                    format!("{}: {}", copy::THEME_SET, overlay::theme_label(theme)),
                ));
                vec![Request::SaveTheme(theme)]
            }
            Action::ShowOutput(id) => {
                let Some(shell) = self.processes.iter().find(|shell| shell.id == id).cloned()
                else {
                    return Vec::new();
                };
                let mut overlay = Overlay::new(Panel::ProcessOutput);
                overlay.output = Some(ProcessOutput::new(&shell));
                self.overlay = Some(overlay);
                vec![Request::ProcessOutput { id, since: None }]
            }
        }
    }

    fn mcp(&mut self, arguments: &str) -> Vec<Request> {
        let words = match crate::mcp_add::split(arguments) {
            Ok(words) => words,
            Err(message) => {
                self.status = Some((Tone::Error, message.to_owned()));
                return Vec::new();
            }
        };
        let mut words = words.into_iter();
        match words.next().as_deref() {
            Some("add") => {}
            Some("inspect") => {
                let name = words.collect::<Vec<_>>().join(" ");
                if name.is_empty() {
                    self.status = Some((Tone::Error, copy::MCP_INSPECT_USAGE.to_owned()));
                    return Vec::new();
                }
                return vec![Request::InspectServer(name)];
            }
            _ => {
                self.status = Some((Tone::Error, copy::MCP_ADD_UNKNOWN.to_owned()));
                return Vec::new();
            }
        }
        if self.linking.is_some() || self.signing.is_some() || self.joining.is_some() {
            self.status = Some((Tone::Info, copy::BUSY.to_owned()));
            return Vec::new();
        }
        match crate::mcp_add::parse(words) {
            Ok(input) => {
                self.status = Some((
                    Tone::Info,
                    format!("{} «{}»…", copy::MCP_ADDING, input.name),
                ));
                vec![Request::AddServer(Box::new(input))]
            }
            Err(message) => {
                self.status = Some((Tone::Error, message.to_owned()));
                Vec::new()
            }
        }
    }

    fn merge_server(&mut self, server: &McpServerDto) {
        if let Some(overlay) = self.overlay_for(Panel::Mcp)
            && let Some(known) = overlay
                .servers
                .iter_mut()
                .find(|known| known.id == server.id)
        {
            *known = server.clone();
        }
    }

    fn joined(&mut self, server: &McpServerDto) -> Vec<Request> {
        if server.oauth == McpOAuthState::Required {
            return self.sign_in(server);
        }
        self.status = Some(server_outcome(server, None));
        Vec::new()
    }

    fn sign_in(&mut self, server: &McpServerDto) -> Vec<Request> {
        if self.busy() || self.linking.is_some() || self.signing.is_some() {
            self.status = Some((Tone::Info, copy::BUSY.to_owned()));
            return Vec::new();
        }
        self.overlay = None;
        self.signing = Some(server.id);
        self.status = Some((
            Tone::Info,
            format!(
                "{} «{}» {}… {}",
                copy::MCP_SIGNING_IN,
                server.name,
                copy::MCP_IN_BROWSER,
                copy::CONNECT_CANCEL_HINT
            ),
        ));
        vec![Request::SignIn(server.id)]
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
                self.keys = false;
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
        if overlay.panel == Panel::ProcessOutput {
            match key {
                Key::Up | Key::PageUp => {
                    let step = if key == Key::Up { 1 } else { SCROLL_STEP };
                    overlay.selected = overlay.selected.saturating_sub(step);
                    if let Some(output) = overlay.output.as_mut() {
                        output.follow = false;
                    }
                    return Vec::new();
                }
                Key::Down | Key::PageDown => {
                    let step = if key == Key::Down { 1 } else { SCROLL_STEP };
                    overlay.selected = (overlay.selected + step).min(rows.len().saturating_sub(1));
                    return Vec::new();
                }
                Key::End => {
                    overlay.selected = rows.len().saturating_sub(1);
                    if let Some(output) = overlay.output.as_mut() {
                        output.follow = true;
                    }
                    return Vec::new();
                }
                _ => {}
            }
        }
        if overlay.panel == Panel::Processes && key == Key::Delete {
            let target = rows
                .get(overlay.selected)
                .and_then(|row| {
                    self.processes
                        .iter()
                        .find(|shell| shell.id.to_string() == row.key)
                })
                .filter(|shell| shell.status.running())
                .map(|shell| shell.id);
            let Some(id) = target else {
                return Vec::new();
            };
            if !overlay.armed {
                overlay.armed = true;
                self.status = Some((Tone::Error, copy::PROCESS_STOP_CONFIRM.to_owned()));
                return Vec::new();
            }
            overlay.armed = false;
            return vec![Request::KillProcess(id)];
        }
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
            && let Some(item) = self.palette.current(self.input.text(), &self.sources())
        {
            self.palette.reset();
            return match item {
                Item::Command(command) => {
                    self.input.take();
                    self.command(command)
                }
                Item::Sub(sub) if sub.immediate() => {
                    self.input.take();
                    self.goal_sub(sub)
                }
                Item::Sub(_) => {
                    self.input.set(&item.completion());
                    Vec::new()
                }
                Item::Value(value) => self.pick(value.pick),
            };
        }
        let text = self.input.take();
        if text.is_empty() {
            return Vec::new();
        }
        if text.starts_with('!') {
            return self.submit_shell(&text);
        }
        if text.starts_with('/') && !text.starts_with("//") {
            return match Command::parse(&text) {
                Some(Command::Mcp) if !palette::arguments(&text).is_empty() => {
                    self.palette.reset();
                    self.mcp(palette::arguments(&text))
                }
                Some(Command::Goal) => {
                    self.palette.reset();
                    self.goal_command(palette::arguments(&text))
                }
                Some(command)
                    if command.arg().is_some() && !palette::arguments(&text).is_empty() =>
                {
                    self.palette.reset();
                    self.typed_argument(command, palette::arguments(&text))
                }
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
        if let Some(family) = self.linking
            && matches!(key, Key::Esc | Key::Interrupt)
        {
            return vec![Request::CancelLink(family)];
        }
        if let Some(id) = self.signing
            && matches!(key, Key::Esc | Key::Interrupt)
        {
            return vec![Request::CancelSignIn(id)];
        }
        if (self.linking.is_some() || self.signing.is_some())
            && !matches!(key, Key::PageUp | Key::PageDown)
        {
            return Vec::new();
        }
        self.selection = None;
        if key != Key::Esc {
            self.escape_armed = false;
        }
        if key != Key::Interrupt {
            self.goal_stop_armed = false;
        }
        self.status = None;
        if let Key::Chord(chord) = key
            && !self.capturing()
        {
            return self.shortcut(chord);
        }
        match key {
            Key::Interrupt => {
                if let Some(id) = self
                    .conversation
                    .as_ref()
                    .map(|conversation| conversation.id)
                    .filter(|_| self.goal.is_some_and(|goal| goal.active()))
                {
                    if self.goal_stop_armed {
                        self.goal_stop_armed = false;
                        self.status = Some((Tone::Info, copy::GOAL_STOPPING.to_owned()));
                        return vec![Request::StopGoal(id)];
                    }
                    self.goal_stop_armed = true;
                    self.status = Some((Tone::Info, copy::GOAL_STOP_CONFIRM.to_owned()));
                    return Vec::new();
                }
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
        if self.keys {
            self.keys = false;
            if matches!(key, Key::Esc | Key::Char('?')) {
                return Vec::new();
            }
        }
        if self.palette_open() {
            let text = self.input.text().to_owned();
            match key {
                Key::Up => {
                    let sources = self.sources();
                    let mut palette = self.palette.clone();
                    palette.up(&text, &sources);
                    self.palette = palette;
                    return Vec::new();
                }
                Key::Down => {
                    let sources = self.sources();
                    let mut palette = self.palette.clone();
                    palette.down(&text, &sources);
                    self.palette = palette;
                    return Vec::new();
                }
                Key::Tab => {
                    if let Some(item) = self.palette.current(&text, &self.sources()) {
                        self.input.set(&item.completion());
                        self.palette.reset();
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
            Key::Char('?') if self.input.is_empty() => self.keys = true,
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
                    _ => {
                        self.quit(Some(copy::ACCESS_DENIED));
                        return Vec::new();
                    }
                }
                self.phase = Phase::Chat;
                self.pending_resume()
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

pub fn server_outcome(server: &McpServerDto, prefix: Option<&str>) -> (Tone, String) {
    let lead = prefix.map(|text| format!("{text}. ")).unwrap_or_default();
    if server.status == McpServerStatus::Connected {
        (
            Tone::Success,
            format!(
                "{lead}{} «{}» {} · {}",
                copy::MCP_SERVER,
                server.name,
                copy::MCP_CONNECTED,
                crate::format::tools(server.tool_count)
            ),
        )
    } else {
        let reason = server
            .last_error
            .as_deref()
            .map(|error| format!(": {error}"))
            .unwrap_or_default();
        (
            Tone::Error,
            format!(
                "{lead}{} «{}» {}{reason}",
                copy::MCP_SERVER,
                server.name,
                copy::MCP_NOT_STARTED
            ),
        )
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
