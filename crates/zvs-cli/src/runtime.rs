use std::io::{self, IsTerminal};
use std::time::Duration;

use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::crossterm::execute;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use zvs_contract::{
    AnswerFolderRequestInput, AppError, AppErrorCode, BindFolderInput, CliOpenInput,
    CreateMcpServerInput, ExecPolicyDto, McpOAuthState, McpServerDetailDto, McpServerDto,
    ProviderDto, ProviderSummaryDto,
};

use crate::args::{self, Args};
use crate::copy;
use crate::keymap_file;
use crate::launch::{self, Connection};
use crate::link::{Incoming, Link};
use crate::shell::{self, ShellControl};
use crate::state::shortcuts::{Chord, ChordKey};
use crate::state::transcript::Tone;
use crate::state::{App, Exit, Key, Msg, Pointer, PolicyChange, Request, Selection};
use crate::theme_file;
use crate::ui;
use crate::version;

const TICK: Duration = Duration::from_millis(100);
const THEME_POLL: Duration = Duration::from_secs(1);
const WHEEL_STEP: isize = 3;

enum Signal {
    App(Box<Msg>),
    Linked(Connection),
}

const BATCH_MAX: usize = 4096;

pub fn typed_later(rest: &[Event]) -> bool {
    rest.iter().any(|event| {
        matches!(
            event,
            Event::Key(later)
                if later.kind != KeyEventKind::Release
                    && !later.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(later.code, KeyCode::Char(value) if !value.is_control())
        )
    })
}

pub fn key(event: KeyEvent, pasting: bool) -> Option<Key> {
    let control = event.modifiers.contains(KeyModifiers::CONTROL);
    let alt = event.modifiers.contains(KeyModifiers::ALT);
    let shift = event.modifiers.contains(KeyModifiers::SHIFT);
    let shifted = shift || alt;
    Some(match event.code {
        KeyCode::Char(value) if control && !alt => match value.to_ascii_lowercase() {
            'c' | 'd' => Key::Interrupt,
            'j' | 'm' if pasting => Key::Newline,
            'j' | 'm' => Key::Enter,
            _ => Key::Chord(Chord::character(value, true, false, shift)?),
        },
        KeyCode::Char(value) if alt && !control && !pasting && !value.is_control() => {
            Key::Chord(Chord::character(value, false, true, shift)?)
        }
        KeyCode::F(number) => Key::Chord(Chord::function_key(number, control, alt, shift)?),
        KeyCode::Up if alt && !control => {
            Key::Chord(Chord::named(ChordKey::Up, false, true, shift)?)
        }
        KeyCode::Down if alt && !control => {
            Key::Chord(Chord::named(ChordKey::Down, false, true, shift)?)
        }
        KeyCode::Enter | KeyCode::Char('\r' | '\n') if alt && !shift && !control && !pasting => {
            Key::Chord(Chord::named(ChordKey::Enter, false, true, false)?)
        }
        KeyCode::Enter | KeyCode::Char('\r' | '\n') if shifted || pasting => Key::Newline,
        KeyCode::Enter | KeyCode::Char('\r' | '\n') => Key::Enter,
        KeyCode::Char(value) => Key::Char(value),
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up | KeyCode::BackTab => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::Tab => Key::Tab,
        KeyCode::Esc => Key::Esc,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        _ => return None,
    })
}

pub fn translate(batch: &[Event]) -> Vec<Msg> {
    batch
        .iter()
        .enumerate()
        .filter_map(|(index, next)| match next {
            Event::Key(pressed) if pressed.kind != KeyEventKind::Release => {
                key(*pressed, typed_later(&batch[index + 1..])).map(Msg::Key)
            }
            Event::Paste(text) => Some(Msg::Paste(text.clone())),
            Event::Mouse(mouse) => {
                let at = (mouse.column, mouse.row);
                match mouse.kind {
                    MouseEventKind::ScrollUp => Some(Msg::Scroll(WHEEL_STEP)),
                    MouseEventKind::ScrollDown => Some(Msg::Scroll(-WHEEL_STEP)),
                    MouseEventKind::Down(MouseButton::Left) => {
                        Some(Msg::Pointer(Pointer::Press(at.0, at.1)))
                    }
                    MouseEventKind::Drag(MouseButton::Left) => {
                        Some(Msg::Pointer(Pointer::Drag(at.0, at.1)))
                    }
                    MouseEventKind::Up(MouseButton::Left) => {
                        Some(Msg::Pointer(Pointer::Release(at.0, at.1)))
                    }
                    _ => None,
                }
            }
            Event::Resize(width, height) => Some(Msg::Resize(*width, *height)),
            _ => None,
        })
        .collect()
}

fn read_terminal(signals: &mpsc::UnboundedSender<Signal>) {
    loop {
        let Ok(first) = event::read() else {
            return;
        };
        let mut batch = vec![first];
        while batch.len() < BATCH_MAX && event::poll(Duration::ZERO).unwrap_or(false) {
            match event::read() {
                Ok(next) => batch.push(next),
                Err(_) => break,
            }
        }
        for message in translate(&batch) {
            if signals.send(Signal::App(Box::new(message))).is_err() {
                return;
            }
        }
    }
}

fn void(result: Result<Value, AppError>) -> Msg {
    Msg::Answered(result.map(drop))
}

async fn providers(link: &Link) -> Result<Vec<ProviderDto>, AppError> {
    let summaries: Vec<ProviderSummaryDto> = link
        .call(
            "providers.list",
            &json!({ "capability": "text", "enabled": true }),
        )
        .await?;
    let mut providers = Vec::with_capacity(summaries.len());
    for summary in summaries {
        let provider: ProviderDto = link
            .call(
                "providers.get",
                &json!({ "id": summary.id, "selectedOnly": true }),
            )
            .await?;
        providers.push(provider);
    }
    Ok(providers)
}

async fn set_policy(link: &Link, change: &PolicyChange) -> Result<ExecPolicyDto, AppError> {
    let mut policy: ExecPolicyDto = link.call("execPolicy.get", &()).await?;
    match change {
        PolicyChange::Mode(mode) => policy.mode = *mode,
        PolicyChange::Judge(judge) => policy.judge.clone_from(judge),
    }
    link.call("execPolicy.set", &policy).await
}

pub fn find_server<'a>(servers: &'a [McpServerDto], name: &str) -> Option<&'a McpServerDto> {
    let wanted = name.trim().to_lowercase();
    servers
        .iter()
        .find(|server| server.name.to_lowercase() == wanted)
        .or_else(|| servers.iter().find(|server| server.namespace == wanted))
        .or_else(|| {
            servers
                .iter()
                .find(|server| server.id.to_string() == wanted)
        })
}

async fn inspect_server(link: &Link, name: &str) -> Result<McpServerDetailDto, AppError> {
    let servers: Vec<McpServerDto> = link.call("mcp.list", &()).await?;
    let server = find_server(&servers, name).ok_or_else(|| {
        AppError::new(
            AppErrorCode::NotFound,
            format!("{}: {name}", copy::MCP_NOT_FOUND),
        )
    })?;
    link.call("mcp.get", &json!({ "id": server.id })).await
}

pub async fn execute(link: Link, request: Request, folder: String) -> Msg {
    match request {
        Request::Open => Msg::Opened(link.call("cli.open", &CliOpenInput { cwd: folder }).await),
        Request::Providers => Msg::Providers(providers(&link).await),
        Request::LinkAccount(family) => {
            #[cfg(windows)]
            zvs_pipe::allow_foreground();
            let result: Result<zvs_contract::AccountLinkResult, AppError> = link
                .call("cli.accounts.link", &json!({ "adapter": family }))
                .await;
            Msg::AccountLinked(
                family,
                match result {
                    Ok(zvs_contract::AccountLinkResult::Linked { .. }) => providers(&link).await,
                    Ok(
                        zvs_contract::AccountLinkResult::Cancelled { detail }
                        | zvs_contract::AccountLinkResult::Timeout { detail },
                    ) => Err(AppError::new(
                        zvs_contract::AppErrorCode::RunCancelled,
                        detail,
                    )),
                    Err(error) => Err(error),
                },
            )
        }
        Request::CancelLink(family) => {
            let result: Result<zvs_contract::AccountsCancelLinkOutput, AppError> = link
                .call("cli.accounts.cancelLink", &json!({ "adapter": family }))
                .await;
            Msg::Answered(result.map(|_| ()))
        }
        Request::Policy => Msg::Policy(link.call("execPolicy.get", &()).await),
        Request::Create { provider, model } => Msg::Created(
            link.call(
                "chat.conversations.create",
                &json!({ "providerId": provider, "modelId": model }),
            )
            .await,
        ),
        Request::Bind { id, grant } => Msg::Bound(
            link.call(
                "chat.bindFolder",
                &BindFolderInput {
                    id,
                    path: folder,
                    grant,
                },
            )
            .await,
        ),
        Request::AttachmentOptions { images } => {
            let stores: Result<Vec<zvs_contract::VectorStoreDto>, AppError> =
                link.call("vectorStores.list", &()).await;
            let result = match stores {
                Ok(stores) => {
                    let mut options: Vec<_> = stores
                        .into_iter()
                        .map(|s| crate::state::attachments::Attachment::Store {
                            id: s.id,
                            name: s.name,
                        })
                        .collect();
                    if images {
                        options.extend(
                            tokio::task::spawn_blocking(move || {
                                crate::state::attachments::discover(&folder)
                            })
                            .await
                            .unwrap_or_default(),
                        );
                    }
                    Ok(options)
                }
                Err(error) => Err(error),
            };
            Msg::AttachmentOptions(result)
        }
        Request::SendAttachments {
            mut input,
            attachments,
        } => {
            let text = input.text.clone();
            let result = send_attachments(&link, &mut input, &attachments).await;
            Msg::AttachmentsSent {
                result,
                text,
                attachments,
            }
        }
        Request::Send(input) => Msg::Sent(link.call("chat.send", &*input).await),
        Request::Cancel(id) => void(link.call("chat.cancel", &json!({ "id": id })).await),
        Request::StopSubagent(id) => void(
            link.call("cli.subagents.cancel", &json!({ "runId": id }))
                .await,
        ),
        Request::StopGoal(id) => Msg::GoalStopped(
            link.call("chat.goal.stop", &json!({ "conversationId": id }))
                .await,
        ),
        Request::QueueList(id) => Msg::Queue(
            link.call("chat.queue.list", &json!({ "conversationId": id }))
                .await,
        ),
        Request::QueueAdd { id, text } => Msg::Queue(
            link.call(
                "chat.queue.add",
                &json!({ "conversationId": id, "text": text }),
            )
            .await,
        ),
        Request::QueueEdit { id, message, text } => Msg::Queue(
            link.call(
                "chat.queue.edit",
                &json!({ "conversationId": id, "messageId": message, "text": text }),
            )
            .await,
        ),
        Request::QueueRemove { id, message } => Msg::Queue(
            link.call(
                "chat.queue.remove",
                &json!({ "conversationId": id, "messageId": message }),
            )
            .await,
        ),
        Request::QueueSend(id) => Msg::Queue(
            link.call("chat.queue.flush", &json!({ "conversationId": id }))
                .await,
        ),
        Request::ContinueGoal(id) => Msg::Sent(
            link.call("chat.goal.continue", &json!({ "conversationId": id }))
                .await,
        ),
        Request::Approve { run, approval } => void(
            link.call(
                "runs.approve",
                &json!({ "id": run, "approvalId": approval, "always": false }),
            )
            .await,
        ),
        Request::Deny { run, approval } => void(
            link.call("runs.deny", &json!({ "id": run, "approvalId": approval }))
                .await,
        ),
        Request::AllowRule(input) => {
            Msg::RuleAdded(link.call("execPolicy.allowRule", &input).await)
        }
        Request::AnswerFolder {
            run,
            approval,
            answer,
        } => void(
            link.call(
                "folders.answer",
                &AnswerFolderRequestInput {
                    id: run,
                    approval_id: approval,
                    answer,
                },
            )
            .await,
        ),
        Request::Answer(input) => void(link.call("chat.answer", &input).await),
        Request::Skip(input) => void(link.call("chat.skip", &input).await),
        Request::Conversations => {
            Msg::Conversations(link.call("chat.conversations.list", &()).await)
        }
        Request::Load(id) => Msg::Loaded(
            link.call("chat.conversations.get", &json!({ "id": id }))
                .await,
        ),
        Request::Attach(id) => Msg::Attached(
            link.call("chat.attach", &json!({ "conversationId": id }))
                .await,
        ),
        Request::Context {
            conversation,
            provider,
            model,
        } => {
            let mut input = json!({ "providerId": provider, "modelId": model });
            if let Some(id) = conversation {
                input["conversationId"] = json!(id);
            }
            Msg::Context(link.call("chat.context", &input).await)
        }
        Request::Skills => Msg::Skills(link.call("skills.list", &()).await),
        Request::Processes(owner) => Msg::Processes(
            owner,
            link.call("shells.list", &json!({ "ownerId": owner })).await,
        ),
        Request::ProcessOutput { id, since } => Msg::ProcessOutput(
            link.call(
                "shells.output",
                &match since {
                    Some(since) => json!({ "id": id, "sinceSeq": since }),
                    None => json!({ "id": id }),
                },
            )
            .await,
        ),
        Request::KillProcess(id) => {
            Msg::ProcessKilled(link.call("shells.kill", &json!({ "id": id })).await)
        }
        Request::Usage(provider) => Msg::Usage(
            link.call("providers.usage", &json!({ "id": provider }))
                .await,
        ),
        Request::Servers => Msg::Servers(link.call("mcp.list", &()).await),
        Request::SetSkill { id, enabled } => Msg::SkillChanged(
            link.call(
                "skills.setEnabled",
                &json!({ "id": id, "enabled": enabled }),
            )
            .await,
        ),
        Request::StartServer(id) => {
            Msg::ServerChanged(link.call("mcp.start", &json!({ "id": id })).await)
        }
        Request::StopServer(id) => {
            Msg::ServerChanged(link.call("mcp.stop", &json!({ "id": id })).await)
        }
        Request::AddServer(input) => Msg::ServerAdded(link.call("cli.mcp.create", &*input).await),
        Request::InspectServer(name) => Msg::Inspected(inspect_server(&link, &name).await),
        Request::SignIn(id) => {
            #[cfg(windows)]
            zvs_pipe::allow_foreground();
            Msg::SignedIn(link.call("mcp.signIn", &json!({ "id": id })).await)
        }
        Request::CancelSignIn(id) => {
            let result: Result<McpServerDto, AppError> =
                link.call("mcp.cancelSignIn", &json!({ "id": id })).await;
            Msg::Answered(result.map(drop))
        }
        Request::SetPolicy(change) => {
            let result = set_policy(&link, &change).await;
            Msg::PolicySet(change, result)
        }
        Request::Shell(_)
        | Request::OpenDesktop(_)
        | Request::StopShell
        | Request::Copy(_)
        | Request::SaveShortcuts(_)
        | Request::SaveTheme(_) => Msg::Tick,
        Request::FocusVerification => {
            #[cfg(windows)]
            zvs_pipe::allow_foreground();
            link.focus_verification();
            Msg::Tick
        }
    }
}

fn run_shell(
    line: String,
    folder: String,
    control: ShellControl,
    signals: &mpsc::UnboundedSender<Signal>,
) {
    let output = signals.clone();
    let ended = signals.clone();
    tokio::spawn(async move {
        let end = shell::run(line, folder, control, move |text| {
            let _ = output.send(Signal::App(Box::new(Msg::ShellOutput(text))));
        })
        .await;
        let _ = ended.send(Signal::App(Box::new(Msg::ShellEnded(end))));
    });
}

pub fn selected_text(screen: &[String], selection: &Selection) -> String {
    let ((start_x, start_y), (end_x, end_y)) = selection.ordered();
    let mut lines = Vec::new();
    for row in start_y..=end_y {
        let Some(line) = screen.get(usize::from(row)) else {
            break;
        };
        let cells: Vec<char> = line.chars().collect();
        let from = if row == start_y {
            usize::from(start_x)
        } else {
            0
        };
        let to = if row == end_y {
            (usize::from(end_x) + 1).min(cells.len())
        } else {
            cells.len()
        };
        let piece: String = cells
            .get(from.min(to)..to)
            .unwrap_or_default()
            .iter()
            .collect();
        lines.push(piece.trim_end().to_owned());
    }
    lines.join("\n")
}

fn copy_to_clipboard(text: &str) -> io::Result<usize> {
    #[cfg(windows)]
    {
        zvs_pipe::copy_text(text)
    }

    #[cfg(not(windows))]
    {
        let _ = text;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Копирование в буфер обмена поддерживается только на Windows",
        ))
    }
}

fn screen_rows(buffer: &ratatui::buffer::Buffer) -> Vec<String> {
    let area = buffer.area;
    (area.top()..area.bottom())
        .map(|y| {
            (area.left()..area.right())
                .map(|x| buffer[(x, y)].symbol().chars().next().unwrap_or(' '))
                .collect()
        })
        .collect()
}

fn dispatch(
    link: Option<&Link>,
    requests: Vec<Request>,
    folder: &str,
    shell: &ShellControl,
    screen: &[String],
    signals: &mpsc::UnboundedSender<Signal>,
) {
    let mut remote = Vec::new();
    for request in requests {
        match request {
            Request::Shell(line) => run_shell(line, folder.to_owned(), shell.clone(), signals),
            Request::StopShell => shell.stop(),
            Request::OpenDesktop(id) => {
                let opened = launch::open_conversation(id);
                let _ = signals.send(Signal::App(Box::new(Msg::DesktopOpened(opened))));
            }
            Request::Copy(selection) => {
                let copied_count =
                    copy_to_clipboard(&selected_text(screen, &selection)).unwrap_or(0);
                let _ = signals.send(Signal::App(Box::new(Msg::Copied(Some(copied_count)))));
            }
            Request::SaveShortcuts(value) => {
                let saved = keymap_file::save(&value);
                let _ = signals.send(Signal::App(Box::new(Msg::ShortcutsSaved(saved))));
            }
            Request::SaveTheme(theme) => {
                let saved = theme_file::save(theme);
                let _ = signals.send(Signal::App(Box::new(Msg::ThemeSaved(saved))));
            }
            other => remote.push(other),
        }
    }
    let Some(link) = link else {
        return;
    };
    for request in remote {
        let link = link.clone();
        let folder = folder.to_owned();
        let signals = signals.clone();
        tokio::spawn(async move {
            let message = execute(link, request, folder).await;
            let _ = signals.send(Signal::App(Box::new(message)));
        });
    }
}

fn watch_theme(signals: mpsc::UnboundedSender<Signal>) {
    let Some(path) = theme_file::default_path() else {
        return;
    };
    tokio::spawn(async move {
        let mut seen = theme_file::modified(&path);
        let mut polls = tokio::time::interval(THEME_POLL);
        loop {
            polls.tick().await;
            let current = theme_file::modified(&path);
            if current == seen {
                continue;
            }
            seen = current;
            let theme = theme_file::load_from(&path);
            if signals
                .send(Signal::App(Box::new(Msg::ThemeChanged(theme))))
                .is_err()
            {
                break;
            }
        }
    });
}

fn forward(
    mut incoming: mpsc::UnboundedReceiver<Incoming>,
    signals: mpsc::UnboundedSender<Signal>,
) {
    tokio::spawn(async move {
        while let Some(message) = incoming.recv().await {
            let message = match message {
                Incoming::Event(event) => Msg::Event(Box::new(event)),
                Incoming::Notice(notice) => Msg::Notice(notice),
                Incoming::Closed => Msg::Closed,
            };
            if signals.send(Signal::App(Box::new(message))).is_err() {
                break;
            }
        }
    });
}

async fn session(
    terminal: &mut ratatui::DefaultTerminal,
    folder: String,
    resume: Option<zvs_contract::ConversationId>,
) -> std::io::Result<(Option<Exit>, Option<zvs_contract::ConversationId>)> {
    let (signals, mut received) = mpsc::unbounded_channel::<Signal>();
    {
        let signals = signals.clone();
        std::thread::spawn(move || read_terminal(&signals));
    }
    {
        let signals = signals.clone();
        tokio::spawn(async move {
            let mut ticks = tokio::time::interval(TICK);
            loop {
                ticks.tick().await;
                if signals.send(Signal::App(Box::new(Msg::Tick))).is_err() {
                    break;
                }
            }
        });
    }
    {
        let signals = signals.clone();
        tokio::spawn(async move {
            let status = signals.clone();
            let report = move |message: &'static str| {
                let _ = status.send(Signal::App(Box::new(Msg::Connecting(message))));
            };
            let signal = match launch::connect(report).await {
                Ok(connection) => Signal::Linked(connection),
                Err(message) => Signal::App(Box::new(Msg::Unreachable(message))),
            };
            let _ = signals.send(signal);
        });
    }

    let mut app = App::new(&folder);
    let size = terminal.size()?;
    app.viewport = (size.width, size.height);
    app.keymap = keymap_file::load();
    app.theme = theme_file::load();
    watch_theme(signals.clone());
    app.resume = resume;
    let mut link: Option<Link> = None;
    let shell = ShellControl::default();
    loop {
        let drawn = terminal.draw(|frame| ui::draw(frame, &app))?;
        let screen = screen_rows(drawn.buffer);
        let Some(first) = received.recv().await else {
            break;
        };
        let mut batch = vec![first];
        while let Ok(next) = received.try_recv() {
            batch.push(next);
        }
        for signal in batch {
            let requests = match signal {
                Signal::Linked((connected, incoming)) => {
                    forward(incoming, signals.clone());
                    link = Some(connected);
                    app.update(Msg::Connected)
                }
                Signal::App(message) => app.update(*message),
            };
            dispatch(
                link.as_ref(),
                requests,
                &app.folder,
                &shell,
                &screen,
                &signals,
            );
            if app.exit.is_some() {
                let conversation = app.conversation.as_ref().map(|known| known.id);
                return Ok((app.exit.take(), conversation));
            }
        }
    }
    Ok((None, app.conversation.as_ref().map(|known| known.id)))
}

async fn add_server(input: CreateMcpServerInput) -> i32 {
    let (link, _incoming) = match launch::connect(|message| eprintln!("{message}")).await {
        Ok(connection) => connection,
        Err(message) => {
            eprintln!("{message}");
            return 1;
        }
    };
    eprintln!("{} «{}»…", copy::MCP_ADDING, input.name);
    let created: McpServerDto = match link.call("cli.mcp.create", &input).await {
        Ok(server) => server,
        Err(error) => {
            eprintln!("{}", error.message);
            return 1;
        }
    };
    let id = json!({ "id": created.id });
    let started: McpServerDto = match link.call("mcp.start", &id).await {
        Ok(server) => server,
        Err(error) => {
            eprintln!("{}", error.message);
            return 1;
        }
    };
    let (server, prefix) = if started.oauth == McpOAuthState::Required {
        eprintln!(
            "{} «{}» {}… {}",
            copy::MCP_SIGNING_IN,
            started.name,
            copy::MCP_IN_BROWSER,
            copy::MCP_SIGN_IN_CANCEL
        );
        #[cfg(windows)]
        zvs_pipe::allow_foreground();
        let signing = link.call::<_, McpServerDto>("mcp.signIn", &id);
        let outcome = tokio::select! {
            result = signing => Some(result),
            _ = tokio::signal::ctrl_c() => None,
        };
        match outcome {
            Some(Ok(server)) => (server, Some(copy::MCP_SIGNED_IN)),
            Some(Err(error)) => {
                eprintln!("{}", error.message);
                return 1;
            }
            None => {
                let _ = link.call::<_, Value>("mcp.cancelSignIn", &id).await;
                eprintln!("{}", copy::MCP_SIGN_IN_CANCELLED);
                return 130;
            }
        }
    } else {
        (started, None)
    };
    let (tone, text) = crate::state::server_outcome(&server, prefix);
    if tone == Tone::Success {
        println!("{text}");
        0
    } else {
        eprintln!("{text}");
        1
    }
}

pub async fn run() -> i32 {
    let resume = match args::parse(std::env::args().skip(1)) {
        Ok(Args::Run { resume }) => resume,
        Ok(Args::McpAdd(input)) => return add_server(*input).await,
        Ok(Args::McpHelp) => {
            println!("{}", args::mcp_usage());
            return 0;
        }
        Ok(Args::Version) => {
            println!("{}", version::version_line());
            return 0;
        }
        Ok(Args::Help) => {
            println!("{}", args::usage());
            return 0;
        }
        Err(error) => {
            eprintln!("{}", args::report(&error));
            return 2;
        }
    };
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        eprintln!("{}", copy::NO_TERMINAL);
        return 2;
    }
    let folder = std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut terminal = ratatui::init();
    let _ = execute!(std::io::stdout(), EnableMouseCapture, EnableBracketedPaste);
    let outcome = session(&mut terminal, folder, resume).await;
    let _ = execute!(
        std::io::stdout(),
        DisableMouseCapture,
        DisableBracketedPaste
    );
    ratatui::restore();
    let (exit, conversation) = match outcome {
        Ok(parts) => parts,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };
    if let Some(id) = conversation {
        println!("{}: aistudio --resume {id}", copy::RESUME_HINT);
    }
    match exit {
        Some(Exit::Message(message)) => {
            eprintln!("{message}");
            1
        }
        _ => 0,
    }
}

async fn send_attachments(
    link: &Link,
    input: &mut zvs_contract::ChatSendInput,
    attachments: &[crate::state::attachments::Attachment],
) -> Result<zvs_contract::RunHandleDto, AppError> {
    use crate::state::attachments::Attachment;
    let paths: Vec<_> = attachments
        .iter()
        .filter_map(|a| match a {
            Attachment::Image { path, .. } => Some(path.clone()),
            _ => None,
        })
        .collect();
    if !paths.is_empty() {
        input.images = link
            .call(
                "cli.images.import",
                &zvs_contract::CliImageImportInput {
                    id: input.conversation_id,
                    paths,
                },
            )
            .await?;
    }
    let stores: Vec<_> = attachments
        .iter()
        .filter_map(|a| match a {
            Attachment::Store { id, .. } => Some(*id),
            _ => None,
        })
        .collect();
    if !stores.is_empty() {
        let detail: zvs_contract::ConversationDetailDto = link
            .call(
                "chat.conversations.get",
                &json!({ "id": input.conversation_id }),
            )
            .await?;
        let mut ids = detail.conversation.attached_store_ids;
        for id in stores {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        let _: zvs_contract::ConversationDto = link
            .call(
                "chat.conversations.updateStores",
                &json!({ "id": input.conversation_id, "attachedStoreIds": ids }),
            )
            .await?;
    }
    link.call("chat.send", &*input).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn attachments_import_then_merge_stores_before_sending_references() {
        use crate::state::attachments::Attachment;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let conversation_cases: Vec<Value> = serde_json::from_str(include_str!(
            "../../../packages/shared/fixtures/chat/ConversationDto.valid.json"
        ))
        .unwrap();
        let conversation = conversation_cases[1]["value"].clone();
        let conversation_id =
            zvs_contract::ConversationId::parse(conversation["id"].as_str().unwrap()).unwrap();
        let handles: Vec<Value> = serde_json::from_str(include_str!(
            "../../../packages/shared/fixtures/runs/RunHandleDto.valid.json"
        ))
        .unwrap();
        let handle = handles[0]["value"].clone();
        let image = json!({ "id":"0199aa11-8888-7111-8111-111111111111", "name":"image.png", "mimeType":"image/png", "sizeBytes":33 });
        let mut detail = conversation.clone();
        detail["messages"] = json!([]);
        let (client, server) = tokio::io::duplex(64 * 1024);
        let fixture = tokio::spawn(async move {
            let (reader, mut writer) = tokio::io::split(server);
            let mut lines = BufReader::new(reader).lines();
            lines.next_line().await.unwrap().unwrap();
            writer
                .write_all(
                    format!(
                        "{}\n",
                        json!({ "type":"welcome", "version":zvs_contract::CLI_PROTOCOL_VERSION })
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            let mut calls = Vec::new();
            for data in [json!([image]), detail, conversation, handle] {
                let call: Value =
                    serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                writer.write_all(format!("{}\n", json!({ "type":"result", "id":call["id"], "envelope": { "ok":true, "data":data } })).as_bytes()).await.unwrap();
                calls.push(call);
            }
            calls
        });
        let (link, _events) = Link::open(client).await.unwrap();
        let store =
            zvs_contract::VectorStoreId::parse("0199aa11-5555-7111-8111-222222222222").unwrap();
        let mut input = zvs_contract::ChatSendInput::new(conversation_id, "question");
        let attachments = [
            Attachment::Image {
                path: "C:/image.png".to_owned(),
                name: "image.png".to_owned(),
            },
            Attachment::Store {
                id: store,
                name: "docs".to_owned(),
            },
        ];
        send_attachments(&link, &mut input, &attachments)
            .await
            .unwrap();
        let calls = fixture.await.unwrap();
        let channels: Vec<_> = calls
            .iter()
            .map(|c| c["channel"].as_str().unwrap())
            .collect();
        assert_eq!(
            channels,
            [
                "cli.images.import",
                "chat.conversations.get",
                "chat.conversations.updateStores",
                "chat.send"
            ]
        );
        assert_eq!(
            calls[2]["payload"]["attachedStoreIds"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(calls[3]["payload"]["images"][0]["name"], "image.png");
        assert!(calls[3]["payload"]["images"][0].get("path").is_none());
    }

    fn pressed(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn press(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn release(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new_with_kind(
            code,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ))
    }

    #[test]
    fn a_server_is_found_by_name_namespace_or_id() {
        let cases: Vec<Value> = serde_json::from_str(include_str!(
            "../../../packages/shared/fixtures/mcp/McpServerDto.valid.json"
        ))
        .expect("fixture");
        let mut server: McpServerDto =
            serde_json::from_value(cases[0]["value"].clone()).expect("server");
        server.name = "Qdrant Search".to_owned();
        let servers = vec![server.clone()];
        assert_eq!(find_server(&servers, "qdrant search"), Some(&server));
        assert_eq!(find_server(&servers, "qdrant"), Some(&server));
        assert_eq!(find_server(&servers, &server.id.to_string()), Some(&server));
        assert_eq!(find_server(&servers, "other"), None);
    }

    #[test]
    fn a_selection_copies_the_covered_cells_across_rows() {
        let screen = vec![
            "  hello world".to_owned(),
            "second line   ".to_owned(),
            "third".to_owned(),
        ];
        let across = Selection {
            anchor: (8, 0),
            cursor: (5, 1),
        };
        assert_eq!(selected_text(&screen, &across), "world\nsecond");
        let backwards = Selection {
            anchor: (5, 1),
            cursor: (8, 0),
        };
        assert_eq!(selected_text(&screen, &backwards), "world\nsecond");
        let tail = Selection {
            anchor: (0, 2),
            cursor: (40, 2),
        };
        assert_eq!(selected_text(&screen, &tail), "third");
    }

    #[test]
    fn an_enter_delivered_with_its_release_still_submits() {
        let batch = [press(KeyCode::Enter), release(KeyCode::Enter)];
        assert_eq!(translate(&batch), [Msg::Key(Key::Enter)]);
        let lf = [Event::Key(KeyEvent::new(
            KeyCode::Char('j'),
            KeyModifiers::CONTROL,
        ))];
        assert_eq!(translate(&lf), [Msg::Key(Key::Enter)]);
        assert_eq!(
            translate(&[press(KeyCode::Char('\r'))]),
            [Msg::Key(Key::Enter)]
        );
    }

    #[test]
    fn an_enter_followed_by_typed_text_in_one_batch_is_a_pasted_newline() {
        let batch = [
            press(KeyCode::Char('a')),
            press(KeyCode::Enter),
            release(KeyCode::Enter),
            press(KeyCode::Char('b')),
            press(KeyCode::Enter),
        ];
        assert_eq!(
            translate(&batch),
            [
                Msg::Key(Key::Char('a')),
                Msg::Key(Key::Newline),
                Msg::Key(Key::Char('b')),
                Msg::Key(Key::Enter),
            ]
        );
    }

    #[test]
    fn enter_submits_unless_shifted_or_part_of_a_paste() {
        assert_eq!(
            key(pressed(KeyCode::Enter, KeyModifiers::NONE), false),
            Some(Key::Enter)
        );
        assert_eq!(
            key(pressed(KeyCode::Enter, KeyModifiers::SHIFT), false),
            Some(Key::Newline)
        );
        assert_eq!(
            key(pressed(KeyCode::Enter, KeyModifiers::NONE), true),
            Some(Key::Newline)
        );
    }

    #[test]
    fn control_alt_and_function_keys_become_chords() {
        assert_eq!(
            key(pressed(KeyCode::Char('c'), KeyModifiers::CONTROL), false),
            Some(Key::Interrupt)
        );
        assert_eq!(
            key(pressed(KeyCode::Char('O'), KeyModifiers::CONTROL), false),
            Some(Key::Chord(Chord::ctrl('o')))
        );
        assert_eq!(
            key(pressed(KeyCode::Char('ы'), KeyModifiers::ALT), false),
            Some(Key::Chord(Chord::alt('s')))
        );
        assert_eq!(
            key(pressed(KeyCode::F(1), KeyModifiers::NONE), false),
            Some(Key::Chord(Chord::function(1)))
        );
        assert_eq!(
            key(
                pressed(
                    KeyCode::Char('@'),
                    KeyModifiers::CONTROL | KeyModifiers::ALT
                ),
                false
            ),
            Some(Key::Char('@'))
        );
        assert_eq!(
            key(pressed(KeyCode::Enter, KeyModifiers::ALT), false),
            Some(Key::Chord(Chord::alt_key(ChordKey::Enter)))
        );
        assert_eq!(
            key(pressed(KeyCode::Enter, KeyModifiers::SHIFT), false),
            Some(Key::Newline)
        );
        assert_eq!(
            key(pressed(KeyCode::Enter, KeyModifiers::ALT), true),
            Some(Key::Newline)
        );
        assert_eq!(
            key(pressed(KeyCode::Up, KeyModifiers::ALT), false),
            Some(Key::Chord(Chord::alt_key(ChordKey::Up)))
        );
        assert_eq!(
            key(pressed(KeyCode::Down, KeyModifiers::NONE), false),
            Some(Key::Down)
        );
        assert_eq!(
            key(pressed(KeyCode::Char('ж'), KeyModifiers::NONE), false),
            Some(Key::Char('ж'))
        );
    }
}
