use serde_json::json;
use zvs_contract::{
    AppErrorCode, CHAT_QUEUE_MAX, ChatCommand, ChatQueueAddInput, ChatQueueDto, ChatQueueEditInput,
    ChatQueueInput, ChatQueueRemoveInput, ChatSendInput, ConversationId, GoalStatus,
    HostEventDraft, MessageRole, QUEUE_STEP_DOMAIN, RunStatus, ToolTier,
};

use super::{Loop, awaiting_approval, call, text, within};
use crate::data::Repositories;
use crate::drivers::ai::{ChatMessage, ChatRole};
use crate::services::chat::tests::eventually;
use crate::services::chat::{QUEUE_FULL, QUEUE_MESSAGE_NOT_FOUND, QUEUE_YIELDED};
use crate::services::chat_goal::GOAL_TOOL_NAME;

impl Loop {
    fn unfinished(&self) -> usize {
        self.harness
            .database
            .with_connection(|connection| Ok(Repositories::new(connection).runs.unfinished()?))
            .expect("runs")
            .len()
    }

    fn add(&self, conversation: ConversationId, value: &str) -> ChatQueueDto {
        self.harness
            .chat
            .queue_add(&ChatQueueAddInput {
                conversation_id: conversation,
                text: value.to_owned(),
            })
            .expect("queues")
    }

    fn queued(&self, conversation: ConversationId) -> ChatQueueDto {
        self.harness
            .chat
            .queue(&ChatQueueInput {
                conversation_id: conversation,
            })
            .expect("a queue")
    }

    fn texts(&self, conversation: ConversationId) -> Vec<(MessageRole, String)> {
        self.harness
            .messages(conversation)
            .into_iter()
            .map(|message| (message.role, message.content))
            .collect()
    }

    fn blocked_turn(&self) -> (ConversationId, zvs_contract::RunHandleDto) {
        self.set_tier(&self.drop, ToolTier::Auto);
        self.driver
            .round(|_| vec![text("Checking."), call("demo__drop", &json!({}))]);
        self.driver.fallback(|_| vec![text("Done.")]);
        let created = self.conversation(&["demo.drop"]);
        let handle = self.harness.send(created, "Drop it");
        (created, handle)
    }
}

fn queue_steps(test: &Loop) -> Vec<serde_json::Map<String, serde_json::Value>> {
    test.harness
        .events()
        .into_iter()
        .filter_map(|event| match event.body {
            HostEventDraft::Step { step }
                if step.get("domain") == Some(&json!(QUEUE_STEP_DOMAIN)) =>
            {
                Some(step)
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn messages_queued_during_a_turn_start_one_turn_when_it_ends() {
    let test = Loop::new();
    let (created, handle) = test.blocked_turn();
    let approval = within(awaiting_approval(&test, created)).await;

    test.add(created, "Первое");
    let queue = test.add(created, "Второе");
    assert_eq!(
        queue
            .messages
            .iter()
            .map(|message| message.text.as_str())
            .collect::<Vec<_>>(),
        ["Первое", "Второе"]
    );
    assert!(!queue.urgent);
    assert_eq!(queue_steps(&test).len(), 2);
    assert_eq!(test.unfinished(), 1);

    test.harness
        .runs
        .approve(&handle.id, &approval, false)
        .expect("approves");
    within(test.harness.runs.wait(&handle.id)).await;
    within(eventually(|| test.harness.messages(created).len() == 5)).await;
    within(eventually(|| test.unfinished() == 0)).await;

    assert_eq!(
        test.texts(created),
        [
            (MessageRole::User, "Drop it".to_owned()),
            (MessageRole::Assistant, "Checking.\n\nDone.".to_owned()),
            (MessageRole::User, "Первое".to_owned()),
            (MessageRole::User, "Второе".to_owned()),
            (MessageRole::Assistant, "Done.".to_owned()),
        ]
    );
    let last = test.driver.requests().pop().expect("a request").messages;
    let tail: Vec<&ChatMessage> = last.iter().rev().take(2).collect();
    assert_eq!(tail[1], &ChatMessage::user("Первое"));
    assert_eq!(tail[0], &ChatMessage::user("Второе"));
    assert!(test.queued(created).messages.is_empty());
}

#[tokio::test]
async fn sending_now_ends_the_turn_at_the_next_safepoint_and_continues_with_the_queue() {
    let test = Loop::new();
    let (created, handle) = test.blocked_turn();
    let approval = within(awaiting_approval(&test, created)).await;

    test.add(created, "Хватит, сделай иначе");
    let flushed = test
        .harness
        .chat
        .queue_flush(&ChatQueueInput {
            conversation_id: created,
        })
        .expect("flushes");
    assert!(flushed.urgent);
    assert_eq!(flushed.messages.len(), 1);

    test.harness
        .runs
        .approve(&handle.id, &approval, false)
        .expect("approves");
    within(test.harness.runs.wait(&handle.id)).await;
    within(eventually(|| test.harness.messages(created).len() == 4)).await;
    within(eventually(|| test.unfinished() == 0)).await;

    assert_eq!(
        test.texts(created),
        [
            (MessageRole::User, "Drop it".to_owned()),
            (MessageRole::Assistant, "Checking.".to_owned()),
            (MessageRole::User, "Хватит, сделай иначе".to_owned()),
            (MessageRole::Assistant, "Done.".to_owned()),
        ]
    );
    assert_eq!(test.calls.drops(), 1);
    assert!(test.harness.events().iter().any(|event| matches!(
        &event.body,
        HostEventDraft::Log { line } if line.get("message") == Some(&json!(QUEUE_YIELDED))
    )));
    let requests = test.driver.requests();
    assert_eq!(requests.len(), 2);
    let carried = &requests[1].messages;
    assert!(
        carried
            .iter()
            .any(|message| message.role == ChatRole::Tool && message.content.contains("dropped"))
    );
    assert_eq!(
        carried.last(),
        Some(&ChatMessage::user("Хватит, сделай иначе"))
    );
}

#[tokio::test]
async fn a_message_queued_while_idle_is_sent_at_once() {
    let test = Loop::new();
    test.driver.fallback(|_| vec![text("Ответ.")]);
    let created = test.conversation(&[]);
    let queue = test.add(created, "Привет");
    assert!(queue.messages.is_empty());
    within(eventually(|| test.harness.messages(created).len() == 2)).await;
    within(eventually(|| test.unfinished() == 0)).await;
    assert_eq!(
        test.texts(created),
        [
            (MessageRole::User, "Привет".to_owned()),
            (MessageRole::Assistant, "Ответ.".to_owned()),
        ]
    );
}

#[tokio::test]
async fn queued_messages_can_be_edited_and_removed_and_the_queue_is_capped() {
    let test = Loop::new();
    let (created, handle) = test.blocked_turn();
    within(awaiting_approval(&test, created)).await;

    let queue = test.add(created, "Черновик");
    let id = queue.messages[0].id.clone();
    let edited = test
        .harness
        .chat
        .queue_edit(&ChatQueueEditInput {
            conversation_id: created,
            message_id: id.clone(),
            text: "  Исправлено ".to_owned(),
        })
        .expect("edits");
    assert_eq!(edited.messages[0].text, "Исправлено");

    for index in 1..CHAT_QUEUE_MAX {
        test.add(created, &format!("Ещё {index}"));
    }
    let full = test
        .harness
        .chat
        .queue_add(&ChatQueueAddInput {
            conversation_id: created,
            text: "Лишнее".to_owned(),
        })
        .expect_err("full");
    assert_eq!(full.code, AppErrorCode::Conflict);
    assert_eq!(full.message, QUEUE_FULL);

    let removed = test
        .harness
        .chat
        .queue_remove(&ChatQueueRemoveInput {
            conversation_id: created,
            message_id: id.clone(),
        })
        .expect("removes");
    assert_eq!(removed.messages.len(), CHAT_QUEUE_MAX - 1);
    let missing = test
        .harness
        .chat
        .queue_remove(&ChatQueueRemoveInput {
            conversation_id: created,
            message_id: id,
        })
        .expect_err("gone");
    assert_eq!(missing.code, AppErrorCode::NotFound);
    assert_eq!(missing.message, QUEUE_MESSAGE_NOT_FOUND);

    within(test.harness.chat.cancel(&handle.id))
        .await
        .expect("cancels");
    assert_eq!(test.harness.status(&handle), RunStatus::Cancelled);
}

#[tokio::test]
async fn a_cancelled_turn_keeps_the_queue_until_it_is_sent_by_hand() {
    let test = Loop::new();
    let (created, handle) = test.blocked_turn();
    within(awaiting_approval(&test, created)).await;
    test.add(created, "Потом");

    within(test.harness.chat.cancel(&handle.id))
        .await
        .expect("cancels");
    within(eventually(|| test.unfinished() == 0)).await;
    let kept = test.queued(created);
    assert_eq!(kept.messages.len(), 1);
    assert!(!kept.urgent);
    assert_eq!(test.harness.messages(created).len(), 2);

    let flushed = test
        .harness
        .chat
        .queue_flush(&ChatQueueInput {
            conversation_id: created,
        })
        .expect("sends");
    assert!(flushed.messages.is_empty());
    within(eventually(|| test.harness.messages(created).len() == 4)).await;
    assert_eq!(
        test.texts(created)[2],
        (MessageRole::User, "Потом".to_owned())
    );
}

#[tokio::test]
async fn sending_now_during_a_goal_keeps_the_goal_and_continues_it_with_the_queue() {
    let test = Loop::new();
    test.set_tier(&test.drop, ToolTier::Auto);
    test.driver.round(|_| vec![call("demo__drop", &json!({}))]);
    test.driver.round(|_| {
        vec![call(
            GOAL_TOOL_NAME,
            &json!({ "summary": "Готово", "evidence": "dropped" }),
        )]
    });
    test.driver.fallback(|_| vec![text("Цель выполнена.")]);
    let created = test.conversation(&["demo.drop"]);
    let handle = test
        .harness
        .chat
        .send(&ChatSendInput {
            command: Some(ChatCommand::Goal),
            ..ChatSendInput::new(created, "Удали лишнее")
        })
        .expect("starts a goal");
    let approval = within(awaiting_approval(&test, created)).await;
    test.add(created, "Только временные файлы");
    test.harness
        .chat
        .queue_flush(&ChatQueueInput {
            conversation_id: created,
        })
        .expect("flushes");
    test.harness
        .runs
        .approve(&handle.id, &approval, false)
        .expect("approves");
    within(test.harness.runs.wait(&handle.id)).await;
    within(eventually(|| {
        test.harness
            .chat
            .get(&created)
            .ok()
            .and_then(|detail| detail.conversation.goal)
            .is_some_and(|goal| goal.status == GoalStatus::Done)
    }))
    .await;

    let messages = test.harness.messages(created);
    let queued = messages
        .iter()
        .find(|message| message.content == "Только временные файлы")
        .expect("the queued message");
    assert_eq!(queued.role, MessageRole::User);
    assert_eq!(queued.command, Some(ChatCommand::Goal));
    let requests = test.driver.requests();
    assert!(
        requests[1]
            .tools
            .iter()
            .any(|tool| tool.name == GOAL_TOOL_NAME)
    );
}
