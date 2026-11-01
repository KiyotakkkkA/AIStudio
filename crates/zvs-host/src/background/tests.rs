use std::collections::BTreeMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use zvs_contract::{
    AppErrorCode, ArtifactDto, BackgroundShellStatus, HostEventDraft, SHELL_STEP_DOMAIN,
};

use super::service::{
    BackgroundShellOptions, BackgroundShells, OWNER_LIMIT, SHELL_MISSING, ShellLimits, ShellStart,
    TOTAL_LIMIT,
};
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::testing::MemorySink;
use crate::toolchain::LaunchSpec;

const STUB: &str = "background::tests::shell_stub";
const STUB_MODE: &str = "ZVS_SHELL_STUB";
const OWNER: &str = "0199aa11-1111-7111-8111-111111111111";
const OTHER: &str = "0199aa11-2222-7111-8111-111111111111";
const NOT_STARTED: &str = "Не удалось запустить команду";

#[test]
#[ignore = "runs only as the child process of a background shell test"]
fn shell_stub() {
    let mut stdout = std::io::stdout();
    match std::env::var(STUB_MODE).unwrap_or_default().as_str() {
        "lines" => {
            let _ = writeln!(stdout, "one\ntwo\nthree");
            let _ = stdout.flush();
            std::process::exit(2);
        }
        "flood" => {
            let line = "x".repeat(99);
            for _ in 0..200 {
                let _ = writeln!(stdout, "{line}");
            }
        }
        "hang" => {
            let _ = writeln!(stdout, "started");
            let _ = stdout.flush();
            std::thread::sleep(Duration::from_secs(120));
        }
        _ => {}
    }
}

fn spec(mode: &str) -> LaunchSpec {
    LaunchSpec {
        command: std::env::current_exe()
            .expect("the test binary")
            .to_string_lossy()
            .into_owned(),
        args: [
            STUB,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ]
        .map(str::to_owned)
        .to_vec(),
        env: BTreeMap::from([(STUB_MODE.to_owned(), mode.to_owned())]),
        cwd: Some(std::env::temp_dir()),
    }
}

fn argv(mode: &str) -> Vec<String> {
    vec!["stub".to_owned(), mode.to_owned()]
}

struct Fixture {
    shells: BackgroundShells,
    events: Arc<MemorySink>,
}

fn fixture(limits: ShellLimits) -> Fixture {
    let events = MemorySink::shared();
    let bus = EventBus::default();
    bus.attach(events.clone());
    Fixture {
        shells: BackgroundShells::new(BackgroundShellOptions {
            limits,
            ..BackgroundShellOptions::new(bus, Arc::new(FakeClock::default()))
        }),
        events,
    }
}

async fn settled(fixture: &Fixture, id: &zvs_contract::BackgroundShellId) -> BackgroundShellStatus {
    for _ in 0..600 {
        let status = fixture.shells.get(None, id).expect("the shell").status;
        if !status.running() {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the shell did not end in time");
}

async fn printed(fixture: &Fixture, id: &zvs_contract::BackgroundShellId, text: &str) {
    for _ in 0..600 {
        let output = fixture.shells.output(None, id, None).expect("output");
        if output.text.contains(text) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the shell never printed {text}");
}

fn statuses(fixture: &Fixture) -> Vec<String> {
    fixture
        .events
        .events()
        .into_iter()
        .filter_map(|event| match event.body {
            HostEventDraft::Step { step }
                if step.get("domain").and_then(|v| v.as_str()) == Some(SHELL_STEP_DOMAIN) =>
            {
                step.get("status")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_background_shell_keeps_its_output_and_reports_its_exit_code() {
    let fixture = fixture(ShellLimits::default());
    let shell = fixture
        .shells
        .start(
            OWNER,
            &spec("lines"),
            &argv("lines"),
            NOT_STARTED,
            ShellStart::default(),
        )
        .expect("starts");
    assert_eq!(shell.status, BackgroundShellStatus::Running);
    assert_eq!(shell.ended_at, None);
    assert_eq!(
        settled(&fixture, &shell.id).await,
        BackgroundShellStatus::Exited
    );
    let output = fixture
        .shells
        .output(None, &shell.id, None)
        .expect("output");
    assert!(output.text.contains("one\ntwo\nthree"));
    assert_eq!(output.shell.exit_code, Some(2));
    assert!(output.shell.ended_at.is_some());
    assert!(!output.truncated);
    assert_eq!(statuses(&fixture), ["running", "exited"]);
    let tokens = fixture
        .events
        .events()
        .into_iter()
        .filter(|event| matches!(event.body, HostEventDraft::Token { .. }))
        .count();
    assert!(tokens > 0);
    let emitted = fixture
        .events
        .events()
        .into_iter()
        .filter(|event| event.stream_id == output.shell.stream_id)
        .count();
    assert_eq!(output.event_seq, emitted as u64);
}

#[tokio::test]
async fn stopping_a_shell_kills_it_and_reports_stopped() {
    let fixture = fixture(ShellLimits::default());
    let shell = fixture
        .shells
        .start(
            OWNER,
            &spec("hang"),
            &argv("hang"),
            NOT_STARTED,
            ShellStart::default(),
        )
        .expect("starts");
    printed(&fixture, &shell.id, "started").await;
    let stopped = tokio::time::timeout(
        Duration::from_secs(30),
        fixture.shells.kill(Some(OWNER), &shell.id),
    )
    .await
    .expect("stops in time")
    .expect("stops");
    assert_eq!(stopped.status, BackgroundShellStatus::Stopped);
    assert_eq!(stopped.exit_code, None);
    assert!(stopped.ended_at.is_some());
}

#[tokio::test]
async fn a_shell_past_its_lifetime_is_stopped() {
    let fixture = fixture(ShellLimits {
        lifetime: Duration::from_secs(1),
        ..ShellLimits::default()
    });
    let shell = fixture
        .shells
        .start(
            OWNER,
            &spec("hang"),
            &argv("hang"),
            NOT_STARTED,
            ShellStart::default(),
        )
        .expect("starts");
    assert_eq!(
        settled(&fixture, &shell.id).await,
        BackgroundShellStatus::TimedOut
    );
}

#[tokio::test]
async fn the_owner_and_total_caps_count_running_shells_only() {
    let fixture = fixture(ShellLimits {
        per_owner: 1,
        total: 2,
        ..ShellLimits::default()
    });
    let first = fixture
        .shells
        .start(
            OWNER,
            &spec("hang"),
            &argv("hang"),
            NOT_STARTED,
            ShellStart::default(),
        )
        .expect("starts");
    let error = fixture
        .shells
        .start(
            OWNER,
            &spec("hang"),
            &argv("hang"),
            NOT_STARTED,
            ShellStart::default(),
        )
        .expect_err("over the owner cap");
    assert_eq!(error.code, AppErrorCode::Conflict);
    assert_eq!(error.message, OWNER_LIMIT);
    fixture
        .shells
        .start(
            OTHER,
            &spec("hang"),
            &argv("hang"),
            NOT_STARTED,
            ShellStart::default(),
        )
        .expect("another owner starts");
    let error = fixture
        .shells
        .start(
            "0199aa11-3333-7111-8111-111111111111",
            &spec("hang"),
            &argv("hang"),
            NOT_STARTED,
            ShellStart::default(),
        )
        .expect_err("over the total cap");
    assert_eq!(error.message, TOTAL_LIMIT);
    fixture
        .shells
        .kill(None, &first.id)
        .await
        .expect("stops the first");
    fixture
        .shells
        .start(
            OWNER,
            &spec("hang"),
            &argv("hang"),
            NOT_STARTED,
            ShellStart::default(),
        )
        .expect("a stopped shell frees its place");
    fixture.shells.dispose().await;
    assert_eq!(fixture.shells.running_count(), 0);
}

#[tokio::test]
async fn one_owner_cannot_reach_another_owners_shell() {
    let fixture = fixture(ShellLimits::default());
    let shell = fixture
        .shells
        .start(
            OWNER,
            &spec("lines"),
            &argv("lines"),
            NOT_STARTED,
            ShellStart::default(),
        )
        .expect("starts");
    for error in [
        fixture
            .shells
            .get(Some(OTHER), &shell.id)
            .expect_err("hidden"),
        fixture
            .shells
            .output(Some(OTHER), &shell.id, None)
            .expect_err("hidden"),
        fixture
            .shells
            .kill(Some(OTHER), &shell.id)
            .await
            .expect_err("hidden"),
    ] {
        assert_eq!(error.code, AppErrorCode::NotFound);
        assert_eq!(error.message, SHELL_MISSING);
    }
    assert!(fixture.shells.list(OTHER).is_empty());
    assert_eq!(fixture.shells.list(OWNER).len(), 1);
    settled(&fixture, &shell.id).await;
}

#[tokio::test]
async fn output_is_paged_by_sequence_and_the_buffer_is_capped() {
    let fixture = fixture(ShellLimits {
        buffer: 4096,
        ..ShellLimits::default()
    });
    let shell = fixture
        .shells
        .start(
            OWNER,
            &spec("flood"),
            &argv("flood"),
            NOT_STARTED,
            ShellStart::default(),
        )
        .expect("starts");
    settled(&fixture, &shell.id).await;
    let all = fixture
        .shells
        .output(None, &shell.id, None)
        .expect("output");
    assert!(all.truncated);
    assert!(all.text.len() <= 4096);
    assert!(all.next_seq >= 20_000);
    let after = fixture
        .shells
        .output(None, &shell.id, Some(all.next_seq))
        .expect("nothing new");
    assert!(after.text.is_empty() && !after.truncated);
    let early = fixture
        .shells
        .output(None, &shell.id, Some(0))
        .expect("from the start");
    assert!(early.truncated);
    assert_eq!(early.text, all.text);
    let (_, tail, cut) = fixture.shells.tail(None, &shell.id, 3).expect("a tail");
    assert_eq!(tail.lines().count(), 3);
    assert!(cut);
}

#[tokio::test]
async fn an_ended_shell_is_reported_to_the_next_turn_once() {
    let fixture = fixture(ShellLimits::default());
    let shell = fixture
        .shells
        .start(
            OWNER,
            &spec("lines"),
            &argv("lines"),
            NOT_STARTED,
            ShellStart::default(),
        )
        .expect("starts");
    settled(&fixture, &shell.id).await;
    let ended = fixture.shells.take_ended(OWNER, 2);
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].shell.id, shell.id);
    assert_eq!(ended[0].tail, "two\nthree");
    assert!(fixture.shells.take_ended(OWNER, 2).is_empty());
    assert!(fixture.shells.take_ended(OTHER, 2).is_empty());
}

#[tokio::test]
async fn closing_an_owner_kills_and_forgets_its_shells() {
    let fixture = fixture(ShellLimits::default());
    let mine = fixture
        .shells
        .start(
            OWNER,
            &spec("hang"),
            &argv("hang"),
            NOT_STARTED,
            ShellStart::default(),
        )
        .expect("starts");
    let theirs = fixture
        .shells
        .start(
            OTHER,
            &spec("hang"),
            &argv("hang"),
            NOT_STARTED,
            ShellStart::default(),
        )
        .expect("starts");
    tokio::time::timeout(Duration::from_secs(30), fixture.shells.close_owner(OWNER))
        .await
        .expect("closes in time");
    assert!(fixture.shells.list(OWNER).is_empty());
    assert_eq!(
        fixture.shells.get(None, &mine.id).expect_err("gone").code,
        AppErrorCode::NotFound
    );
    assert_eq!(
        fixture.shells.get(None, &theirs.id).expect("kept").status,
        BackgroundShellStatus::Running
    );
    fixture.shells.dispose().await;
    assert!(fixture.shells.list(OTHER).is_empty());
}

#[tokio::test]
async fn only_the_newest_ended_shells_are_kept() {
    let fixture = fixture(ShellLimits {
        kept: 2,
        ..ShellLimits::default()
    });
    let mut ids = Vec::new();
    for _ in 0..4 {
        let shell = fixture
            .shells
            .start(
                OWNER,
                &spec("lines"),
                &argv("lines"),
                NOT_STARTED,
                ShellStart::default(),
            )
            .expect("starts");
        settled(&fixture, &shell.id).await;
        ids.push(shell.id);
    }
    let kept: Vec<_> = fixture
        .shells
        .list(OWNER)
        .into_iter()
        .map(|shell| shell.id)
        .collect();
    assert_eq!(kept, ids[1..]);
}

#[tokio::test]
async fn a_program_that_cannot_start_reports_the_caller_message() {
    let fixture = fixture(ShellLimits::default());
    let error = fixture
        .shells
        .start(
            OWNER,
            &LaunchSpec {
                command: std::env::temp_dir()
                    .join("zvs-missing-program.exe")
                    .to_string_lossy()
                    .into_owned(),
                args: Vec::new(),
                env: BTreeMap::new(),
                cwd: None,
            },
            &argv("missing"),
            NOT_STARTED,
            ShellStart::default(),
        )
        .expect_err("nothing to start");
    assert_eq!(error.code, AppErrorCode::NativeError);
    assert_eq!(error.message, NOT_STARTED);
    assert!(fixture.shells.list(OWNER).is_empty());
}

async fn heard(heard: &Mutex<Vec<(String, bool)>>) -> Vec<(String, bool)> {
    for _ in 0..600 {
        let seen = heard.lock().expect("the log").clone();
        if !seen.is_empty() {
            return seen;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no listener was called");
}

#[tokio::test]
async fn a_notified_shell_waits_for_its_wake_up_and_carries_what_its_exit_hook_found() {
    let fixture = fixture(ShellLimits::default());
    let log: Arc<Mutex<Vec<(String, bool)>>> = Arc::default();
    let sink = Arc::clone(&log);
    fixture
        .shells
        .on_ended(Arc::new(move |owner: &str, notify: bool| {
            sink.lock()
                .expect("the log")
                .push((owner.to_owned(), notify));
        }));
    let artifact = ArtifactDto {
        id: format!("{OWNER}/report.xlsx"),
        name: "report.xlsx".to_owned(),
        format: "xlsx".to_owned(),
        size_bytes: 10,
    };
    let found = artifact.clone();
    let shell = fixture
        .shells
        .start(
            OWNER,
            &spec("lines"),
            &argv("lines"),
            NOT_STARTED,
            ShellStart::notify(true).finishing(Box::new(move || vec![found])),
        )
        .expect("starts");
    settled(&fixture, &shell.id).await;
    assert_eq!(heard(&log).await, vec![(OWNER.to_owned(), true)]);
    assert!(fixture.shells.has_notified(OWNER));
    assert!(fixture.shells.take_ended(OWNER, 2).is_empty());
    let ended = fixture.shells.take_notified(OWNER, 2);
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].tail, "two\nthree");
    assert_eq!(ended[0].artifacts, vec![artifact]);
    assert!(!fixture.shells.has_notified(OWNER));
    assert!(fixture.shells.take_notified(OWNER, 2).is_empty());
}

#[tokio::test]
async fn a_demoted_shell_goes_back_to_the_next_turn_note() {
    let fixture = fixture(ShellLimits::default());
    let shell = fixture
        .shells
        .start(
            OWNER,
            &spec("lines"),
            &argv("lines"),
            NOT_STARTED,
            ShellStart::notify(true),
        )
        .expect("starts");
    settled(&fixture, &shell.id).await;
    assert_eq!(fixture.shells.demote(OWNER), 1);
    assert!(fixture.shells.take_notified(OWNER, 2).is_empty());
    assert_eq!(fixture.shells.take_ended(OWNER, 2).len(), 1);
}

#[tokio::test]
async fn a_killed_shell_still_runs_its_exit_hook() {
    let fixture = fixture(ShellLimits::default());
    let ran = Arc::new(Mutex::new(false));
    let flag = Arc::clone(&ran);
    let shell = fixture
        .shells
        .start(
            OWNER,
            &spec("hang"),
            &argv("hang"),
            NOT_STARTED,
            ShellStart::default().finishing(Box::new(move || {
                *flag.lock().expect("the flag") = true;
                Vec::new()
            })),
        )
        .expect("starts");
    printed(&fixture, &shell.id, "started").await;
    fixture.shells.kill(None, &shell.id).await.expect("stops");
    assert!(*ran.lock().expect("the flag"));
}
