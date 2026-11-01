use std::collections::BTreeMap;

use zvs_contract::{BackgroundShellStatus, ChatEventDto, MessageRole, WAKE_STEP_DOMAIN};

use super::*;
use crate::background::{BackgroundShellOptions, BackgroundShells, ShellStart};
use crate::testing::FakeScript;
use crate::toolchain::LaunchSpec;

const STUB: &str = "background::tests::shell_stub";
const STUB_MODE: &str = "ZVS_SHELL_STUB";

fn stub(mode: &str) -> LaunchSpec {
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

fn waking(delay: Duration) -> (Harness, BackgroundShells) {
    let shells = BackgroundShells::new(BackgroundShellOptions::new(
        EventBus::default(),
        Arc::new(SystemClock),
    ));
    let harness = Harness::with(HarnessOptions {
        shells: Some(shells.clone()),
        wake_delay: Some(delay),
        ..HarnessOptions::default()
    });
    (harness, shells)
}

fn start(shells: &BackgroundShells, conversation: ConversationId, notify: bool) {
    shells
        .start(
            &conversation.to_string(),
            &stub("lines"),
            &["cargo".to_owned(), "test".to_owned()],
            "Не удалось запустить команду",
            ShellStart::notify(notify),
        )
        .expect("starts");
}

async fn all_ended(shells: &BackgroundShells, conversation: ConversationId) {
    for _ in 0..1200 {
        if shells
            .list(&conversation.to_string())
            .iter()
            .all(|shell| !shell.status.running())
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the processes did not end in time");
}

fn events_of(harness: &Harness, conversation: ConversationId) -> Vec<MessageDto> {
    harness
        .messages(conversation)
        .into_iter()
        .filter(|message| message.role == MessageRole::Event)
        .collect()
}

async fn settled(harness: &Harness, conversation: ConversationId, events: usize) {
    for _ in 0..1200 {
        let messages = harness.messages(conversation);
        let woken = messages
            .iter()
            .filter(|message| message.role == MessageRole::Event)
            .count();
        let idle = messages
            .last()
            .is_some_and(|last| last.role == MessageRole::Assistant && !last.partial);
        if woken >= events
            && idle
            && harness
                .chat
                .inner
                .require_idle(&conversation.to_string())
                .is_ok()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the dialog was not continued in time");
}

fn event_row(conversation: ConversationId, created_at: i64) -> MessageRow {
    MessageRow {
        id: new_id().hyphenated().to_string(),
        conversation_id: conversation.to_string(),
        role: MessageRole::Event,
        content: WAKE_HEADING.to_owned(),
        reasoning: String::new(),
        citations: Vec::new(),
        tool_calls: Vec::new(),
        selections: Vec::new(),
        command: None,
        offered_tools: Vec::new(),
        web_sources: Vec::new(),
        rounds: Vec::new(),
        tokens_in: 0,
        tokens_out: 0,
        usage_estimated: true,
        duration_ms: 0,
        run_id: None,
        partial: false,
        pinned: false,
        event: Some(ChatEventDto { shells: Vec::new() }),
        created_at,
    }
}

#[tokio::test]
async fn a_notified_process_continues_the_dialog_with_an_event_message() {
    let (harness, shells) = waking(Duration::from_millis(50));
    let conversation = harness.conversation();
    harness.turn(conversation, "Run the tests").await;
    start(&shells, conversation, true);
    settled(&harness, conversation, 1).await;
    let messages = harness.messages(conversation);
    let roles: Vec<MessageRole> = messages.iter().map(|message| message.role).collect();
    assert_eq!(
        roles,
        [
            MessageRole::User,
            MessageRole::Assistant,
            MessageRole::Event,
            MessageRole::Assistant
        ]
    );
    let woken = &messages[2];
    assert!(woken.content.starts_with(WAKE_HEADING));
    assert!(woken.run_id.is_some());
    let event = woken.event.as_ref().expect("the process result");
    assert_eq!(event.shells.len(), 1);
    assert_eq!(event.shells[0].shell.status, BackgroundShellStatus::Exited);
    assert_eq!(event.shells[0].shell.exit_code, Some(2));
    assert!(event.shells[0].tail.ends_with("one\ntwo\nthree"));
    assert!(woken.validate().is_ok());
    let prompt = harness
        .driver
        .requests()
        .last()
        .cloned()
        .expect("a request");
    assert!(
        prompt
            .messages
            .iter()
            .any(|message| message.content.contains("one\ntwo\nthree\n</output>"))
    );
    assert!(harness.events().iter().any(|event| matches!(
        &event.body,
        HostEventDraft::Step { step }
            if step.get("domain").and_then(Value::as_str) == Some(WAKE_STEP_DOMAIN)
    )));
    assert!(shells.take_ended(&conversation.to_string(), 5).is_empty());
}

#[tokio::test]
async fn processes_that_end_together_wake_the_dialog_once() {
    let (harness, shells) = waking(Duration::from_millis(1500));
    let conversation = harness.conversation();
    start(&shells, conversation, true);
    start(&shells, conversation, true);
    all_ended(&shells, conversation).await;
    settled(&harness, conversation, 1).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let woken = events_of(&harness, conversation);
    assert_eq!(woken.len(), 1);
    assert_eq!(woken[0].event.as_ref().expect("a result").shells.len(), 2);
}

#[tokio::test]
async fn a_process_without_notify_never_wakes_the_dialog() {
    let (harness, shells) = waking(Duration::from_millis(50));
    let conversation = harness.conversation();
    start(&shells, conversation, false);
    all_ended(&shells, conversation).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(harness.messages(conversation).is_empty());
    assert_eq!(shells.take_ended(&conversation.to_string(), 5).len(), 1);
}

#[tokio::test]
async fn a_wake_up_waits_for_the_running_turn_and_follows_it() {
    let (harness, shells) = waking(Duration::from_millis(50));
    harness.driver.script(FakeScript {
        manual: Some(true),
        ..FakeScript::default()
    });
    let conversation = harness.conversation();
    let handle = harness.send(conversation, "Write while a process runs");
    within(harness.driver.when_holding()).await;
    start(&shells, conversation, true);
    all_ended(&shells, conversation).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        harness
            .chat
            .wake(&conversation.to_string())
            .expect("checks")
            .is_none()
    );
    assert!(events_of(&harness, conversation).is_empty());
    assert!(shells.has_notified(&conversation.to_string()));
    harness.driver.clear_manual();
    harness.driver.release();
    harness.runs.wait(&handle.id).await;
    settled(&harness, conversation, 1).await;
    let roles: Vec<MessageRole> = harness
        .messages(conversation)
        .iter()
        .map(|message| message.role)
        .collect();
    assert_eq!(
        roles,
        [
            MessageRole::User,
            MessageRole::Assistant,
            MessageRole::Event,
            MessageRole::Assistant
        ]
    );
}

#[tokio::test]
async fn consecutive_wake_ups_are_capped_and_the_rest_goes_to_the_next_turn() {
    let (harness, shells) = waking(Duration::from_secs(3600));
    let conversation = harness.conversation();
    harness.turn(conversation, "Build it").await;
    harness
        .database
        .with_connection(|connection| {
            let repositories = Repositories::new(connection);
            for index in 0..WAKE_STREAK_MAX {
                repositories
                    .chat
                    .add_message(&event_row(conversation, 10_000_000_000_000 + index as i64))?;
            }
            Ok(())
        })
        .expect("seeds the wake-ups");
    assert_eq!(
        wake_streak(&harness.messages(conversation)),
        WAKE_STREAK_MAX
    );
    start(&shells, conversation, true);
    all_ended(&shells, conversation).await;
    assert!(
        harness
            .chat
            .wake(&conversation.to_string())
            .expect("checks")
            .is_none()
    );
    assert!(!shells.has_notified(&conversation.to_string()));
    assert_eq!(shells.take_ended(&conversation.to_string(), 5).len(), 1);
}

#[test]
fn a_message_from_the_person_resets_the_streak() {
    let conversation =
        ConversationId::parse("0199aa11-2222-7111-8111-111111111111").expect("an id");
    let event = message_dto(event_row(conversation, 1)).expect("a message");
    let person = MessageDto {
        role: MessageRole::User,
        event: None,
        ..event.clone()
    };
    let answer = MessageDto {
        role: MessageRole::Assistant,
        event: None,
        ..event.clone()
    };
    assert_eq!(wake_streak(&[]), 0);
    assert_eq!(
        wake_streak(&[event.clone(), answer.clone(), event.clone(), answer.clone()]),
        2
    );
    assert_eq!(
        wake_streak(&[
            event.clone(),
            answer.clone(),
            person,
            answer.clone(),
            event,
            answer
        ]),
        1
    );
}

#[tokio::test]
async fn a_removed_dialog_is_never_woken() {
    let (harness, shells) = waking(Duration::from_secs(3600));
    let conversation = harness.conversation();
    start(&shells, conversation, true);
    all_ended(&shells, conversation).await;
    harness.chat.remove(&conversation).expect("removes");
    assert!(
        harness
            .chat
            .wake(&conversation.to_string())
            .expect("checks")
            .is_none()
    );
    assert!(
        harness
            .runs
            .list(&RunListFilter::default())
            .expect("lists")
            .items
            .is_empty()
    );
}
