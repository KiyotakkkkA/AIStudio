use std::sync::Arc;
use std::sync::atomic::Ordering;

use serde_json::json;
use zvs_contract::{
    AppErrorCode, ChatCommand, ChatGoalDto, ChatSendInput, ChatToolCallStatus, ConversationId,
    GOAL_STEP_DOMAIN, GoalReason, GoalStatus, HostEventDraft, RunHandleDto, RunStatus,
};

use super::{Loop, call, call_with_id, text, within};
use crate::data::Repositories;
use crate::data::repositories::ConversationPatch;
use crate::drivers::ai::{TextDelta, TurnUsage};
use crate::platform::clock::{FakeClock, SharedClock};
use crate::services::chat::tests::eventually;
use crate::services::chat_goal::{
    GOAL_CONTINUE_TEXT, GOAL_EVIDENCE_REQUIRED, GOAL_NOT_PAUSED, GOAL_NOT_STOPPABLE, GOAL_RUNNING,
    GOAL_TOOL, GOAL_TOOL_NAME, MINUTES_KEY, ROUNDS_KEY, TOKENS_KEY,
};
use crate::services::chat_tools::CHAT_TOOL_CALLS;

const DONE: &str = "Тесты добавлены";
const EVIDENCE: &str = "cargo test: 12 passed";

fn complete(summary: &str, evidence: &str) -> TextDelta {
    call(
        GOAL_TOOL_NAME,
        &json!({ "summary": summary, "evidence": evidence }),
    )
}

fn lookup(round: usize) -> TextDelta {
    call_with_id(
        "demo__lookup",
        &json!({ "query": format!("q{round}") }),
        &format!("call_{round}"),
    )
}

impl Loop {
    fn set(&self, key: &str, value: &str) {
        self.harness
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).settings.set(key, value, 1)?)
            })
            .expect("stores a setting");
    }

    fn goal_of(&self, conversation: ConversationId) -> ChatGoalDto {
        self.harness
            .chat
            .get(&conversation)
            .expect("a conversation")
            .conversation
            .goal
            .expect("a goal")
    }

    fn start_goal(&self, conversation: ConversationId, text: &str) -> RunHandleDto {
        self.harness
            .chat
            .send(&ChatSendInput {
                command: Some(ChatCommand::Goal),
                ..ChatSendInput::new(conversation, text)
            })
            .expect("starts a goal")
    }

    async fn goal_turn(&self, conversation: ConversationId, text: &str) -> RunHandleDto {
        let handle = self.start_goal(conversation, text);
        within(self.harness.runs.wait(&handle.id)).await;
        handle
    }

    fn goal_steps(&self, handle: &RunHandleDto) -> Vec<serde_json::Map<String, serde_json::Value>> {
        self.harness
            .events()
            .into_iter()
            .filter(|event| event.stream_id == handle.stream_id)
            .filter_map(|event| match event.body {
                HostEventDraft::Step { step }
                    if step.get("domain").and_then(|domain| domain.as_str())
                        == Some(GOAL_STEP_DOMAIN) =>
                {
                    Some(step)
                }
                _ => None,
            })
            .collect()
    }
}

#[tokio::test]
async fn a_goal_works_past_the_chat_call_cap_and_ends_on_completion_with_evidence() {
    let test = Loop::new();
    let rounds = CHAT_TOOL_CALLS + 3;
    for round in 0..rounds {
        test.driver.round(move |_| vec![lookup(round)]);
    }
    test.driver
        .round(|_| vec![text("Проверено."), complete(DONE, EVIDENCE)]);
    let created = test.conversation(&["demo.lookup"]);

    let handle = test.goal_turn(created, "Добавить тесты").await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert_eq!(test.calls.lookups().len(), rounds);
    let goal = test.goal_of(created);
    assert_eq!(goal.status, GoalStatus::Done);
    assert_eq!(goal.summary.as_deref(), Some(DONE));
    assert_eq!(goal.usage.rounds, rounds as u64 + 1);
    assert!(goal.ended_at.is_some());
    let requests = test.driver.requests();
    assert!(
        requests[0]
            .tools
            .iter()
            .any(|tool| tool.name == GOAL_TOOL_NAME)
    );
    let system = requests[0].system.as_deref().unwrap_or_default();
    assert!(system.contains("Добавить тесты"));
    assert!(system.contains(GOAL_TOOL_NAME));
    let last = test.harness.last(created);
    let completion = last
        .tool_calls
        .iter()
        .find(|call| call.key == GOAL_TOOL)
        .expect("the completion call");
    assert_eq!(completion.status, ChatToolCallStatus::Succeeded);
    assert_eq!(completion.arguments["evidence"], EVIDENCE);
    let steps = test.goal_steps(&handle);
    assert_eq!(steps.len(), rounds + 2);
    assert_eq!(steps[0]["round"], 0);
    assert_eq!(steps.last().expect("a last step")["status"], "done");
}

#[tokio::test]
async fn an_ordinary_turn_after_a_finished_goal_keeps_the_call_cap() {
    let test = Loop::new();
    test.driver.round(|_| vec![complete(DONE, EVIDENCE)]);
    let created = test.conversation(&["demo.lookup"]);
    test.goal_turn(created, "Цель").await;
    test.driver.fallback(|count| vec![lookup(count)]);

    let handle = test.turn(created, "Обычный вопрос").await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    assert_eq!(test.calls.lookups().len(), CHAT_TOOL_CALLS);
    let last = test.driver.requests().pop().expect("a request");
    assert!(!last.tools.iter().any(|tool| tool.name == GOAL_TOOL_NAME));
    assert_eq!(test.goal_of(created).status, GoalStatus::Done);
}

#[tokio::test]
async fn empty_evidence_is_refused_and_the_goal_keeps_going() {
    let test = Loop::new();
    test.driver.round(|_| vec![complete(DONE, "  ")]);
    test.driver.round(|_| vec![complete(DONE, EVIDENCE)]);
    let created = test.conversation(&["demo.lookup"]);

    test.goal_turn(created, "Цель").await;

    let calls = test.harness.last(created).tool_calls;
    assert_eq!(calls[0].status, ChatToolCallStatus::Failed);
    assert_eq!(calls[0].error.as_deref(), Some(GOAL_EVIDENCE_REQUIRED));
    assert_eq!(calls[1].status, ChatToolCallStatus::Succeeded);
    let goal = test.goal_of(created);
    assert_eq!(goal.status, GoalStatus::Done);
    assert_eq!(goal.usage.rounds, 2);
}

#[tokio::test]
async fn a_goal_pauses_when_its_rounds_run_out_and_continues_with_a_renewed_budget() {
    let test = Loop::new();
    test.set(ROUNDS_KEY, "3");
    test.driver.fallback(|count| vec![lookup(count)]);
    let created = test.conversation(&["demo.lookup"]);

    let handle = test.goal_turn(created, "Цель").await;

    assert_eq!(test.harness.status(&handle), RunStatus::Succeeded);
    let goal = test.goal_of(created);
    assert_eq!(goal.status, GoalStatus::Paused);
    assert_eq!(goal.reason, Some(GoalReason::Rounds));
    assert_eq!(goal.usage.rounds, 3);
    assert_eq!(test.calls.lookups().len(), 3);
    let refused = test
        .harness
        .chat
        .stop_goal(&ConversationId::parse("0199aa11-2222-7111-8111-111111111111").expect("id"))
        .await;
    assert!(refused.is_err());

    test.driver.round(|_| vec![complete(DONE, EVIDENCE)]);
    let resumed = test
        .harness
        .chat
        .continue_goal(&created)
        .expect("continues");
    within(test.harness.runs.wait(&resumed.id)).await;

    let goal = test.goal_of(created);
    assert_eq!(goal.status, GoalStatus::Done);
    assert_eq!(goal.budget.rounds, 6);
    assert_eq!(goal.usage.rounds, 4);
    let user = test
        .harness
        .messages(created)
        .into_iter()
        .rev()
        .find(|message| message.role == zvs_contract::MessageRole::User)
        .expect("the continuation");
    assert_eq!(user.content, GOAL_CONTINUE_TEXT);
    assert_eq!(user.command, Some(ChatCommand::Goal));
    let again = test
        .harness
        .chat
        .continue_goal(&created)
        .expect_err("a done goal does not continue");
    assert_eq!(again.message, GOAL_NOT_PAUSED);
}

#[tokio::test]
async fn a_goal_pauses_when_its_tokens_run_out() {
    let test = Loop::new();
    test.set(TOKENS_KEY, "1000");
    test.driver.fallback(|count| {
        vec![
            lookup(count),
            TextDelta::Usage(TurnUsage {
                input_tokens: Some(900),
                output_tokens: Some(200),
                context_window: None,
            }),
        ]
    });
    let created = test.conversation(&["demo.lookup"]);

    test.goal_turn(created, "Цель").await;

    let goal = test.goal_of(created);
    assert_eq!(goal.status, GoalStatus::Paused);
    assert_eq!(goal.reason, Some(GoalReason::Tokens));
    assert_eq!(goal.usage.tokens, 1100);
    assert_eq!(goal.usage.rounds, 1);
}

#[tokio::test]
async fn a_goal_pauses_when_its_minutes_run_out() {
    let clock = Arc::new(FakeClock::new(1_000));
    let test = Loop::build_with(false, Some(Arc::clone(&clock) as SharedClock));
    test.set(MINUTES_KEY, "1");
    let ticking = Arc::clone(&clock);
    test.driver.fallback(move |count| {
        ticking.advance(25_000);
        vec![lookup(count)]
    });
    let created = test.conversation(&["demo.lookup"]);

    test.goal_turn(created, "Цель").await;

    let goal = test.goal_of(created);
    assert_eq!(goal.status, GoalStatus::Paused);
    assert_eq!(goal.reason, Some(GoalReason::Minutes));
    assert_eq!(goal.usage.rounds, 3);
    assert_eq!(goal.usage.elapsed_ms, 75_000);
}

#[tokio::test]
async fn repeating_the_same_call_stops_the_goal() {
    let test = Loop::new();
    test.driver
        .fallback(|_| vec![call("demo__lookup", &json!({ "query": "same" }))]);
    let created = test.conversation(&["demo.lookup"]);

    test.goal_turn(created, "Цель").await;

    let goal = test.goal_of(created);
    assert_eq!(goal.status, GoalStatus::Stopped);
    assert_eq!(goal.reason, Some(GoalReason::Repetition));
    assert_eq!(goal.usage.rounds, 3);
    assert!(goal.ended_at.is_some());
}

#[tokio::test]
async fn a_round_without_calls_is_nudged_instead_of_ending_the_goal() {
    let test = Loop::new();
    test.driver.round(|_| vec![text("План готов.")]);
    test.driver.round(|_| vec![complete(DONE, EVIDENCE)]);
    let created = test.conversation(&["demo.lookup"]);

    test.goal_turn(created, "Цель").await;

    let requests = test.driver.requests();
    assert_eq!(requests.len(), 2);
    let nudged = requests[1].messages.last().expect("a message");
    assert!(nudged.content.contains("Goal mode"));
    assert_eq!(test.goal_of(created).status, GoalStatus::Done);
}

#[tokio::test]
async fn messages_are_refused_while_a_goal_runs_and_stop_ends_it_within_the_round() {
    let test = Loop::new();
    test.calls.hang.store(true, Ordering::SeqCst);
    test.driver.fallback(|count| vec![lookup(count)]);
    let created = test.conversation(&["demo.lookup"]);
    let handle = test.start_goal(created, "Цель");
    eventually(|| test.calls.started.load(Ordering::SeqCst)).await;

    let refused = test
        .harness
        .chat
        .send(&ChatSendInput::new(created, "Ещё вопрос"))
        .expect_err("refused while the goal runs");
    assert_eq!(refused.code, AppErrorCode::Conflict);
    assert_eq!(refused.message, GOAL_RUNNING);

    let stopped = test.harness.chat.stop_goal(&created).await.expect("stops");
    within(test.harness.runs.wait(&handle.id)).await;

    assert_eq!(test.harness.status(&handle), RunStatus::Cancelled);
    let goal = stopped.goal.expect("a goal");
    assert_eq!(goal.status, GoalStatus::Stopped);
    assert_eq!(goal.reason, Some(GoalReason::User));
    assert_eq!(test.goal_of(created), goal);
    let again = test
        .harness
        .chat
        .stop_goal(&created)
        .await
        .expect_err("a stopped goal does not stop again");
    assert_eq!(again.message, GOAL_NOT_STOPPABLE);
    test.harness
        .chat
        .send(&ChatSendInput::new(created, "Теперь можно"))
        .expect("a message after the stop");
}

#[tokio::test]
async fn cancelling_the_turn_stops_the_goal_and_a_failed_turn_fails_it() {
    let test = Loop::new();
    test.calls.hang.store(true, Ordering::SeqCst);
    test.driver.fallback(|count| vec![lookup(count)]);
    let created = test.conversation(&["demo.lookup"]);
    let handle = test.start_goal(created, "Цель");
    eventually(|| test.calls.started.load(Ordering::SeqCst)).await;

    test.harness.chat.cancel(&handle.id).await.expect("cancels");
    within(test.harness.runs.wait(&handle.id)).await;
    eventually(|| test.goal_of(created).status != GoalStatus::Active).await;

    let goal = test.goal_of(created);
    assert_eq!(goal.status, GoalStatus::Stopped);
    assert_eq!(goal.reason, Some(GoalReason::User));
}

#[tokio::test]
async fn an_active_goal_reads_as_paused_after_a_restart_and_is_never_resumed() {
    let test = Loop::new();
    let created = test.conversation(&["demo.lookup"]);
    let id = created.to_string();
    test.harness
        .database
        .with_connection(|connection| {
            Ok(Repositories::new(connection).chat.update(
                &id,
                &ConversationPatch {
                    goal: Some(Some(ChatGoalDto::new(
                        "Цель",
                        zvs_contract::GoalBudgetDto::default(),
                        1,
                    ))),
                    ..ConversationPatch::default()
                },
            )?)
        })
        .expect("stores an active goal");

    assert_eq!(test.harness.chat.recover_goals().expect("recovers"), 1);
    assert_eq!(test.harness.chat.recover_goals().expect("recovers"), 0);

    let goal = test.goal_of(created);
    assert_eq!(goal.status, GoalStatus::Paused);
    assert_eq!(goal.reason, Some(GoalReason::Restart));
    assert!(test.driver.requests().is_empty());
}
