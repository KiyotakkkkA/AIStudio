use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use serde_json::{Map, Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AppError, ChatToolCallStatus, ComputerStatus, DeltaKind, HostEvent, HostEventDraft, StreamId,
};

use super::*;
use crate::browser::AgentRuns;
use crate::computer::agent::{ComputerAgent, ComputerChat, ComputerControls};
use crate::computer::sidecar::{DesktopFault, DesktopPort};
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

fn event(body: HostEventDraft) -> HostEvent {
    HostEvent::new(
        StreamId::parse("0199aa11-7777-7111-8111-111111111111").expect("a stream id"),
        1,
        0,
        body,
    )
}

fn call_step(id: &str, key: &str, status: &str, approval: Option<&str>, arguments: Value) -> Value {
    json!({
        "domain": "tool-call",
        "index": 0,
        "call": {
            "id": id,
            "key": key,
            "name": key.replace('.', "__"),
            "round": 0,
            "arguments": arguments,
            "status": status,
            "approvalId": approval,
        }
    })
}

fn process_request(id: &str, run: &str) -> Map<String, Value> {
    object(json!({
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
    }))
}

#[test]
fn the_reply_keeps_only_its_tail_on_a_character_boundary() {
    let mut text = "привет мир".to_owned();
    keep_tail(&mut text, 3);
    assert_eq!(text, "мир");
    let mut short = "ok".to_owned();
    keep_tail(&mut short, 10);
    assert_eq!(short, "ok");
}

#[test]
fn tool_call_steps_become_panel_entries_with_the_shell_reason() {
    let step = object(call_step(
        "c1",
        "shell.run",
        "running",
        None,
        json!({ "argv": ["npm", "test"], "reason": "  Проверить тесты " }),
    ));
    let call = tool_call(&step).expect("a tool call");
    let entry = call_entry(&call);
    assert_eq!(entry.key, "shell.run");
    assert_eq!(entry.status, ChatToolCallStatus::Running);
    assert_eq!(entry.summary.as_deref(), Some("Проверить тесты"));
    assert!(tool_call(&object(json!({ "domain": "goal", "call": {} }))).is_none());

    let mut calls = Vec::new();
    merge_call(&mut calls, entry.clone());
    merge_call(
        &mut calls,
        ComputerCall {
            status: ChatToolCallStatus::Succeeded,
            ..entry
        },
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].status, ChatToolCallStatus::Succeeded);
    for index in 0..CALLS_KEPT + 5 {
        merge_call(
            &mut calls,
            ComputerCall {
                id: format!("n{index}"),
                key: "fs.read".to_owned(),
                status: ChatToolCallStatus::Running,
                summary: None,
            },
        );
    }
    assert_eq!(calls.len(), CALLS_KEPT);
}

#[test]
fn approval_requests_are_described_and_the_computer_own_ones_are_skipped() {
    let pending = pending_request(&process_request(PROCESS_APPROVAL, RUN)).expect("a request");
    assert_eq!(pending.run, RUN);
    assert_eq!(pending.request.kind, "process");
    assert_eq!(pending.request.reason.as_deref(), Some("Собрать проект"));
    assert_eq!(
        pending.request.detail.as_deref(),
        Some("npm run \"build app\"")
    );

    let files = pending_request(&object(json!({
        "id": FILES_APPROVAL,
        "runId": RUN,
        "kind": "files",
        "subject": "fs.apply",
        "tool": "fs.apply",
        "action": "изменение файлов",
        "changes": [{ "path": "D:\\a.txt" }, { "path": "D:\\b.txt" }]
    })))
    .expect("a files request");
    assert_eq!(
        files.request.detail.as_deref(),
        Some("D:\\a.txt\nD:\\b.txt")
    );

    let own = object(json!({
        "id": FILES_APPROVAL,
        "runId": RUN,
        "kind": "apps",
        "subject": "computer.request_access",
        "tool": "computer.request_access",
        "action": "доступ"
    }));
    assert!(pending_request(&own).is_none());
    let action = object(json!({
        "id": FILES_APPROVAL,
        "runId": RUN,
        "subject": "computer.act",
        "tool": "computer.act",
        "action": "действие"
    }));
    assert!(pending_request(&action).is_none());
}

#[derive(Default)]
struct Runs {
    approved: Mutex<Vec<(String, String)>>,
    denied: Mutex<Vec<(String, String)>>,
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

    fn started(&self, _run_id: &str) -> Option<i64> {
        Some(1_000)
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
async fn the_panel_follows_the_reply_the_tool_calls_and_answers_requests_of_the_run() {
    let runs = Arc::new(Runs::default());
    let (agent, _database) = agent(runs.clone());
    agent.begin("conversation", RUN).expect("begins");
    assert_eq!(agent.state().started_at, Some(1_000));

    agent.absorb(&event(HostEventDraft::Token {
        delta: "Открываю ".to_owned(),
        kind: None,
    }));
    agent.absorb(&event(HostEventDraft::Token {
        delta: "скрытое".to_owned(),
        kind: Some(DeltaKind::Reasoning),
    }));
    agent.absorb(&event(HostEventDraft::Token {
        delta: "окно".to_owned(),
        kind: Some(DeltaKind::Text),
    }));
    agent.absorb(&event(HostEventDraft::Approval {
        request: process_request(PROCESS_APPROVAL, SUBAGENT_RUN),
    }));
    agent.absorb(&event(HostEventDraft::Approval {
        request: process_request(PROCESS_APPROVAL, SUBAGENT_RUN),
    }));
    agent.absorb(&event(HostEventDraft::Step {
        step: object(call_step(
            "c1",
            "shell.run",
            "awaiting-approval",
            Some(PROCESS_APPROVAL),
            json!({ "argv": ["npm"], "reason": "Собрать проект" }),
        )),
    }));

    let state = agent.state();
    assert_eq!(state.reply, "Открываю окно");
    assert_eq!(state.calls.len(), 1);
    assert_eq!(state.requests.len(), 1);
    assert_eq!(state.status, ComputerStatus::Working);

    agent.approve(PROCESS_APPROVAL).expect("approves");
    assert_eq!(
        runs.approved
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_slice(),
        &[(SUBAGENT_RUN.to_owned(), PROCESS_APPROVAL.to_owned())]
    );
    assert!(agent.state().requests.is_empty());

    agent.absorb(&event(HostEventDraft::Approval {
        request: process_request(FILES_APPROVAL, RUN),
    }));
    agent.absorb(&event(HostEventDraft::Step {
        step: object(call_step(
            "c2",
            "shell.run",
            "denied",
            Some(FILES_APPROVAL),
            json!({ "argv": ["npm"], "reason": "x" }),
        )),
    }));
    assert!(
        agent.state().requests.is_empty(),
        "answered in the main window"
    );
    assert!(agent.approve(FILES_APPROVAL).is_err());

    agent.end();
    agent.absorb(&event(HostEventDraft::Token {
        delta: "после".to_owned(),
        kind: None,
    }));
    assert_eq!(agent.state().reply, "");
}
