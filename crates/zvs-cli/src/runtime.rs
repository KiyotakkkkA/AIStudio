use std::io::IsTerminal;
use std::time::Duration;

use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind,
};
use ratatui::crossterm::execute;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use zvs_contract::{
    AnswerFolderRequestInput, AppError, BindFolderInput, CliOpenInput, ProviderDto,
    ProviderSummaryDto,
};

use crate::copy;
use crate::launch::{self, Connection};
use crate::link::{Incoming, Link};
use crate::state::{App, Exit, Key, Msg, Request};
use crate::ui;

const TICK: Duration = Duration::from_millis(100);
const WHEEL_STEP: isize = 3;

enum Signal {
    App(Box<Msg>),
    Linked(Connection),
}

pub fn key(event: KeyEvent, more_pending: bool) -> Option<Key> {
    let control = event.modifiers.contains(KeyModifiers::CONTROL);
    let shifted = event
        .modifiers
        .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT);
    Some(match event.code {
        KeyCode::Char(value) if control => match value.to_ascii_lowercase() {
            'c' | 'd' => Key::Interrupt,
            'o' => Key::Details,
            'j' => Key::Newline,
            'u' => Key::ClearLine,
            _ => return None,
        },
        KeyCode::Char(value) => Key::Char(value),
        KeyCode::Enter if shifted || more_pending => Key::Newline,
        KeyCode::Enter => Key::Enter,
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

fn read_terminal(signals: &mpsc::UnboundedSender<Signal>) {
    loop {
        let Ok(next) = event::read() else {
            return;
        };
        let message = match next {
            Event::Key(pressed) if pressed.kind != KeyEventKind::Release => {
                let more = event::poll(Duration::ZERO).unwrap_or(false);
                key(pressed, more).map(Msg::Key)
            }
            Event::Paste(text) => Some(Msg::Paste(text)),
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => Some(Msg::Scroll(WHEEL_STEP)),
                MouseEventKind::ScrollDown => Some(Msg::Scroll(-WHEEL_STEP)),
                _ => None,
            },
            Event::Resize(..) => Some(Msg::Tick),
            _ => None,
        };
        if let Some(message) = message
            && signals.send(Signal::App(Box::new(message))).is_err()
        {
            return;
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
    }
}

fn dispatch(
    link: Option<&Link>,
    requests: Vec<Request>,
    folder: &str,
    signals: &mpsc::UnboundedSender<Signal>,
) {
    let Some(link) = link else {
        return;
    };
    for request in requests {
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
) -> std::io::Result<Option<Exit>> {
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
    let mut link: Option<Link> = None;
    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;
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
            dispatch(link.as_ref(), requests, &app.folder, &signals);
            if app.exit.is_some() {
                return Ok(app.exit.take());
            }
        }
    }
    Ok(None)
}

pub async fn run() -> i32 {
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        eprintln!("{}", copy::NO_TERMINAL);
        return 2;
    }
    let folder = std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut terminal = ratatui::init();
    let _ = execute!(std::io::stdout(), EnableMouseCapture, EnableBracketedPaste);
    let outcome = session(&mut terminal, folder).await;
    let _ = execute!(
        std::io::stdout(),
        DisableMouseCapture,
        DisableBracketedPaste
    );
    ratatui::restore();
    match outcome {
        Ok(Some(Exit::Message(message))) => {
            eprintln!("{message}");
            1
        }
        Ok(_) => 0,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pressed(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
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
    fn control_shortcuts_map_to_actions() {
        assert_eq!(
            key(pressed(KeyCode::Char('c'), KeyModifiers::CONTROL), false),
            Some(Key::Interrupt)
        );
        assert_eq!(
            key(pressed(KeyCode::Char('O'), KeyModifiers::CONTROL), false),
            Some(Key::Details)
        );
        assert_eq!(
            key(pressed(KeyCode::Char('x'), KeyModifiers::CONTROL), false),
            None
        );
        assert_eq!(
            key(pressed(KeyCode::Char('ж'), KeyModifiers::NONE), false),
            Some(Key::Char('ж'))
        );
    }
}
