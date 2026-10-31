use std::io::IsTerminal;
use std::time::Duration;

use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::crossterm::execute;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use zvs_contract::{
    AnswerFolderRequestInput, AppError, BindFolderInput, CliOpenInput, ProviderDto,
    ProviderSummaryDto,
};

use crate::args::{self, Args};
use crate::copy;
use crate::keymap_file;
use crate::launch::{self, Connection};
use crate::link::{Incoming, Link};
use crate::shell::{self, ShellControl};
use crate::state::shortcuts::Chord;
use crate::state::{App, Exit, Key, Msg, Pointer, Request, Selection};
use crate::ui;
use crate::version;

const TICK: Duration = Duration::from_millis(100);
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
            Event::Resize(..) => Some(Msg::Tick),
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

pub async fn execute(link: Link, request: Request, folder: String) -> Msg {
    match request {
        Request::Open => Msg::Opened(link.call("cli.open", &CliOpenInput { cwd: folder }).await),
        Request::Providers => Msg::Providers(providers(&link).await),
        Request::LinkCopilot => {
            let result: Result<zvs_contract::AccountLinkResult, AppError> =
                link.call("cli.copilot.login", &()).await;
            Msg::CopilotLinked(match result {
                Ok(zvs_contract::AccountLinkResult::Linked { .. }) => providers(&link).await,
                Ok(
                    zvs_contract::AccountLinkResult::Cancelled { detail }
                    | zvs_contract::AccountLinkResult::Timeout { detail },
                ) => Err(AppError::new(
                    zvs_contract::AppErrorCode::RunCancelled,
                    detail,
                )),
                Err(error) => Err(error),
            })
        }
        Request::CancelCopilotLink => {
            let result: Result<zvs_contract::AccountsCancelLinkOutput, AppError> =
                link.call("cli.copilot.cancelLogin", &()).await;
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
        Request::Send(input) => Msg::Sent(link.call("chat.send", &*input).await),
        Request::Cancel(id) => void(link.call("chat.cancel", &json!({ "id": id })).await),
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
        Request::SetPolicy(policy) => Msg::PolicySet(link.call("execPolicy.set", &policy).await),
        Request::Shell(_) | Request::StopShell | Request::Copy(_) | Request::SaveShortcuts(_) => {
            Msg::Tick
        }
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

fn copy_to_clipboard(text: &str) -> bool {
    #[cfg(windows)]
    {
        zvs_pipe::copy_text(text).is_ok()
    }
    #[cfg(not(windows))]
    {
        let _ = text;
        false
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
            Request::Copy(selection) => {
                let copied = copy_to_clipboard(&selected_text(screen, &selection));
                let _ = signals.send(Signal::App(Box::new(Msg::Copied(copied))));
            }
            Request::SaveShortcuts(value) => {
                let saved = keymap_file::save(&value);
                let _ = signals.send(Signal::App(Box::new(Msg::ShortcutsSaved(saved))));
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
    app.keymap = keymap_file::load();
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

pub async fn run() -> i32 {
    let resume = match args::parse(std::env::args().skip(1)) {
        Ok(Args::Run { resume }) => resume,
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

#[cfg(test)]
mod tests {
    use super::*;

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
            Some(Key::Newline)
        );
        assert_eq!(
            key(pressed(KeyCode::Char('ж'), KeyModifiers::NONE), false),
            Some(Key::Char('ж'))
        );
    }
}
