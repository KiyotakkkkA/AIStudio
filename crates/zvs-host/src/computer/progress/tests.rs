use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, ChatToolCallStatus, ComputerFeed, ComputerStatus, DeltaKind, HostEvent,
    HostEventDraft, StreamId,
};

use super::*;
use crate::browser::AgentRuns;
use crate::computer::agent::{ComputerAgent, ComputerChat, ComputerControls};
use crate::computer::sidecar::{DesktopFault, DesktopPort};
use crate::kernel::{RunObserver, Subscription};
use crate::platform::clock::FakeClock;
use crate::platform::logger::SilentLogger;
use crate::testing::{TemporaryDatabase, temporary_database};

const RUN: &str = "0199aa11-3333-7111-8111-111111111111";
const SUBAGENT_RUN: &str = "0199aa11-4444-7111-8111-111111111111";
const PROCESS_APPROVAL: &str = "0199aa11-5555-7111-8111-111111111111";
const FILES_APPROVAL: &str = "0199aa11-6666-7111-8111-111111111111";

fn object(value: Value) -> Map<String, Value> {
    value.as_object().cloned().expect("an object")
}

fn event(seq: u64, body: HostEventDraft) -> HostEvent {
    HostEvent::new(
        StreamId::parse("0199aa11-7777-7111-8111-111111111111").expect("a stream id"),
        seq,
        0,
        body,
    )
}

fn token(delta: &str, kind: Option<DeltaKind>) -> HostEventDraft {
    HostEventDraft::Token {
        delta: delta.to_owned(),
        kind,
    }
}

fn call_step(
    index: usize,
    id: &str,
    key: &str,
    status: &str,
    approval: Option<&str>,
    arguments: Value,
) -> HostEventDraft {
    HostEventDraft::Step {
        step: object(json!({
            "domain": "tool-call",
            "index": index,
            "call": {
                "id": id,
                "key": key,
                "name": key.replace('.', "__"),
                "round": index,
                "arguments": arguments,
                "status": status,
                "approvalId": approval,
            }
        })),
    }
}

fn process_request(id: &str, run: &str) -> HostEventDraft {
    HostEventDraft::Approval {
        request: object(json!({
            "id": id,
            "runId": run,
            "kind": "process",
            "subject": "shell.run",
            "tool": "shell.run",
            "action": "запуск команды",
            "process": {
                "program": "npm",
                "argv": ["npm", "run", "build app"],
                "cwd": "D:\\work",
                "packages": [],
                "reason": "Собрать проект"
            }
        })),
    }
}

#[test]
fn the_feed_keeps_text_reasoning_and_rounds_like_the_chat() {
    let mut feed = ComputerFeed::default();

    assert_eq!(
        absorb(&mut feed, &token("план", Some(DeltaKind::Reasoning))),
        Absorbed::Token
    );
    absorb(&mut feed, &token("Открываю ", None));
    absorb(
        &mut feed,
        &call_step(0, "c1", "computer.screenshot", "succeeded", None, json!({})),
    );
    absorb(&mut feed, &token("окно", Some(DeltaKind::Text)));

    assert_eq!(feed.content, "Открываю окно");
    assert_eq!(feed.reasoning, "план");
    assert_eq!(feed.rounds.len(), 2);
    assert_eq!(feed.rounds[0].text, "Открываю ");
    assert_eq!(feed.rounds[0].reasoning, "план");
    assert_eq!(feed.rounds[1].text, "окно");
}

#[test]
fn tool_calls_keep_their_arguments_and_replace_by_index() {
    let mut feed = ComputerFeed::default();
    let arguments = json!({ "argv": ["npm"], "reason": "Собрать проект" });

    absorb(
        &mut feed,
        &call_step(0, "c1", "shell.run", "running", None, arguments.clone()),
    );
    let changed = absorb(
        &mut feed,
        &call_step(0, "c1", "shell.run", "failed", None, arguments.clone()),
    );

    assert_eq!(changed, Absorbed::Changed);
    assert_eq!(feed.tool_calls.len(), 1);
    assert_eq!(feed.tool_calls[0].status, ChatToolCallStatus::Failed);
    assert_eq!(feed.tool_calls[0].arguments, arguments);
}

#[test]
fn approvals_are_kept_once_and_dropped_when_their_call_settles() {
    let mut feed = ComputerFeed::default();

    assert_eq!(
        absorb(&mut feed, &process_request(PROCESS_APPROVAL, RUN)),
        Absorbed::Changed
    );
    assert_eq!(
        absorb(&mut feed, &process_request(PROCESS_APPROVAL, RUN)),
        Absorbed::Nothing
    );
    absorb(
        &mut feed,
        &call_step(
            0,
            "c1",
            "shell.run",
            "awaiting-approval",
            Some(PROCESS_APPROVAL),
            json!({}),
        ),
    );
    assert_eq!(feed.approvals.len(), 1);

    absorb(
        &mut feed,
        &call_step(
            0,
            "c1",
            "shell.run",
            "running",
            Some(PROCESS_APPROVAL),
            json!({}),
        ),
    );
    assert!(feed.approvals.is_empty());
}

const MESSAGES: &str =
    include_str!("../../../../../packages/shared/fixtures/chat/MessageDto.valid.json");

#[test]
fn the_persisted_message_replaces_the_streamed_text() {
    let fixtures: Value = serde_json::from_str(MESSAGES).expect("fixtures");
    let message = fixtures[0]["value"].clone();
    let mut feed = ComputerFeed {
        content: "черновик".to_owned(),
        ..ComputerFeed::default()
    };
    let step = HostEventDraft::Step {
        step: object(json!({ "nodeId": "persist", "status": "succeeded", "output": message })),
    };

    assert_eq!(absorb(&mut feed, &step), Absorbed::Changed);
    assert_eq!(feed.content, message["content"].as_str().expect("content"));
}

#[test]
fn the_status_follows_the_live_feed() {
    let mut feed = ComputerFeed::default();
    assert_eq!(
        derived_status(ComputerStatus::Working, &feed, &[], false),
        ComputerStatus::Starting
    );
    assert_eq!(
        derived_status(ComputerStatus::Idle, &feed, &[], false),
        ComputerStatus::Idle
    );

    absorb(
        &mut feed,
        &call_step(0, "c1", "computer.act", "running", None, json!({})),
    );
    assert_eq!(
        derived_status(ComputerStatus::Starting, &feed, &[], false),
        ComputerStatus::Working
    );

    absorb(&mut feed, &process_request(PROCESS_APPROVAL, RUN));
    assert_eq!(
        derived_status(ComputerStatus::Working, &feed, &[], false),
        ComputerStatus::Awaiting
    );
    assert_eq!(
        derived_status(ComputerStatus::Stopping, &feed, &[], false),
        ComputerStatus::Stopping
    );
}

#[derive(Default)]
struct Runs {
    approved: Mutex<Vec<(String, String)>>,
    denied: Mutex<Vec<(String, String)>>,
    recorded: Mutex<Vec<HostEvent>>,
    observers: Mutex<Vec<RunObserver>>,
    settled: Mutex<Vec<String>>,
}

impl Runs {
    fn emit(&self, event: &HostEvent) {
        let observers = self
            .observers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        for observer in observers {
            observer(event);
        }
    }
}

#[async_trait]
impl AgentRuns for Runs {
    fn title(&self, _run_id: &str) -> Option<String> {
        None
    }

    fn approve(&self, run_id: &str, approval_id: &str) -> Result<(), AppError> {
        self.approved
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((run_id.to_owned(), approval_id.to_owned()));
        Ok(())
    }

    fn deny(&self, run_id: &str, approval_id: &str) -> Result<(), AppError> {
        self.denied
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((run_id.to_owned(), approval_id.to_owned()));
        Ok(())
    }

    async fn cancel(&self, _run_id: &str) -> Result<(), AppError> {
        Ok(())
    }

    async fn wait(&self, _run_id: &str) {
        std::future::pending::<()>().await;
    }

    fn observe(&self, _run_id: &str, observer: RunObserver) -> Option<Subscription> {
        self.observers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(observer);
        None
    }

    fn started(&self, _run_id: &str) -> Option<i64> {
        Some(1_000)
    }

    fn record(&self, _run_id: &str) -> Vec<HostEvent> {
        self.recorded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn pending(&self, _run_id: &str, approval_id: &str) -> bool {
        !self
            .settled
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|id| id == approval_id)
    }
}

struct NoChat;

impl ComputerChat for NoChat {
    fn send(&self, _conversation_id: &str, _text: &str) -> Result<String, AppError> {
        Ok(String::new())
    }
}

struct NoDesktop;

#[async_trait]
impl DesktopPort for NoDesktop {
    async fn call(
        &self,
        _method: &str,
        _params: Value,
        _cancel: &CancellationToken,
    ) -> Result<Value, DesktopFault> {
        Ok(json!({}))
    }

    async fn close(&self) {}
}

fn agent(runs: Arc<Runs>) -> (ComputerAgent, TemporaryDatabase) {
    let database = temporary_database();
    let agent = ComputerAgent::new(
        Arc::new(NoDesktop),
        database.shared(),
        Arc::new(FakeClock::default()),
        Arc::new(SilentLogger),
    );
    agent.attach(ComputerControls {
        runs,
        chat: Arc::new(NoChat),
        folders: None,
    });
    (agent, database)
}

#[tokio::test]
async fn the_panel_replays_what_happened_before_the_first_computer_call() {
    let runs = Arc::new(Runs::default());
    *runs.recorded.lock().unwrap_or_else(PoisonError::into_inner) = vec![
        event(1, token("думаю", Some(DeltaKind::Reasoning))),
        event(2, token("Открываю отчёт", None)),
        event(
            3,
            call_step(
                0,
                "c1",
                "shell.run",
                "failed",
                None,
                json!({ "argv": "[\"cmd.exe\"]" }),
            ),
        ),
    ];
    let (agent, _database) = agent(runs.clone());

    agent.begin("conversation", RUN).expect("begins");

    let state = agent.state();
    assert_eq!(state.started_at, Some(1_000));
    assert_eq!(state.feed.reasoning, "думаю");
    assert_eq!(state.feed.content, "Открываю отчёт");
    assert_eq!(state.feed.tool_calls.len(), 1);
    assert_eq!(state.status, ComputerStatus::Starting);

    runs.emit(&event(3, token(" повтор", None)));
    runs.emit(&event(4, token(" и окно", None)));
    assert_eq!(agent.state().feed.content, "Открываю отчёт и окно");
}

#[tokio::test]
async fn the_panel_answers_approvals_of_the_run_and_its_subagents() {
    let runs = Arc::new(Runs::default());
    let (agent, _database) = agent(runs.clone());
    agent.begin("conversation", RUN).expect("begins");

    runs.emit(&event(1, process_request(PROCESS_APPROVAL, SUBAGENT_RUN)));
    runs.emit(&event(
        2,
        call_step(
            0,
            "c1",
            "shell.run",
            "awaiting-approval",
            Some(PROCESS_APPROVAL),
            json!({ "argv": ["npm"], "reason": "Собрать проект" }),
        ),
    ));
    let state = agent.state();
    assert_eq!(state.feed.approvals.len(), 1);
    assert_eq!(state.status, ComputerStatus::Awaiting);

    agent.approve(PROCESS_APPROVAL).expect("approves");
    assert_eq!(
        runs.approved
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_slice(),
        &[(SUBAGENT_RUN.to_owned(), PROCESS_APPROVAL.to_owned())]
    );
    assert!(agent.state().feed.approvals.is_empty());

    runs.emit(&event(3, process_request(FILES_APPROVAL, RUN)));
    assert_eq!(agent.state().feed.approvals.len(), 1);
    runs.settled
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(FILES_APPROVAL.to_owned());
    assert!(
        agent.state().feed.approvals.is_empty(),
        "answered in the main window"
    );

    agent.deny(FILES_APPROVAL).expect("denies");
    assert_eq!(
        runs.denied
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_slice(),
        &[(RUN.to_owned(), FILES_APPROVAL.to_owned())]
    );
    assert!(agent.approve(FILES_APPROVAL).is_err(), "already settled");

    agent.end();
    runs.emit(&event(4, token("после", None)));
    assert_eq!(agent.state().feed, ComputerFeed::default());
}
