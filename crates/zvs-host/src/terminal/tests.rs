use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use zvs_contract::{
    AppErrorCode, BindFolderInput, ConversationId, CreateConversationInput, FolderGrantScope,
    HostEventDraft, ProviderCapability, ProviderId, ProviderKind, StreamId, TERMINAL_SESSIONS_MAX,
    TerminalCreateInput, TerminalId, TerminalSessionDto, TerminalShell, TerminalStatus,
    TerminalStatusDto,
};

use super::service::{decode, trim_scrollback};
use super::{NO_SHELL, SESSION_EXITED, SESSIONS_LIMIT, TerminalService};
use crate::data::Repositories;
use crate::data::repositories::{ModelDraft, ProviderDraft};
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::services::SystemService;
use crate::testing::{FakeProcess, TestServices, temporary_directory, test_services};

struct Host {
    services: TestServices,
    provider_id: ProviderId,
}

impl Host {
    fn new() -> Self {
        let services = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        let provider_id = services
            .database
            .with_connection(|connection| {
                let repositories = Repositories::new(connection);
                let provider = repositories.providers.create(&ProviderDraft::new(
                    ProviderKind::Ollama,
                    "Fake",
                    "http://localhost:11434",
                    vec![ProviderCapability::Text],
                    1,
                ))?;
                repositories.models.replace_for_provider(
                    &provider.id,
                    &[ModelDraft::new("fake-model", "Fake", 1)],
                    1,
                )?;
                Ok(provider.id)
            })
            .expect("seeds a provider");
        Self {
            services,
            provider_id: ProviderId::parse(&provider_id).expect("a provider id"),
        }
    }

    fn terminals(&self) -> &TerminalService {
        &self.services.terminals
    }

    fn conversation(&self) -> ConversationId {
        self.services
            .chat
            .create(&CreateConversationInput::new(
                self.provider_id,
                "fake-model".to_owned(),
            ))
            .expect("creates")
            .id
    }

    fn create(&self, conversation: ConversationId) -> TerminalSessionDto {
        self.terminals()
            .create(&input(conversation, None))
            .expect("creates a session")
    }

    fn output(&self, stream: StreamId) -> String {
        self.events(stream)
            .into_iter()
            .filter_map(|body| match body {
                HostEventDraft::Token { delta, .. } => Some(delta),
                _ => None,
            })
            .collect()
    }

    fn statuses(&self, stream: StreamId) -> Vec<TerminalStatusDto> {
        self.events(stream)
            .into_iter()
            .filter_map(|body| match body {
                HostEventDraft::Step { step } => {
                    serde_json::from_value(serde_json::Value::Object(step)).ok()
                }
                _ => None,
            })
            .collect()
    }

    fn ended(&self, stream: StreamId) -> bool {
        self.events(stream)
            .iter()
            .any(|body| matches!(body, HostEventDraft::End { .. }))
    }

    fn events(&self, stream: StreamId) -> Vec<HostEventDraft> {
        self.services
            .run_events
            .events()
            .into_iter()
            .filter(|event| event.stream_id == stream)
            .map(|event| event.body)
            .collect()
    }
}

fn input(conversation: ConversationId, shell: Option<TerminalShell>) -> TerminalCreateInput {
    TerminalCreateInput {
        conversation_id: conversation,
        shell,
        cols: 100,
        rows: 30,
    }
}

async fn until(what: &str, check: impl Fn() -> bool) {
    for _ in 0..500 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("{what} never happened");
}

fn process(host: &Host) -> FakeProcess {
    host.services.pty.last()
}

#[test]
fn a_split_utf8_sequence_waits_for_its_tail() {
    let bytes = "привет".as_bytes();
    let mut pending = bytes[..3].to_vec();
    assert_eq!(decode(&mut pending), "п");
    assert_eq!(pending, bytes[2..3]);
    pending.extend_from_slice(&bytes[3..]);
    assert_eq!(decode(&mut pending), "ривет");
    assert!(pending.is_empty());
}

#[test]
fn invalid_bytes_become_replacement_characters() {
    let mut pending = vec![b'a', 0xff, b'b'];
    assert_eq!(decode(&mut pending), "a\u{fffd}b");
}

#[test]
fn the_scrollback_keeps_the_newest_whole_lines() {
    let mut scrollback = "first line\nsecond line\nthird".to_owned();
    trim_scrollback(&mut scrollback, 16);
    assert_eq!(scrollback, "third");
    let mut short = "ok".to_owned();
    trim_scrollback(&mut short, 16);
    assert_eq!(short, "ok");
}

#[test]
fn the_first_available_shell_is_the_default() {
    let host = Host::new();
    assert_eq!(
        host.terminals().shells(),
        [TerminalShell::PowerShell, TerminalShell::Cmd]
    );
    let session = host.create(host.conversation());
    assert_eq!(session.shell, TerminalShell::PowerShell);
    assert_eq!(session.title, "Windows PowerShell");
    assert_eq!(session.status, TerminalStatus::Running);
    assert_eq!((session.cols, session.rows), (100, 30));
    let spec = process(&host).spec;
    assert_eq!(spec.program, Path::new("powershell.exe"));
    assert_eq!((spec.cols, spec.rows), (100, 30));
    assert!(
        spec.env
            .contains(&("TERM".to_owned(), "xterm-256color".to_owned()))
    );
}

#[test]
fn a_missing_shell_is_refused() {
    let host = Host::new();
    let error = host
        .terminals()
        .create(&input(host.conversation(), Some(TerminalShell::Wsl)))
        .expect_err("wsl is not installed");
    assert_eq!(error.code, AppErrorCode::NotFound);
    assert_eq!(error.message, NO_SHELL);
}

#[test]
fn a_session_starts_in_the_working_folder_of_its_dialog() {
    let host = Host::new();
    let folder = temporary_directory("terminal-cwd-");
    let conversation = host.conversation();
    host.services
        .chat
        .bind_folder(&BindFolderInput {
            id: conversation,
            path: folder.path().to_string_lossy().into_owned(),
            grant: Some(FolderGrantScope::Always),
        })
        .expect("binds");
    let session = host.create(conversation);
    let spec = process(&host).spec;
    assert_eq!(
        spec.cwd.canonicalize().expect("exists"),
        folder.path().canonicalize().expect("exists")
    );
    assert_eq!(session.cwd, spec.cwd.to_string_lossy());
}

#[test]
fn without_a_working_folder_a_session_starts_at_home() {
    let host = Host::new();
    host.create(host.conversation());
    assert_eq!(
        process(&host).spec.cwd,
        host.services.workspace.join("home")
    );
}

#[test]
fn an_unknown_dialog_is_not_found() {
    let host = Host::new();
    let error = host
        .terminals()
        .create(&input(ConversationId::new(uuid::Uuid::now_v7()), None))
        .expect_err("no such dialog");
    assert_eq!(error.code, AppErrorCode::NotFound);
}

#[test]
fn titles_are_numbered_within_a_dialog() {
    let host = Host::new();
    let conversation = host.conversation();
    let titles: Vec<String> = (0..3).map(|_| host.create(conversation).title).collect();
    assert_eq!(
        titles,
        [
            "Windows PowerShell",
            "Windows PowerShell 2",
            "Windows PowerShell 3"
        ]
    );
    let other = host.create(host.conversation());
    assert_eq!(other.title, "Windows PowerShell");
}

#[test]
fn a_dialog_has_a_session_limit() {
    let host = Host::new();
    let conversation = host.conversation();
    for _ in 0..TERMINAL_SESSIONS_MAX {
        host.create(conversation);
    }
    let error = host
        .terminals()
        .create(&input(conversation, None))
        .expect_err("over the limit");
    assert_eq!(error.code, AppErrorCode::Conflict);
    assert_eq!(error.message, SESSIONS_LIMIT);
}

#[tokio::test]
async fn output_streams_as_tokens_and_fills_the_scrollback() {
    let host = Host::new();
    let session = host.create(host.conversation());
    let child = process(&host);
    child.print("PS> ".as_bytes());
    child.print(&"готово\r\n".as_bytes()[..3]);
    child.print(&"готово\r\n".as_bytes()[3..]);
    until("the output", || {
        host.output(session.stream_id) == "PS> готово\r\n"
    })
    .await;
    let attached = host.terminals().attach(&session.id).expect("attaches");
    assert_eq!(attached.scrollback, "PS> готово\r\n");
    assert_eq!(
        attached.next_seq,
        u64::try_from(host.events(session.stream_id).len()).expect("fits")
    );
}

#[tokio::test]
async fn input_and_resizes_reach_the_process() {
    let host = Host::new();
    let session = host.create(host.conversation());
    host.terminals()
        .write(&session.id, "dir\r")
        .expect("writes");
    host.terminals()
        .write(&session.id, "\u{3}")
        .expect("interrupts");
    host.terminals()
        .resize(&session.id, 140, 40)
        .expect("resizes");
    let child = process(&host);
    assert_eq!(child.input(), "dir\r\u{3}");
    assert_eq!(child.sizes(), [(140, 40)]);
    let listed = host.terminals().list(&session.conversation_id);
    assert_eq!((listed[0].cols, listed[0].rows), (140, 40));
}

#[tokio::test]
async fn an_exit_is_reported_after_the_last_output() {
    let host = Host::new();
    let session = host.create(host.conversation());
    let child = process(&host);
    child.print(b"bye\r\n");
    child.exit(3);
    until("the exit", || !host.statuses(session.stream_id).is_empty()).await;
    let events = host.events(session.stream_id);
    assert!(matches!(events.first(), Some(HostEventDraft::Token { .. })));
    let status = &host.statuses(session.stream_id)[0];
    assert_eq!(status.status, TerminalStatus::Exited);
    assert_eq!(status.exit_code, Some(3));
    let listed = &host.terminals().list(&session.conversation_id)[0];
    assert_eq!(listed.status, TerminalStatus::Exited);
    let error = host
        .terminals()
        .write(&session.id, "x")
        .expect_err("the shell is gone");
    assert_eq!(error.message, SESSION_EXITED);
}

#[tokio::test]
async fn a_restart_kills_the_old_shell_and_clears_the_scrollback() {
    let host = Host::new();
    let session = host.create(host.conversation());
    let first = process(&host);
    first.print(b"old\r\n");
    until("the output", || !host.output(session.stream_id).is_empty()).await;
    let restarted = host.terminals().restart(&session.id).expect("restarts");
    assert!(first.killed());
    assert_eq!(restarted.status, TerminalStatus::Running);
    assert_eq!(host.services.pty.processes().len(), 2);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let statuses = host.statuses(session.stream_id);
    assert_eq!(statuses.len(), 1, "the old shell's exit is not reported");
    assert_eq!(statuses[0].status, TerminalStatus::Running);
    assert_eq!(
        host.terminals()
            .attach(&session.id)
            .expect("attaches")
            .scrollback,
        ""
    );
    process(&host).print(b"new");
    until("the new output", || {
        host.terminals()
            .attach(&session.id)
            .expect("attaches")
            .scrollback
            == "new"
    })
    .await;
}

#[tokio::test]
async fn closing_kills_the_shell_and_ends_the_stream() {
    let host = Host::new();
    let session = host.create(host.conversation());
    host.terminals().close(&session.id).expect("closes");
    assert!(process(&host).killed());
    assert!(host.ended(session.stream_id));
    assert!(host.terminals().list(&session.conversation_id).is_empty());
    let error = host.terminals().attach(&session.id).expect_err("gone");
    assert_eq!(error.code, AppErrorCode::NotFound);
}

#[tokio::test]
async fn removing_a_dialog_closes_only_its_sessions() {
    let host = Host::new();
    let removed = host.conversation();
    let kept = host.conversation();
    let doomed = host.create(removed);
    let survivor = host.create(kept);
    host.terminals().close_conversation(&removed);
    assert!(host.ended(doomed.stream_id));
    assert!(!host.ended(survivor.stream_id));
    assert_eq!(host.terminals().open(), 1);
}

#[tokio::test]
async fn disposing_closes_every_session() {
    let host = Host::new();
    let first = host.create(host.conversation());
    let second = host.create(host.conversation());
    host.terminals().dispose();
    assert!(host.ended(first.stream_id) && host.ended(second.stream_id));
    assert!(
        host.services
            .pty
            .processes()
            .iter()
            .all(FakeProcess::killed)
    );
    assert_eq!(host.terminals().open(), 0);
}

#[test]
fn a_failed_spawn_leaves_no_session() {
    let host = Host::new();
    host.services.pty.fail(true);
    let conversation = host.conversation();
    let error = host
        .terminals()
        .create(&input(conversation, None))
        .expect_err("spawn fails");
    assert_eq!(error.code, AppErrorCode::Unknown);
    assert!(host.terminals().list(&conversation).is_empty());
}

#[test]
fn renaming_trims_the_title() {
    let host = Host::new();
    let session = host.create(host.conversation());
    let renamed = host
        .terminals()
        .rename(&session.id, "  сервер  ")
        .expect("renames");
    assert_eq!(renamed.title, "сервер");
    let unknown = TerminalId::new(uuid::Uuid::now_v7());
    assert!(host.terminals().rename(&unknown, "x").is_err());
}

#[tokio::test]
async fn the_startup_cursor_query_is_answered_and_hidden() {
    let host = Host::new();
    let session = host.create(host.conversation());
    let child = process(&host);
    child.print(b"\x1b[6nPS> ");
    until("the prompt", || host.output(session.stream_id) == "PS> ").await;
    assert_eq!(child.input(), "\u{1b}[1;1R");
    child.print(b"\x1b[6n");
    until("the second query", || {
        host.output(session.stream_id).ends_with("\u{1b}[6n")
    })
    .await;
    assert_eq!(
        child.input(),
        "\u{1b}[1;1R",
        "only the first query is answered"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn a_real_console_runs_a_command_and_reports_its_exit() {
    use super::{NativePty, SystemShells, TerminalServiceOptions};
    use crate::testing::MemorySink;

    let host = Host::new();
    let events = MemorySink::shared();
    let bus = EventBus::default();
    bus.attach(events.clone());
    let terminals = TerminalService::new(TerminalServiceOptions {
        folders: host.services.folders.clone(),
        events: bus,
        clock: Arc::new(FakeClock::default()),
        spawner: Arc::new(NativePty),
        shells: Arc::new(SystemShells::from_env()),
        launcher: None,
        home: std::env::temp_dir(),
    });
    let session = terminals
        .create(&input(host.conversation(), Some(TerminalShell::Cmd)))
        .expect("opens cmd");
    let output = || -> String {
        events
            .events()
            .into_iter()
            .filter_map(|event| match event.body {
                HostEventDraft::Token { delta, .. } => Some(delta),
                _ => None,
            })
            .collect()
    };
    let status = || terminals.list(&session.conversation_id)[0].clone();
    terminals
        .write(&session.id, "echo zvs-%COMSPEC:~0,0%terminal\r")
        .expect("types");
    for _ in 0..1000 {
        if output().contains("zvs-terminal") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(output().contains("zvs-terminal"), "{:?}", output());
    terminals.write(&session.id, "exit 7\r").expect("types");
    for _ in 0..1000 {
        if status().status == TerminalStatus::Exited {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(status().status, TerminalStatus::Exited);
    assert_eq!(status().exit_code, Some(7));
    terminals.dispose();
}
