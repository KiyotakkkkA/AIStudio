use std::sync::Arc;

use serde_json::{Map, json};
use zvs_contract::{
    ConversationId, HostEventDraft, NotificationTargetDto, RunKind, RunOutcomeDto, RunOutcomeStatus,
};

use super::copy;
use super::{
    ACTIVATION_REPEAT_MS, COALESCE_WINDOW_MS, NotificationService, NotificationServiceOptions,
    RunSubject,
};
use crate::platform::events::EventBus;
use crate::platform::shell::{NotificationKind, TRAY_SETTING_KEY};
use crate::services::SettingService;
use crate::testing::{
    FakeClock, MemorySink, RecordingShell, TemporaryDatabase, temporary_database,
};

const CONVERSATION: &str = "0199aa11-1111-7111-8111-111111111111";
const DOWNLOAD: &str = "0199aa11-2222-7111-8111-111111111111";

struct Fixture {
    service: NotificationService,
    shell: Arc<RecordingShell>,
    clock: Arc<FakeClock>,
    settings: SettingService,
    sink: Arc<MemorySink>,
    _database: TemporaryDatabase,
}

fn fixture() -> Fixture {
    let database = temporary_database();
    let clock = Arc::new(FakeClock::default());
    let shell = RecordingShell::shared();
    let settings = SettingService::new(database.shared(), clock.clone());
    let sink = MemorySink::shared();
    let events = EventBus::default();
    events.attach(sink.clone());
    let service = NotificationService::new(NotificationServiceOptions::new(
        database.shared(),
        settings.clone(),
        shell.clone(),
        events,
        clock.clone(),
    ));
    Fixture {
        service,
        shell,
        clock,
        settings,
        sink,
        _database: database,
    }
}

fn chat() -> RunSubject {
    RunSubject {
        kind: RunKind::Chat,
        id: CONVERSATION.to_owned(),
        label: "Отчёт за квартал".to_owned(),
    }
}

fn download() -> RunSubject {
    RunSubject {
        kind: RunKind::Download,
        id: DOWNLOAD.to_owned(),
        label: "Qwen2.5 7B".to_owned(),
    }
}

fn conversation() -> NotificationTargetDto {
    NotificationTargetDto::Conversation {
        conversation_id: ConversationId::parse(CONVERSATION).expect("uuid"),
    }
}

fn end(status: RunOutcomeStatus) -> HostEventDraft {
    HostEventDraft::End {
        outcome: RunOutcomeDto {
            status,
            code: None,
            message: None,
        },
    }
}

fn approval(subject: &str) -> HostEventDraft {
    let mut request = Map::new();
    request.insert("id".to_owned(), json!("a1"));
    request.insert("subject".to_owned(), json!(subject));
    HostEventDraft::Approval { request }
}

impl Fixture {
    fn set(&self, key: &str, value: bool) {
        self.settings.set(key, &json!(value)).expect("stores");
    }

    fn titles(&self) -> Vec<String> {
        self.shell
            .notified()
            .into_iter()
            .map(|draft| draft.title)
            .collect()
    }
}

#[test]
fn each_event_raises_its_own_toast_with_its_target() {
    let fixture = fixture();
    fixture.service.handle(&chat(), &approval("fs.apply"));
    fixture.service.handle(&chat(), &end(RunOutcomeStatus::Ok));
    fixture
        .service
        .handle(&download(), &end(RunOutcomeStatus::Ok));
    let notified = fixture.shell.notified();
    assert_eq!(
        notified
            .iter()
            .map(|draft| (draft.kind, draft.tag.as_str(), draft.target))
            .collect::<Vec<_>>(),
        [
            (NotificationKind::Approval, CONVERSATION, conversation()),
            (NotificationKind::Turn, CONVERSATION, conversation()),
            (
                NotificationKind::Download,
                DOWNLOAD,
                NotificationTargetDto::Downloads
            ),
        ]
    );
    assert_eq!(
        fixture.titles(),
        [
            copy::APPROVAL_TITLE,
            copy::TURN_DONE_TITLE,
            copy::DOWNLOAD_DONE_TITLE
        ]
    );
    assert_eq!(notified[0].body, "«fs.apply» в диалоге «Отчёт за квартал»");
    assert_eq!(notified[1].body, "Отчёт за квартал");
    assert_eq!(notified[2].body, "Qwen2.5 7B");
}

#[test]
fn failures_have_their_own_titles_and_cancellations_stay_silent() {
    let fixture = fixture();
    fixture
        .service
        .handle(&chat(), &end(RunOutcomeStatus::Cancelled));
    fixture
        .service
        .handle(&download(), &end(RunOutcomeStatus::Cancelled));
    assert!(fixture.shell.notified().is_empty());
    fixture
        .service
        .handle(&chat(), &end(RunOutcomeStatus::Failed));
    fixture
        .service
        .handle(&download(), &end(RunOutcomeStatus::Failed));
    assert_eq!(
        fixture.titles(),
        [copy::TURN_FAILED_TITLE, copy::DOWNLOAD_FAILED_TITLE]
    );
}

#[test]
fn other_run_kinds_and_events_raise_nothing() {
    let fixture = fixture();
    let job = RunSubject {
        kind: RunKind::Job,
        ..chat()
    };
    fixture.service.handle(&job, &approval("fs.apply"));
    fixture.service.handle(&job, &end(RunOutcomeStatus::Ok));
    fixture.service.handle(&download(), &approval("fs.apply"));
    fixture.service.handle(
        &chat(),
        &HostEventDraft::Token {
            delta: "x".to_owned(),
            kind: None,
        },
    );
    assert!(fixture.shell.notified().is_empty());
}

#[test]
fn the_master_switch_silences_every_kind_and_each_kind_only_itself() {
    let fixture = fixture();
    fixture.set("notifications.enabled", false);
    fixture.service.handle(&chat(), &approval("fs.apply"));
    fixture.service.handle(&chat(), &end(RunOutcomeStatus::Ok));
    fixture
        .service
        .handle(&download(), &end(RunOutcomeStatus::Ok));
    assert!(fixture.shell.notified().is_empty());

    fixture.set("notifications.enabled", true);
    fixture.set("notifications.turn", false);
    fixture.service.handle(&chat(), &approval("fs.apply"));
    fixture.service.handle(&chat(), &end(RunOutcomeStatus::Ok));
    fixture
        .service
        .handle(&download(), &end(RunOutcomeStatus::Ok));
    assert_eq!(
        fixture.titles(),
        [copy::APPROVAL_TITLE, copy::DOWNLOAD_DONE_TITLE]
    );
    for kind in NotificationKind::ALL {
        assert_eq!(
            fixture.service.enabled(kind),
            kind != NotificationKind::Turn
        );
    }
}

#[test]
fn a_burst_for_one_subject_replaces_the_toast_without_popping_it_again() {
    let fixture = fixture();
    fixture.service.handle(&chat(), &end(RunOutcomeStatus::Ok));
    fixture.clock.advance(COALESCE_WINDOW_MS - 1);
    fixture.service.handle(&chat(), &end(RunOutcomeStatus::Ok));
    fixture
        .service
        .handle(&download(), &end(RunOutcomeStatus::Ok));
    fixture.clock.advance(COALESCE_WINDOW_MS);
    fixture.service.handle(&chat(), &end(RunOutcomeStatus::Ok));
    assert_eq!(
        fixture
            .shell
            .notified()
            .iter()
            .map(|draft| (draft.kind, draft.silent))
            .collect::<Vec<_>>(),
        [
            (NotificationKind::Turn, false),
            (NotificationKind::Turn, true),
            (NotificationKind::Download, false),
            (NotificationKind::Turn, false),
        ]
    );
}

#[test]
fn bodies_are_clipped_and_carry_no_message_text() {
    let fixture = fixture();
    let long = RunSubject {
        label: "а".repeat(500),
        ..chat()
    };
    fixture.service.handle(&long, &end(RunOutcomeStatus::Ok));
    let body = &fixture.shell.notified()[0].body;
    assert_eq!(body.chars().count(), copy::BODY_MAX_CHARS);
    assert!(body.ends_with('…'));
    assert_eq!(copy::approval_body(None, "Диалог"), "Диалог «Диалог»");
}

fn navigations(fixture: &Fixture) -> Vec<serde_json::Value> {
    fixture
        .sink
        .events()
        .into_iter()
        .filter_map(|event| match event.body {
            HostEventDraft::Step { step } => Some(serde_json::Value::Object(step)),
            _ => None,
        })
        .collect()
}

#[test]
fn activation_navigates_the_renderer_to_the_target() {
    let fixture = fixture();
    assert_eq!(fixture.service.take_pending(), None);
    fixture.service.activated(conversation());
    fixture.service.activated(NotificationTargetDto::Downloads);
    assert_eq!(
        navigations(&fixture),
        [
            json!({ "domain": "navigation", "path": "/chat", "conversationId": CONVERSATION }),
            json!({ "domain": "navigation", "path": "/downloads" }),
        ]
    );
}

#[test]
fn a_cold_start_target_waits_for_the_first_subscriber_only() {
    let fixture = fixture();
    fixture.service.activated(conversation());
    assert!(navigations(&fixture).is_empty());
    assert_eq!(fixture.service.take_pending(), Some(conversation()));
    assert_eq!(fixture.service.take_pending(), None);
    assert!(navigations(&fixture).is_empty());
}

#[test]
fn a_later_activation_replaces_an_undelivered_target() {
    let fixture = fixture();
    fixture.service.activated(conversation());
    fixture.service.activated(NotificationTargetDto::Downloads);
    assert_eq!(
        fixture.service.take_pending(),
        Some(NotificationTargetDto::Downloads)
    );
}

#[test]
fn one_click_delivered_twice_opens_the_target_once() {
    let fixture = fixture();
    fixture.service.take_pending();
    fixture.service.activated(conversation());
    fixture.clock.advance(ACTIVATION_REPEAT_MS - 1);
    fixture.service.activated(conversation());
    assert_eq!(navigations(&fixture).len(), 1);
    fixture.service.activated(NotificationTargetDto::Downloads);
    assert_eq!(navigations(&fixture).len(), 2);
    fixture.clock.advance(ACTIVATION_REPEAT_MS);
    fixture.service.activated(conversation());
    assert_eq!(navigations(&fixture).len(), 3);
}

#[test]
fn clearing_reaches_the_shell() {
    let fixture = fixture();
    fixture.service.clear(conversation());
    assert_eq!(fixture.shell.cleared(), [conversation()]);
}

#[test]
fn startup_applies_the_tray_and_adopts_the_system_autostart_state() {
    let fixture = fixture();
    fixture.set(TRAY_SETTING_KEY, true);
    fixture.set("app.autostart", true);
    fixture.shell.register(Some(false));
    fixture.service.start().expect("starts");
    assert_eq!(fixture.shell.tray(), [true]);
    assert_eq!(
        fixture
            .settings
            .get("app.autostart")
            .expect("reads")
            .map(|setting| setting.value),
        Some(json!(false))
    );

    let unknown = self::fixture();
    unknown.set("app.autostart", true);
    unknown.service.start().expect("starts");
    assert_eq!(unknown.shell.tray(), [false]);
    assert_eq!(
        unknown
            .settings
            .get("app.autostart")
            .expect("reads")
            .map(|setting| setting.value),
        Some(json!(true))
    );
}
