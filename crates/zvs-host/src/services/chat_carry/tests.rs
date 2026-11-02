use serde_json::{Value, json};
use zvs_contract::{ChatCarriedKind, ChatCarriedStatus, MessageDto};

use super::*;
use crate::drivers::ai::tool_dialect::INJECTED_RESULT_PREFIX;
use crate::services::chat_history::window_chat_history;
use crate::services::chat_prompt::{model_history, sendable_messages};

const CONVERSATION: &str = "0199aa11-1111-7111-8111-111111111111";
const STORE: &str = "0199bb11-1111-7111-8111-00000000000a";

fn id(index: u32) -> String {
    format!("0199aa11-2222-7222-8222-{index:012}")
}

fn message(index: u32, role: &str, content: &str, extra: Value) -> MessageDto {
    let mut value = json!({
        "id": id(index),
        "conversationId": CONVERSATION,
        "role": role,
        "content": content,
        "citations": [],
        "tokensIn": 0,
        "tokensOut": 0,
        "usageEstimated": true,
        "durationMs": 0,
        "runId": null,
        "partial": false,
        "createdAt": index,
    });
    if let (Some(target), Value::Object(fields)) = (value.as_object_mut(), extra) {
        target.extend(fields);
    }
    serde_json::from_value(value).expect("a message")
}

fn call(call_id: &str, status: &str, result: Option<&str>) -> Value {
    json!({
        "id": call_id,
        "key": "fs.read",
        "name": "fs__read",
        "round": 0,
        "arguments": { "path": "notes.md" },
        "status": status,
        "result": result,
        "error": if status == "failed" { json!("Файл не найден") } else { Value::Null },
    })
}

fn citation(path: &str, chunk: u64) -> Value {
    json!({
        "storeId": STORE,
        "documentId": path,
        "sourcePath": format!("docs/{path}"),
        "chunkIndex": chunk,
        "score": 0.8,
        "section": "",
        "page": 2,
    })
}

fn lookup() -> SourceLookup {
    let store = VectorStoreId::parse(STORE).expect("a store");
    SourceLookup {
        excerpts: HashMap::from([
            (
                (store, "a.md".to_owned(), 1),
                "Отпуск длится 28 дней.".to_owned(),
            ),
            (
                (store, "b.md".to_owned(), 4),
                "Перенос по заявлению.".to_owned(),
            ),
        ]),
        stores: HashMap::from([(store, "Кадры".to_owned())]),
    }
}

fn answer_with_work() -> MessageDto {
    message(
        2,
        "assistant",
        "Отпуск 28 дней [2]. Подробнее [[1]].",
        json!({
            "reasoning": "думаю долго",
            "citations": [citation("a.md", 1), citation("b.md", 4)],
            "toolCalls": [call("call_a", "succeeded", Some("line one\nline two"))],
            "webSources": [{ "url": "https://example.com/leave", "title": "Отпуска", "snippet": "Порядок отпусков" }],
        }),
    )
}

#[test]
fn tool_records_and_cited_sources_are_carried_but_uncited_sources_and_reasoning_are_not() {
    let answer = answer_with_work();
    let carried = Carried::of(&answer, &lookup());
    let expanded = carried.expand(
        ChatMessage::assistant(answer.content.clone()),
        CarryShape::Native,
    );
    assert_eq!(expanded.len(), 4);
    let sources = &expanded[0];
    assert_eq!(sources.role, ChatRole::User);
    assert!(sources.content.starts_with(CARRIED_SOURCES));
    assert!(
        sources
            .content
            .contains("marker=\"[2]\" title=\"b.md\" store=\"Кадры\"")
    );
    assert!(sources.content.contains("Перенос по заявлению."));
    assert!(
        sources
            .content
            .contains("marker=\"[[1]]\" title=\"Отпуска\" url=\"https://example.com/leave\"")
    );
    assert!(!sources.content.contains("a.md"));
    assert_eq!(expanded[1].tool_calls[0].name, "fs__read");
    assert_eq!(
        expanded[1].tool_calls[0].arguments,
        r#"{"path":"notes.md"}"#
    );
    assert!(expanded[2].content.contains("line one\nline two"));
    assert_eq!(expanded[3], ChatMessage::assistant(answer.content.clone()));
    assert!(
        expanded
            .iter()
            .all(|message| !message.content.contains("думаю"))
    );
}

#[test]
fn markers_inside_code_links_and_out_of_range_numbers_are_ignored() {
    let (store, web) =
        cited_markers("См. [1], [3](http://x) и `x` ```\n[2]\n``` [[2]] [1][4] [5]:");
    assert_eq!(store, [1, 4]);
    assert_eq!(web, [2]);
    let answer = message(
        2,
        "assistant",
        "[7] и [1]",
        json!({ "citations": [citation("a.md", 1)] }),
    );
    assert_eq!(cited_citations(&answer).len(), 1);
}

#[test]
fn citations_from_tool_results_follow_the_message_citations_in_numbering() {
    let answer = message(
        2,
        "assistant",
        "Ответ [2].",
        json!({
            "citations": [citation("a.md", 1)],
            "toolCalls": [{
                "id": "call_k", "key": "knowledge.search", "name": "knowledge_search", "round": 0,
                "arguments": {}, "status": "succeeded", "result": "found",
                "meta": { "citations": [citation("a.md", 1), citation("b.md", 4)] },
            }],
        }),
    );
    let cited = cited_citations(&answer);
    assert_eq!(cited.len(), 1);
    assert_eq!(cited[0].document_id, "b.md");
}

#[test]
fn native_pairing_keeps_every_call_id_with_exactly_one_result() {
    let answer = message(
        2,
        "assistant",
        "Готово.",
        json!({ "toolCalls": [
            call("call_a", "succeeded", Some("alpha")),
            call("call_b", "failed", None),
        ]}),
    );
    let mut carried = [Carried::of(&answer, &SourceLookup::default())];
    carried[0].shed(Stage::Shorten);
    let expanded = carried[0].expand(ChatMessage::assistant("Готово."), CarryShape::Native);
    let ids: Vec<&str> = expanded[0]
        .tool_calls
        .iter()
        .map(|call| call.id.as_str())
        .collect();
    let results: Vec<&str> = expanded
        .iter()
        .filter(|message| message.role == ChatRole::Tool)
        .filter_map(|message| message.tool_call_id.as_deref())
        .collect();
    assert_eq!(ids, ["call_a", "call_b"]);
    assert_eq!(results, ids);
    assert!(expanded[1].content.contains(SHORTENED_NOTE));
    assert!(expanded[1].content.contains("alpha"));
    assert_eq!(
        expanded[2].content,
        "<tool_result name=\"fs__read\" status=\"failed\">\n\n</tool_result>"
    );
    assert!(!expanded[2].content.contains("Файл"));
}

#[test]
fn native_pairing_survives_the_step_from_prepare_to_generate() {
    use crate::kernel::nodes::{GenerateMessage, GenerateNodeInput};
    use zvs_contract::Validate;
    let answer = answer_with_work();
    let mut expanded = vec![ChatMessage::user("Сколько дней отпуска?")];
    expanded.extend(Carried::of(&answer, &lookup()).expand(
        ChatMessage::assistant(answer.content.clone()),
        CarryShape::Native,
    ));
    let input: GenerateNodeInput = serde_json::from_value(json!({
        "providerId": "p",
        "model": "m",
        "messages": serde_json::to_value(
            expanded.iter().map(GenerateMessage::from_chat).collect::<Vec<_>>()
        ).expect("serialises"),
    }))
    .expect("deserialises");
    input.validate().expect("paired");
    assert_eq!(input.request().messages, expanded);
}

#[test]
fn text_dialects_get_the_calls_as_a_text_block_without_tool_messages() {
    let answer = answer_with_work();
    let expanded = Carried::of(&answer, &lookup()).expand(
        ChatMessage::assistant(answer.content.clone()),
        CarryShape::Text,
    );
    assert!(expanded.iter().all(|message| message.role != ChatRole::Tool
        && message.tool_calls.is_empty()
        && message.tool_call_id.is_none()));
    let call = expanded
        .iter()
        .find(|message| {
            message.role == ChatRole::Assistant && message.content.contains("zvs_call_tool")
        })
        .expect("the call block");
    assert!(call.content.contains("\"zvs_call_tool\":\"fs__read\""));
    assert!(
        expanded
            .iter()
            .any(|message| message.content.starts_with(INJECTED_RESULT_PREFIX))
    );
    assert_eq!(
        expanded.last(),
        Some(&ChatMessage::assistant(answer.content.clone()))
    );
}

#[test]
fn a_result_over_the_cap_is_truncated_and_marked() {
    let long = "строка ".repeat(3_000);
    let answer = message(
        2,
        "assistant",
        "Ок.",
        json!({ "toolCalls": [call("call_a", "succeeded", Some(&long))] }),
    );
    let carried = Carried::of(&answer, &SourceLookup::default());
    let parts = carried.parts();
    assert!(parts[0].truncated);
    assert!(parts[0].tokens <= RESULT_CAP_TOKENS + 200);
    let expanded = carried.expand(ChatMessage::assistant("Ок."), CarryShape::Native);
    assert!(expanded[1].content.contains(TRUNCATED_NOTE));
}

fn conversation() -> Vec<MessageDto> {
    let result = "данные ".repeat(300);
    vec![
        message(1, "user", "Прочитай первый", json!({})),
        message(
            2,
            "assistant",
            "Первый прочитан.",
            json!({ "toolCalls": [call("call_1", "succeeded", Some(&result))] }),
        ),
        message(3, "event", "Процесс завершён", json!({})),
        message(
            4,
            "assistant",
            "Второй прочитан [1].",
            json!({
                "citations": [citation("a.md", 1)],
                "toolCalls": [call("call_2", "succeeded", Some(&result))],
            }),
        ),
    ]
}

fn prepare(messages: &[MessageDto], window: i64) -> (PreparedHistory, HashMap<MessageId, Carried>) {
    let history = model_history(messages);
    let sent = sendable_messages(messages);
    let mut prepared = window_chat_history(
        "system",
        &history,
        &ChatMessage::user("Что дальше?"),
        window,
        100,
        &[],
        &[],
    )
    .expect("fits");
    let carried = carry_into(
        &mut prepared,
        &sent,
        &lookup(),
        CarryShape::Native,
        window - 100,
    );
    (prepared, carried)
}

fn statuses(carried: &HashMap<MessageId, Carried>, index: u32) -> Vec<ChatCarriedStatus> {
    let key = MessageId::parse(&id(index)).expect("an id");
    carried
        .get(&key)
        .map(Carried::parts)
        .unwrap_or_default()
        .iter()
        .map(|part| part.status)
        .collect()
}

#[test]
fn everything_is_carried_when_the_window_is_large_and_event_messages_stay_user() {
    let messages = conversation();
    let (prepared, carried) = prepare(&messages, 100_000);
    assert_eq!(prepared.trimmed, 0);
    let roles: Vec<ChatRole> = prepared
        .messages
        .iter()
        .map(|message| message.role)
        .collect();
    assert_eq!(
        roles,
        [
            ChatRole::User,
            ChatRole::Assistant,
            ChatRole::Tool,
            ChatRole::Assistant,
            ChatRole::User,
            ChatRole::User,
            ChatRole::Assistant,
            ChatRole::Tool,
            ChatRole::Assistant,
            ChatRole::User,
        ]
    );
    assert_eq!(prepared.messages[4].content, "Процесс завершён");
    assert_eq!(statuses(&carried, 2), [ChatCarriedStatus::Sent]);
    assert_eq!(
        statuses(&carried, 4),
        [ChatCarriedStatus::Sent, ChatCarriedStatus::Sent]
    );
    assert_eq!(
        prepared.tokens_in,
        messages_cost(&prepared.messages) + tokens("system") + MESSAGE_OVERHEAD
    );
}

fn levels(carried: &[Carried]) -> Vec<Vec<ChatCarriedStatus>> {
    carried
        .iter()
        .map(|entry| entry.parts().iter().map(|part| part.status).collect())
        .collect()
}

fn answers() -> Vec<Carried> {
    conversation()
        .iter()
        .filter(|message| message.role == MessageRole::Assistant)
        .map(|message| Carried::of(message, &lookup()))
        .collect()
}

fn total(carried: &[Carried]) -> i64 {
    carried
        .iter()
        .map(|entry| entry.cost(CarryShape::Native))
        .sum()
}

#[test]
fn shedding_shortens_results_oldest_first_then_drops_records_then_sources() {
    use ChatCarriedStatus::{Dropped, Sent, Shortened};
    let mut carried = answers();
    let full = total(&carried);
    assert_eq!(fit(&mut carried, full, CarryShape::Native), full);
    assert_eq!(levels(&carried), [vec![Sent], vec![Sent, Sent]]);

    let mut carried = answers();
    let used = fit(&mut carried, full - 1, CarryShape::Native);
    assert!(used < full);
    assert_eq!(levels(&carried), [vec![Shortened], vec![Sent, Sent]]);

    let mut shortened = answers();
    shortened.iter_mut().for_each(|entry| {
        entry.shed(Stage::Shorten);
    });
    let mut carried = answers();
    fit(&mut carried, total(&shortened) - 1, CarryShape::Native);
    assert_eq!(levels(&carried), [vec![Dropped], vec![Shortened, Sent]]);

    let mut sources_only = answers();
    sources_only.iter_mut().for_each(|entry| {
        entry.shed(Stage::DropCalls);
    });
    let room = total(&sources_only);
    let mut carried = answers();
    assert_eq!(fit(&mut carried, room, CarryShape::Native), room);
    assert_eq!(levels(&carried), [vec![Dropped], vec![Dropped, Sent]]);

    let mut carried = answers();
    assert_eq!(fit(&mut carried, 0, CarryShape::Native), 0);
    assert_eq!(levels(&carried), [vec![Dropped], vec![Dropped, Dropped]]);
}

#[test]
fn messages_are_never_dropped_to_make_room_for_carried_parts() {
    let messages = conversation();
    let bare = {
        let history = model_history(&messages);
        window_chat_history(
            "system",
            &history,
            &ChatMessage::user("Что дальше?"),
            100_000,
            100,
            &[],
            &[],
        )
        .expect("fits")
        .tokens_in
    };
    let (tight, carried) = prepare(&messages, bare + 100);
    assert_eq!(tight.trimmed, 0);
    assert_eq!(
        statuses(&carried, 4),
        [ChatCarriedStatus::Dropped, ChatCarriedStatus::Dropped]
    );
    assert_eq!(tight.tokens_in, bare);
}

#[test]
fn carried_parts_never_take_more_than_their_share_of_the_history_room() {
    let messages = conversation();
    let (prepared, _) = prepare(&messages, 1_400);
    assert!(prepared.tokens_in <= 1_300);
    let carried_cost = prepared.tokens_in
        - messages_cost(&model_history(&messages)[prepared.trimmed..])
        - tokens("system")
        - MESSAGE_OVERHEAD
        - messages_cost(&[ChatMessage::user("Что дальше?")]);
    let room = 1_300
        - tokens("system")
        - MESSAGE_OVERHEAD
        - messages_cost(&[ChatMessage::user("Что дальше?")]);
    assert!(carried_cost * 100 <= room * CARRY_SHARE_PERCENT);
}

#[test]
fn parts_report_kinds_labels_and_the_marker_of_each_source() {
    let answer = answer_with_work();
    let parts = Carried::of(&answer, &lookup()).parts();
    assert_eq!(
        parts
            .iter()
            .map(|part| (part.kind, part.label.as_str(), part.marker.as_str()))
            .collect::<Vec<_>>(),
        [
            (ChatCarriedKind::ToolCall, "fs.read", ""),
            (ChatCarriedKind::Source, "b.md", "[2]"),
            (ChatCarriedKind::Source, "Отпуска", "[[1]]"),
        ]
    );
    assert!(
        parts
            .iter()
            .all(|part| part.status == ChatCarriedStatus::Sent && part.tokens > 0)
    );
    assert!(Carried::of(&message(1, "user", "[1]", json!({})), &lookup()).is_empty());
}
