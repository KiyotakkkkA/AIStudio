use zvs_contract::{
    AssistantPreferencesDto, ChatCompactInput, ChatCompactionRef, ChatToolCallDto,
    ChatToolCallStatus, CompactionTrigger, MessageRole,
};

use super::*;
use crate::services::AssistantService;
use crate::services::chat::compact::{
    COMPACT_NOT_SMALLER, COMPACT_TOO_SHORT, COMPACTING, COMPACTION_NOT_LATEST, SUMMARY_HEADING,
    compacted_view, over_threshold,
};
use crate::testing::FakeScript;

const LONG: &str = "Подробное обсуждение сборки проекта, зависимостей и конфигурации окружения. ";

fn row(conversation: ConversationId, role: MessageRole, content: &str, at: i64) -> MessageRow {
    MessageRow {
        id: new_id().hyphenated().to_string(),
        conversation_id: conversation.to_string(),
        role,
        content: content.to_owned(),
        reasoning: String::new(),
        citations: Vec::new(),
        tool_calls: Vec::new(),
        images: Vec::new(),
        selections: Vec::new(),
        commands: Vec::new(),
        plugins: Vec::new(),
        offered_tools: Vec::new(),
        web_sources: Vec::new(),
        rounds: Vec::new(),
        tokens_in: 0,
        tokens_out: 0,
        tokens_cached: 0,
        tokens_reasoning: 0,
        usage_estimated: true,
        duration_ms: 0,
        run_id: None,
        partial: false,
        pinned: false,
        event: None,
        created_at: at,
        recap: None,
    }
}

fn seed(harness: &Harness, conversation: ConversationId, pairs: usize, repeat: usize) {
    let text = LONG.repeat(repeat);
    let base = harness.messages(conversation).len();
    harness
        .database
        .with_connection(|connection| {
            let repositories = Repositories::new(connection);
            for index in 0..pairs {
                let at = i64::try_from(base + index * 2 + 1).unwrap_or(1);
                repositories.chat.add_message(&row(
                    conversation,
                    MessageRole::User,
                    &format!("Вопрос {index}. {text}"),
                    at,
                ))?;
                repositories.chat.add_message(&row(
                    conversation,
                    MessageRole::Assistant,
                    &format!("Ответ {index}. {text}"),
                    at + 1,
                ))?;
            }
            Ok(())
        })
        .expect("seeds the history");
}

fn summary(harness: &Harness, text: &str) {
    harness.driver.script(FakeScript {
        deltas: Some(vec![text.to_owned()]),
        ..FakeScript::default()
    });
}

fn preferences(harness: &Harness, auto_compact: bool, recaps: bool) {
    AssistantService::new(Arc::clone(&harness.database), Arc::new(SystemClock))
        .set(&AssistantPreferencesDto {
            auto_compact,
            recaps,
            summarizer_models: Vec::new(),
        })
        .expect("stores the preferences");
}

fn compact(harness: &Harness, conversation: ConversationId) -> Result<RunHandleDto, AppError> {
    harness.chat.compact(&ChatCompactInput {
        conversation_id: conversation,
        instructions: None,
    })
}

fn detail(harness: &Harness, conversation: ConversationId) -> ConversationDetailDto {
    harness.chat.get(&conversation).expect("a conversation")
}

#[test]
fn the_view_replaces_messages_up_to_the_latest_boundary_with_one_summary() {
    let conversation = ConversationId::parse("0199aa11-2222-7111-8111-111111111111").expect("id");
    let messages: Vec<MessageDto> = (0..4)
        .map(|index| {
            message_dto(row(
                conversation,
                if index % 2 == 0 {
                    MessageRole::User
                } else {
                    MessageRole::Assistant
                },
                &format!("m{index}"),
                index,
            ))
            .expect("a message")
        })
        .collect();
    let plain = compacted_view(&messages, &[]);
    assert_eq!(plain.messages, messages);
    assert_eq!(plain.compacted(), 0);

    let compaction = zvs_contract::ChatCompactionDto {
        id: new_id().hyphenated().to_string(),
        boundary_message_id: messages[1].id,
        summary: "Итог первых двух сообщений".to_owned(),
        messages: 2,
        tokens_before: 100,
        tokens_after: 10,
        provider_id: "p".to_owned(),
        model_id: "m".to_owned(),
        trigger: CompactionTrigger::Manual,
        created_at: 5,
    };
    let view = compacted_view(&messages, std::slice::from_ref(&compaction));
    assert_eq!(view.messages.len(), 3);
    assert_eq!(view.messages[0].role, MessageRole::User);
    assert!(view.messages[0].content.starts_with(SUMMARY_HEADING));
    assert!(
        view.messages[0]
            .content
            .contains("Итог первых двух сообщений")
    );
    assert_eq!(view.messages[1..], messages[2..]);
    assert_eq!(view.covered, messages[..2]);
    assert_eq!(view.compacted(), 2);

    let orphan = zvs_contract::ChatCompactionDto {
        boundary_message_id: MessageId::parse("0199aa11-9999-7111-8111-111111111111").expect("id"),
        ..compaction
    };
    assert_eq!(compacted_view(&messages, &[orphan]).messages, messages);
}

#[test]
fn the_threshold_is_eighty_percent_of_the_budget() {
    assert!(!over_threshold(800, 1000));
    assert!(over_threshold(801, 1000));
    assert!(!over_threshold(10, 0));
}

#[tokio::test]
async fn compact_summarizes_the_dialog_and_later_turns_send_the_summary_instead() {
    let harness = Harness::with(HarnessOptions {
        context_window: 32_000,
        max_output: 1024,
        ..HarnessOptions::default()
    });
    preferences(&harness, false, false);
    let conversation = harness.conversation();
    seed(&harness, conversation, 3, 20);
    let last = harness.last(conversation).id;
    summary(
        &harness,
        "Пользователь настраивает сборку; решено использовать pnpm.",
    );

    let handle = compact(&harness, conversation).expect("starts");
    within(harness.runs.wait(&handle.id)).await;
    assert_eq!(harness.status(&handle), RunStatus::Succeeded);

    let after = detail(&harness, conversation);
    let [compaction] = after.compactions.as_slice() else {
        panic!("one compaction: {:?}", after.compactions);
    };
    assert_eq!(compaction.boundary_message_id, last);
    assert_eq!(compaction.messages, 6);
    assert_eq!(compaction.trigger, CompactionTrigger::Manual);
    assert!(compaction.tokens_after < compaction.tokens_before);
    assert_eq!(after.messages.len(), 6);

    let context = harness
        .chat
        .context(&ChatContextInput {
            conversation_id: Some(conversation),
            provider_id: harness.provider_id,
            model_id: harness.model_id.clone(),
            settings: None,
            system_prompt: None,
            commands: Vec::new(),
            plugins: Vec::new(),
        })
        .await
        .expect("measures");
    assert_eq!(context.compacted, 6);
    assert_eq!(context.history_messages, 1);

    summary(&harness, "Hello world");
    harness.turn(conversation, "Что дальше?").await;
    let request = harness.driver.requests().pop().expect("a request");
    let sent: Vec<&str> = request
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect();
    assert!(sent[0].starts_with(SUMMARY_HEADING));
    assert!(sent.iter().all(|text| !text.starts_with("Вопрос 0.")));
    assert_eq!(sent.last().copied(), Some("Что дальше?"));
}

#[tokio::test]
async fn a_short_dialog_is_refused_before_a_run_starts() {
    let harness = Harness::new();
    let conversation = harness.conversation();
    harness.turn(conversation, "Привет").await;
    let error = compact(&harness, conversation).expect_err("too short");
    assert_eq!(error.message, COMPACT_TOO_SHORT);
}

#[tokio::test]
async fn a_cancelled_compaction_leaves_the_dialog_untouched_and_idle() {
    let harness = Harness::with(HarnessOptions {
        context_window: 32_000,
        max_output: 1024,
        ..HarnessOptions::default()
    });
    let conversation = harness.conversation();
    seed(&harness, conversation, 3, 20);
    harness.driver.script(FakeScript {
        manual: Some(true),
        ..FakeScript::default()
    });
    let handle = compact(&harness, conversation).expect("starts");
    within(harness.driver.when_holding()).await;

    let busy = harness
        .chat
        .send(&ChatSendInput::new(conversation, "ещё"))
        .expect_err("busy");
    assert_eq!(busy.message, COMPACTING);
    let attached = harness
        .chat
        .attach(&conversation)
        .expect("attaches")
        .expect("a live compaction");
    assert!(attached.compacting);
    assert_eq!(attached.run.id, handle.id);

    harness.chat.cancel(&handle.id).await.expect("cancels");
    within(harness.runs.wait(&handle.id)).await;
    assert_eq!(harness.status(&handle), RunStatus::Cancelled);
    assert!(detail(&harness, conversation).compactions.is_empty());
    harness.driver.clear_manual();
    within(eventually(|| {
        harness.chat.attach(&conversation).expect("reads").is_none()
    }))
    .await;
    harness.turn(conversation, "ещё").await;
}

#[tokio::test]
async fn a_failed_or_useless_summary_is_never_stored() {
    let harness = Harness::with(HarnessOptions {
        context_window: 32_000,
        max_output: 1024,
        ..HarnessOptions::default()
    });
    let conversation = harness.conversation();
    seed(&harness, conversation, 3, 20);

    harness.driver.script(FakeScript {
        fail_with: Some(AppError::new(AppErrorCode::ProviderUnreachable, "offline")),
        ..FakeScript::default()
    });
    let failed = compact(&harness, conversation).expect("starts");
    within(harness.runs.wait(&failed.id)).await;
    assert_eq!(harness.status(&failed), RunStatus::Failed);
    assert!(detail(&harness, conversation).compactions.is_empty());
    harness.driver.clear_failure();

    summary(&harness, &LONG.repeat(400));
    let bloated = compact(&harness, conversation).expect("starts");
    within(harness.runs.wait(&bloated.id)).await;
    assert_eq!(harness.status(&bloated), RunStatus::Failed);
    let run = harness.runs.get(&bloated.id).expect("a run");
    assert_eq!(
        run.outcome.and_then(|outcome| outcome.message),
        Some(COMPACT_NOT_SMALLER.to_owned())
    );
    assert!(detail(&harness, conversation).compactions.is_empty());
}

#[tokio::test]
async fn only_the_latest_compaction_can_be_undone() {
    let harness = Harness::with(HarnessOptions {
        context_window: 32_000,
        max_output: 1024,
        ..HarnessOptions::default()
    });
    preferences(&harness, false, false);
    let conversation = harness.conversation();
    seed(&harness, conversation, 3, 20);
    summary(&harness, "Первая сводка.");
    let first = compact(&harness, conversation).expect("starts");
    within(harness.runs.wait(&first.id)).await;
    seed(&harness, conversation, 2, 20);
    summary(&harness, "Вторая сводка.");
    let second = compact(&harness, conversation).expect("starts");
    within(harness.runs.wait(&second.id)).await;

    let compactions = detail(&harness, conversation).compactions;
    assert_eq!(compactions.len(), 2);
    assert_eq!(compactions[1].messages, compactions[0].messages + 4);
    let older = harness
        .chat
        .undo_compaction(&ChatCompactionRef {
            conversation_id: conversation,
            compaction_id: compactions[0].id.clone(),
        })
        .expect_err("not the latest");
    assert_eq!(older.message, COMPACTION_NOT_LATEST);

    let undone = harness
        .chat
        .undo_compaction(&ChatCompactionRef {
            conversation_id: conversation,
            compaction_id: compactions[1].id.clone(),
        })
        .expect("undoes");
    assert_eq!(undone.compactions, compactions[..1]);
}

#[tokio::test]
async fn a_full_context_is_compacted_automatically_before_the_turn() {
    let harness = Harness::with(HarnessOptions {
        context_window: 4096,
        max_output: 256,
        ..HarnessOptions::default()
    });
    preferences(&harness, true, false);
    let conversation = harness.conversation();
    seed(&harness, conversation, 4, 20);
    summary(&harness, "Hello world");

    harness.turn(conversation, "Продолжим").await;
    let after = detail(&harness, conversation);
    let [compaction] = after.compactions.as_slice() else {
        panic!("one automatic compaction: {:?}", after.compactions);
    };
    assert_eq!(compaction.trigger, CompactionTrigger::Auto);
    assert_eq!(compaction.messages, 8);
    let request = harness.driver.requests().pop().expect("a request");
    assert!(request.messages[0].content.starts_with(SUMMARY_HEADING));
    assert_eq!(
        request
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("Продолжим")
    );
    assert_eq!(harness.last(conversation).content, "Hello world");
}

#[tokio::test]
async fn automatic_compaction_stays_off_when_the_preference_is_off() {
    let harness = Harness::with(HarnessOptions {
        context_window: 4096,
        max_output: 256,
        ..HarnessOptions::default()
    });
    preferences(&harness, false, false);
    let conversation = harness.conversation();
    seed(&harness, conversation, 4, 20);
    harness.turn(conversation, "Продолжим").await;
    assert!(detail(&harness, conversation).compactions.is_empty());
}

#[tokio::test]
async fn a_recap_is_written_after_a_turn_with_tool_calls_only() {
    let harness = Harness::new();
    preferences(&harness, false, true);
    let conversation = harness.conversation();
    harness.turn(conversation, "Привет").await;
    let plain = harness.last(conversation);
    harness
        .chat
        .schedule_recap(&conversation.to_string(), &plain.id.to_string(), false);

    let mut reply = row(conversation, MessageRole::Assistant, "Готово", 100);
    reply.tool_calls = vec![ChatToolCallDto {
        id: "call_1".to_owned(),
        key: "fs.apply".to_owned(),
        name: "fs__apply".to_owned(),
        round: 0,
        arguments: json!({ "path": "a.txt" }),
        status: ChatToolCallStatus::Succeeded,
        result: Some("written".to_owned()),
        truncated: false,
        error: None,
        approval_id: None,
        step_id: None,
        duration_ms: 1,
        meta: None,
        authorisation: None,
    }];
    let reply_id = reply.id.clone();
    harness
        .database
        .with_connection(|connection| {
            Repositories::new(connection).chat.add_message(&reply)?;
            Ok(())
        })
        .expect("adds a reply");
    summary(&harness, "Создан файл a.txt.");
    harness
        .chat
        .schedule_recap(&conversation.to_string(), &reply_id, false);

    within(eventually(|| {
        harness
            .messages(conversation)
            .iter()
            .any(|message| message.recap.is_some())
    }))
    .await;
    let messages = harness.messages(conversation);
    let recaps: Vec<(&str, &str)> = messages
        .iter()
        .filter_map(|message| {
            message
                .recap
                .as_deref()
                .map(|recap| (message.content.as_str(), recap))
        })
        .collect();
    assert_eq!(recaps, [("Готово", "Создан файл a.txt.")]);
    assert!(harness.events().iter().any(|event| matches!(
        &event.body,
        HostEventDraft::Step { step }
            if step.get("domain").and_then(Value::as_str) == Some(zvs_contract::RECAP_STEP_DOMAIN)
    )));
}

#[tokio::test]
async fn recaps_stay_off_when_the_preference_is_off() {
    let harness = Harness::new();
    preferences(&harness, true, false);
    let conversation = harness.conversation();
    let mut reply = row(conversation, MessageRole::Assistant, "Готово", 100);
    reply.tool_calls = vec![
        serde_json::from_value(json!({
            "id": "call_1", "key": "fs.apply", "name": "fs__apply", "round": 0,
            "arguments": {}, "status": "succeeded", "result": "ok", "truncated": false,
            "error": null, "approvalId": null, "stepId": null, "durationMs": 1, "meta": null
        }))
        .expect("a call"),
    ];
    let reply_id = reply.id.clone();
    harness
        .database
        .with_connection(|connection| {
            Repositories::new(connection).chat.add_message(&reply)?;
            Ok(())
        })
        .expect("adds a reply");
    harness
        .chat
        .schedule_recap(&conversation.to_string(), &reply_id, false);
    let runs = harness
        .runs
        .list(&RunListFilter::default())
        .expect("lists runs");
    assert!(runs.items.iter().all(|run| run.kind != RunKind::Job));
}
