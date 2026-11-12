use std::sync::Mutex;

use zvs_contract::{ConversationId, ExecMode, HostEvent};

use super::*;
use crate::platform::clock::FakeClock;
use crate::testing::{MemorySink, RecordingShell};

struct Rig {
    connections: CliConnections,
    shell: Arc<RecordingShell>,
    studio: Arc<MemorySink>,
    events: EventBus,
}

fn rig(timeout: Duration) -> Rig {
    let events = EventBus::default();
    let studio = MemorySink::shared();
    events.attach(studio.clone());
    let shell = RecordingShell::shared();
    Rig {
        connections: CliConnections::with_timeout(
            Arc::new(FakeClock::new(1_000)),
            events.clone(),
            shell.clone(),
            timeout,
        ),
        shell,
        studio,
        events,
    }
}

fn recording() -> (NoticeSender, Arc<Mutex<Vec<CliNotice>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    (
        Arc::new(move |notice| sink.lock().expect("notices").push(notice)),
        seen,
    )
}

fn change() -> CliChangeDto {
    CliChangeDto::Mode {
        from: ExecMode::AlwaysAsk,
        to: ExecMode::NoRestrictions,
    }
}

async fn held(connections: &CliConnections) -> CliConsentDto {
    for _ in 0..200 {
        if let Some(consent) = connections.pending().into_iter().next() {
            return consent;
        }
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("no consent was held");
}

fn cli_steps(events: &[HostEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(&event.body, HostEventDraft::Step { step }
                if step.get("domain") == Some(&json!(CLI_STEP_DOMAIN)))
        })
        .count()
}

#[tokio::test]
async fn a_confirmed_change_applies_and_the_console_is_told_while_it_waits() {
    let rig = rig(CLI_CONSENT_TIMEOUT);
    let (notify, notices) = recording();
    let (id, _closed) = rig.connections.open(
        CliClient {
            pid: Some(42),
            image: Some("C:/tools/other.exe".to_owned()),
            recognised: false,
        },
        notify,
    );
    let waiting = {
        let connections = rig.connections.clone();
        tokio::spawn(async move { connections.confirm(id, change()).await })
    };
    let consent = held(&rig.connections).await;
    assert_eq!(consent.connection_id, id);
    assert_eq!(consent.change, change());
    assert!(!consent.recognised);
    assert_eq!(consent.image.as_deref(), Some("C:/tools/other.exe"));
    assert_eq!(consent.expires_at - consent.created_at, 60_000);
    assert_eq!(rig.shell.studio_shown(), 1);
    rig.connections.answer(&consent.id, true).expect("answers");
    assert!(waiting.await.expect("joins").is_ok());
    assert!(rig.connections.pending().is_empty());
    assert_eq!(
        *notices.lock().expect("notices"),
        [
            CliNotice::Consent { pending: true },
            CliNotice::Consent { pending: false }
        ]
    );
    assert!(cli_steps(&rig.studio.events()) >= 3);
}

#[tokio::test]
async fn a_declined_change_is_refused() {
    let rig = rig(CLI_CONSENT_TIMEOUT);
    let (notify, _) = recording();
    let (id, _closed) = rig.connections.open(CliClient::default(), notify);
    let waiting = {
        let connections = rig.connections.clone();
        tokio::spawn(async move { connections.confirm(id, change()).await })
    };
    let consent = held(&rig.connections).await;
    rig.connections.answer(&consent.id, false).expect("answers");
    let error = waiting.await.expect("joins").expect_err("declined");
    assert_eq!(error.code, AppErrorCode::PermissionDenied);
    assert_eq!(error.message, CLI_CONSENT_DECLINED);
    assert_eq!(
        rig.connections
            .answer(&consent.id, true)
            .expect_err("gone")
            .code,
        AppErrorCode::NotFound
    );
}

#[tokio::test]
async fn an_unanswered_change_expires() {
    let rig = rig(Duration::from_millis(5));
    let (notify, _) = recording();
    let (id, _closed) = rig.connections.open(CliClient::default(), notify);
    let error = rig
        .connections
        .confirm(id, change())
        .await
        .expect_err("expired");
    assert_eq!(error.message, CLI_CONSENT_EXPIRED);
    assert!(rig.connections.pending().is_empty());
}

#[tokio::test]
async fn without_a_studio_window_the_change_fails_at_once() {
    let rig = rig(CLI_CONSENT_TIMEOUT);
    rig.events.detach();
    let (notify, notices) = recording();
    let (id, _closed) = rig.connections.open(CliClient::default(), notify);
    let error = rig
        .connections
        .confirm(id, change())
        .await
        .expect_err("refused");
    assert_eq!(error.code, AppErrorCode::PermissionDenied);
    assert_eq!(error.message, CLI_STUDIO_REQUIRED);
    assert!(notices.lock().expect("notices").is_empty());
    assert_eq!(rig.shell.studio_shown(), 0);
}

#[tokio::test]
async fn closing_a_connection_withdraws_its_held_change() {
    let rig = rig(CLI_CONSENT_TIMEOUT);
    let (notify, _) = recording();
    let (id, closed) = rig.connections.open(CliClient::default(), notify);
    let waiting = {
        let connections = rig.connections.clone();
        tokio::spawn(async move { connections.confirm(id, change()).await })
    };
    held(&rig.connections).await;
    rig.connections.close(id);
    let error = waiting.await.expect("joins").expect_err("closed");
    assert_eq!(error.message, CLI_CONNECTION_CLOSED);
    assert!(closed.is_cancelled());
    assert!(rig.connections.list().is_empty());
    assert!(rig.connections.ensure_open(id).is_err());
}

#[test]
fn the_studio_lists_and_disconnects_connections() {
    let rig = rig(CLI_CONSENT_TIMEOUT);
    let (notify, _) = recording();
    let (id, closed) = rig.connections.open(
        CliClient {
            pid: Some(7),
            image: Some("C:/ZVS/cli/aistudio.exe".to_owned()),
            recognised: true,
        },
        notify,
    );
    let dialog = ConversationId::parse("1f0e2d3c-4b5a-4968-8776-655443322110")
        .expect("an id")
        .to_string();
    rig.connections.adopt(id, &dialog);
    let listed = rig.connections.list();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);
    assert_eq!(listed[0].pid, Some(7));
    assert!(listed[0].recognised);
    assert_eq!(listed[0].connected_at, 1_000);
    assert_eq!(listed[0].dialogs, 1);
    rig.connections.disconnect(id).expect("disconnects");
    assert!(closed.is_cancelled());
    assert_eq!(
        rig.connections.disconnect(99).expect_err("unknown").code,
        AppErrorCode::NotFound
    );
}

#[test]
fn a_connection_sees_only_streams_of_its_dialogs_and_resolves_each_once() {
    let rig = rig(CLI_CONSENT_TIMEOUT);
    let (notify, _) = recording();
    let (first, _) = rig.connections.open(CliClient::default(), notify.clone());
    let (second, _) = rig.connections.open(CliClient::default(), notify);
    let stream = StreamId::new(new_id());
    let dialog = "1f0e2d3c-4b5a-4968-8776-655443322110".to_owned();
    let resolved = std::sync::atomic::AtomicUsize::new(0);
    let resolve = || {
        resolved.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Some(dialog.clone())
    };
    assert!(!rig.connections.sees(first, stream, resolve));
    assert!(!rig.connections.sees(first, stream, resolve));
    assert_eq!(resolved.load(std::sync::atomic::Ordering::SeqCst), 1);
    rig.connections.adopt(first, &dialog);
    assert!(rig.connections.sees(first, stream, resolve));
    assert!(!rig.connections.sees(second, stream, resolve));
    assert!(
        !rig.connections
            .sees(first, StreamId::new(new_id()), || None)
    );
    assert!(rig.connections.owns(first, &dialog));
    assert!(!rig.connections.owns(second, &dialog));
}

#[test]
fn a_released_dialog_stays_released_after_its_connection_closes() {
    let rig = rig(CLI_CONSENT_TIMEOUT);
    let (notify, _) = recording();
    let (id, _) = rig.connections.open(CliClient::default(), notify);
    rig.connections.release("d");
    rig.connections.adopt(id, "d");
    rig.connections.close(id);
    assert!(rig.connections.released("d"));
    assert!(!rig.connections.owns(id, "d"));
}
