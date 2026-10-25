use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::drivers::ai::mock::{FakeTextDriver, search_tool, tool_request};

pub(crate) fn read_tool() -> ToolSpec {
    ToolSpec {
        name: "fs__read".to_owned(),
        description: "Read a file".to_owned(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"],
        }),
    }
}

pub(crate) fn counted_ids() -> CallIds {
    let next = Arc::new(AtomicUsize::new(0));
    Arc::new(move || format!("call_{}", next.fetch_add(1, Ordering::SeqCst) + 1))
}

pub(crate) async fn drain(stream: DeltaStream) -> Vec<TextDelta> {
    stream
        .map(|delta| delta.expect("a delta"))
        .collect::<Vec<_>>()
        .await
}

fn call(name: &str, arguments: &str) -> InjectedParse {
    InjectedParse::Call {
        name: name.to_owned(),
        arguments: arguments.to_owned(),
    }
}

fn parser() -> ReplyParser {
    DefaultDialect.parse_stream(&[search_tool(), read_tool()])
}

fn parse(text: &str) -> Vec<InjectedParse> {
    parser().parse(text).calls
}

fn buffer() -> ReplyParser {
    DefaultDialect.parse_stream(&[])
}

fn could_be_marker(tail: &str) -> bool {
    DEFAULT_GRAMMAR.could_be_marker(tail)
}

#[test]
fn a_marked_object_is_a_call_fenced_bare_or_after_a_sentence() {
    assert_eq!(
        parse(&format!(
            r#"{{"{INJECTED_CALL_KEY}": "docs__search", "arguments": {{"query": "a"}}}}"#
        )),
        [call("docs__search", r#"{"query":"a"}"#)]
    );
    assert_eq!(
        parse(&format!(
            "```json\n{{\"{INJECTED_CALL_KEY}\": \"docs__search\", \"arguments\": {{}}}}\n```"
        )),
        [call("docs__search", "{}")]
    );
    assert_eq!(
        parse(&format!(r#"{{"{INJECTED_CALL_KEY}": " docs__search "}}"#)),
        [call("docs__search", "{}")]
    );
    let prose = parser().parse(&format!(
        "Сначала найду в документации.\n```json\n{{\"{INJECTED_CALL_KEY}\": \"docs__search\", \"arguments\": {{\"query\": \"a\"}}}}\n```\nЖду результат."
    ));
    assert_eq!(prose.text, "Сначала найду в документации.");
    assert_eq!(prose.calls, [call("docs__search", r#"{"query":"a"}"#)]);
    for text in [
        "Plain answer.",
        r#"{"name": "docs__search", "arguments": {}}"#,
        r#"<tool_call>{"name": "docs__search", "arguments": {}}</tool_call>"#,
        "[1, 2]",
        "Use `{ \"a\": 1 }` in the config.",
    ] {
        let outcome = parser().parse(text);
        assert!(outcome.calls.is_empty(), "{text}");
        assert_eq!(outcome.text, text);
    }
    assert_eq!(
        buffer()
            .parse(&format!(r#"{{"{INJECTED_CALL_KEY}": "any"}}"#))
            .calls,
        [call("any", "{}")]
    );
}

#[test]
fn a_near_miss_resolves_to_the_offered_tool_and_the_default_has_no_aliases() {
    assert_eq!(
        parse(&format!(
            r#"{{"{INJECTED_CALL_KEY}": "docs.search", "arguments": "{{\"query\": \"a\"}}"}}"#
        )),
        [call("docs__search", r#"{"query":"a"}"#)]
    );
    assert_eq!(
        parse(&format!(
            r#"{{"{INJECTED_CALL_KEY}": "python", "arguments": {{"code": "1"}}}}"#
        )),
        [InjectedParse::Malformed {
            name: "python".to_owned(),
            arguments: r#"{"code":"1"}"#.to_owned()
        }]
    );
}

#[test]
fn a_broken_call_is_never_returned_as_text() {
    for (text, name) in [
        (
            format!(r#"{{"{INJECTED_CALL_KEY}": "rm_rf", "arguments": {{}}}}"#),
            "rm_rf",
        ),
        (
            format!(r#"{{"{INJECTED_CALL_KEY}": "docs__search", "arguments": ["a"]}}"#),
            "docs__search",
        ),
        (
            format!(r#"{{"{INJECTED_CALL_KEY}": "docs__search", "arguments": {{"query": }}}}"#),
            "docs__search",
        ),
        (
            format!(r#"{{"{INJECTED_CALL_KEY}": "docs__search", "argu"#),
            INVALID_CALL_NAME,
        ),
    ] {
        let outcome = parser().parse(&text);
        assert!(
            matches!(outcome.calls.as_slice(), [InjectedParse::Malformed { name: found, .. }] if found == name),
            "{text}: {:?}",
            outcome.calls
        );
        assert_eq!(outcome.text, "");
    }
}

#[test]
fn several_calls_in_one_reply_are_parsed_in_order() {
    let lines = parser().parse(&format!(
        "Читаю оба файла.\n{{\"{INJECTED_CALL_KEY}\": \"fs__read\", \"arguments\": {{\"path\": \"a.md\"}}}}\n{{\"{INJECTED_CALL_KEY}\": \"fs__read\", \"arguments\": {{\"path\": \"b.md\"}}}}"
    ));
    assert_eq!(lines.text, "Читаю оба файла.");
    assert_eq!(
        lines.calls,
        [
            call("fs__read", r#"{"path":"a.md"}"#),
            call("fs__read", r#"{"path":"b.md"}"#)
        ]
    );
    assert_eq!(
        parse(&format!(
            "```json\n{{\"{INJECTED_CALL_KEY}\": \"fs__read\", \"arguments\": {{\"path\": \"a.md\"}}}}\n```\n```json\n{{\"{INJECTED_CALL_KEY}\": \"docs__search\", \"arguments\": {{\"query\": \"q\"}}}}\n```"
        )),
        [
            call("fs__read", r#"{"path":"a.md"}"#),
            call("docs__search", r#"{"query":"q"}"#)
        ]
    );
    let array = parser().parse(&format!(
        "Сейчас.\n```json\n[\n  {{\"{INJECTED_CALL_KEY}\": \"fs__read\", \"arguments\": {{\"path\": \"a.md\"}}}},\n  {{\"{INJECTED_CALL_KEY}\": \"fs__read\", \"arguments\": {{\"path\": \"b.md\"}}}}\n]\n```"
    ));
    assert_eq!(array.text, "Сейчас.");
    assert_eq!(
        array.calls,
        [
            call("fs__read", r#"{"path":"a.md"}"#),
            call("fs__read", r#"{"path":"b.md"}"#)
        ]
    );
}

#[test]
fn a_malformed_second_call_keeps_the_first_and_is_reported_as_broken() {
    assert_eq!(
        parse(&format!(
            "{{\"{INJECTED_CALL_KEY}\": \"fs__read\", \"arguments\": {{\"path\": \"a.md\"}}}}\n{{\"{INJECTED_CALL_KEY}\": \"fs__read\", \"arguments\": {{\"path\": }}}}"
        )),
        [
            call("fs__read", r#"{"path":"a.md"}"#),
            InjectedParse::Malformed {
                name: "fs__read".to_owned(),
                arguments: format!(
                    "{{\"{INJECTED_CALL_KEY}\": \"fs__read\", \"arguments\": {{\"path\": }}}}"
                )
            }
        ]
    );
}

#[test]
fn only_a_tail_that_can_still_become_a_call_is_held() {
    for tail in [
        "`",
        "```",
        "```j",
        "```json\n",
        "{",
        "{ \"zvs_c",
        &format!("{{\"{INJECTED_CALL_KEY}\""),
    ] {
        assert!(could_be_marker(tail), "{tail}");
    }
    for tail in [
        "",
        "H",
        "```python",
        "{\"name\"",
        "{\"zvs_x",
        "[",
        "``x",
        "<b>",
        "<tool",
    ] {
        assert!(!could_be_marker(tail), "{tail}");
    }

    let mut plain = buffer();
    assert_eq!(plain.push("Hel"), "Hel");
    assert_eq!(plain.push("lo {"), "lo ");
    assert_eq!(plain.push("\"a\": 1}"), "{\"a\": 1}");
    assert_eq!(
        plain.finish(),
        InjectedOutcome {
            calls: Vec::new(),
            text: String::new()
        }
    );

    let mut held = parser();
    assert_eq!(held.push("Ищу. "), "Ищу. ");
    assert_eq!(
        held.push(&format!(r#"{{"{INJECTED_CALL_KEY}": "docs__se"#)),
        ""
    );
    assert_eq!(held.push(r#"arch", "arguments": {}}"#), "");
    assert_eq!(
        held.finish(),
        InjectedOutcome {
            calls: vec![call("docs__search", "{}")],
            text: String::new()
        }
    );

    let mut tail = parser();
    assert_eq!(tail.push("Ответ: ```"), "Ответ: ");
    assert_eq!(tail.push("python\nprint(1)\n```"), "```python\nprint(1)\n");
    assert_eq!(tail.finish().text, "```");
}

#[test]
fn a_capture_past_its_limit_is_released_as_text() {
    let mut buffer = buffer();
    let opening = format!("{{\"{INJECTED_CALL_KEY}\": \"");
    assert_eq!(buffer.push(&opening), "");
    let filler = "я".repeat(INJECTED_BUFFER_LIMIT);
    assert_eq!(buffer.push(&filler), format!("{opening}{filler}"));
    assert_eq!(buffer.push("tail"), "tail");
}

#[test]
fn the_dialect_request_names_the_tools_and_fences_results() {
    let encoded = dialect_request(&DefaultDialect, &tool_request());
    assert!(encoded.tools.is_empty());
    let system = encoded.system.clone().expect("a system prompt");
    assert!(system.starts_with("Be brief.\n\nYou can call external tools"));
    assert!(system.contains("Name: docs__search"));
    assert!(system.contains("Use only these tool names, spelled exactly: docs__search."));
    assert!(system.contains(r#""text": "He said \"yes\".\nNext line""#));
    assert!(system.contains(r#"Parameters (JSON Schema): {"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}"#));
    assert!(system.contains("may go in one reply, each object on its own line"));
    assert!(!system.contains("at most one tool"));
    assert_eq!(
        encoded.messages[1],
        ChatMessage::assistant(format!(
            r#"{{"{INJECTED_CALL_KEY}":"docs__search","arguments":{{"query":"rotate"}}}}"#
        ))
    );
    assert_eq!(
        encoded.messages[2],
        ChatMessage::user(format!(
            "{INJECTED_RESULT_PREFIX} docs__search\n```\nUse the CLI.\n```"
        ))
    );
    let bare = dialect_request(
        &DefaultDialect,
        &GenerateRequest {
            tools: Vec::new(),
            ..tool_request()
        },
    );
    assert_eq!(bare.system.as_deref(), Some("Be brief."));
}

#[test]
fn a_refused_or_broken_call_is_answered_with_how_to_fix_it() {
    let request = GenerateRequest {
        tools: vec![search_tool(), read_tool()],
        ..GenerateRequest::new(
            "m",
            vec![
                ChatMessage::assistant("").with_tool_calls(vec![ToolCall::new(
                    "c1",
                    "code_interpreter_v2",
                    r#"{"code":"1"}"#,
                )]),
                ChatMessage::tool("c1", "Tool call refused: not offered"),
                ChatMessage::assistant("").with_tool_calls(vec![ToolCall::new(
                    "c2",
                    "fs__read",
                    r#"{"path": "a"b"}"#,
                )]),
                ChatMessage::tool("c2", "Tool call failed: not an object"),
                ChatMessage::assistant("").with_tool_calls(vec![ToolCall::new(
                    "c3",
                    INVALID_CALL_NAME,
                    "{",
                )]),
                ChatMessage::tool("c3", "Tool call refused"),
            ],
        )
    };
    let encoded = dialect_request(&DefaultDialect, &request);
    assert!(encoded.messages[1].content.contains(
        "There is no tool named \"code_interpreter_v2\". Call one of these tools by its exact name: docs__search, fs__read."
    ));
    assert!(
        encoded.messages[3]
            .content
            .contains("escape every double quote")
    );
    assert!(
        encoded.messages[5]
            .content
            .contains("escape every double quote")
    );
}

#[test]
fn an_assistant_turn_keeps_its_text_before_the_calls() {
    let messages = injected_messages(
        &[
            ChatMessage::assistant("  Checking.  ").with_tool_calls(vec![
                ToolCall::new("c1", "docs__search", "not json"),
                ToolCall::new("c2", "fs__read", r#"{"path":"a"}"#),
            ]),
        ],
        None,
    );
    assert_eq!(
        messages[0].content,
        format!(
            "Checking.\n{{\"{INJECTED_CALL_KEY}\":\"docs__search\",\"arguments\":{{}}}}\n{{\"{INJECTED_CALL_KEY}\":\"fs__read\",\"arguments\":{{\"path\":\"a\"}}}}"
        )
    );
    assert!(messages[0].tool_calls.is_empty());
}

#[tokio::test]
async fn the_dialect_driver_passes_plain_answers_through_and_parses_whole_replies() {
    let fake = Arc::new(FakeTextDriver::with_deltas(&["The ", "answer."]));
    let request = GenerateRequest {
        tools: vec![search_tool(), read_tool()],
        ..GenerateRequest::new("m", vec![ChatMessage::user("x")])
    };
    let deltas = drain(
        with_tool_dialect(fake.clone(), Arc::new(DefaultDialect), None)
            .stream(&request, &CancellationToken::new()),
    )
    .await;
    assert_eq!(
        deltas,
        [TextDelta::text("The "), TextDelta::text("answer.")]
    );
    assert!(fake.requests()[0].tools.is_empty());

    let calling = Arc::new(FakeTextDriver::with_text(&format!(
        "Ищу. {{\"{INJECTED_CALL_KEY}\":\"docs__search\",\"arguments\":{{\"query\":\"q\"}}}}\n{{\"{INJECTED_CALL_KEY}\":\"fs__read\",\"arguments\":{{\"path\":\"a\"}}}}"
    )));
    let generated = with_tool_dialect(calling, Arc::new(DefaultDialect), Some(counted_ids()))
        .generate(&request, &CancellationToken::new())
        .await
        .expect("generates");
    assert_eq!(generated.text, "Ищу.");
    assert_eq!(generated.finish_reason, FinishReason::ToolCalls);
    assert_eq!(
        generated.tool_calls,
        [
            ToolCall::new("call_1", "docs__search", r#"{"query":"q"}"#),
            ToolCall::new("call_2", "fs__read", r#"{"path":"a"}"#)
        ]
    );
}

#[tokio::test]
async fn a_streamed_call_after_prose_is_emitted_as_a_tool_call() {
    let fake = Arc::new(FakeTextDriver::with_deltas(&[
        "Сейчас поищу.\n```json\n{\"zvs_call_tool\": \"docs",
        "__search\", \"arguments\": {\"query\": \"rotate token\"}}\n```",
    ]));
    let request = GenerateRequest {
        tools: vec![search_tool()],
        ..GenerateRequest::new("m", vec![ChatMessage::user("rotate?")])
    };
    let deltas = drain(
        with_tool_dialect(
            fake,
            Arc::new(DefaultDialect),
            Some(Arc::new(|| "call_q1".to_owned())),
        )
        .stream(&request, &CancellationToken::new()),
    )
    .await;
    assert_eq!(
        deltas,
        [
            TextDelta::text("Сейчас поищу.\n"),
            TextDelta::ToolCall(ToolCall::new(
                "call_q1",
                "docs__search",
                r#"{"query":"rotate token"}"#
            ))
        ]
    );
}

#[tokio::test]
async fn several_streamed_calls_become_tool_call_deltas_in_order_with_their_own_ids() {
    let fake = Arc::new(FakeTextDriver::with_deltas(&[
        "{\"zvs_call_tool\": \"fs__read\", \"arguments\": {\"path\": \"a.md\"}}\n{\"zvs_call",
        "_tool\": \"fs__read\", \"arguments\": {\"path\": \"b.md\"}}\n{\"zvs_call_tool\": \"fs__",
        "read\", \"arguments\": {\"path\": \"c.md\"}}",
    ]));
    let request = GenerateRequest {
        tools: vec![read_tool()],
        ..GenerateRequest::new("m", vec![ChatMessage::user("read three")])
    };
    let deltas = drain(
        with_tool_dialect(fake, Arc::new(DefaultDialect), Some(counted_ids()))
            .stream(&request, &CancellationToken::new()),
    )
    .await;
    assert_eq!(
        deltas,
        [
            TextDelta::ToolCall(ToolCall::new("call_1", "fs__read", r#"{"path":"a.md"}"#)),
            TextDelta::ToolCall(ToolCall::new("call_2", "fs__read", r#"{"path":"b.md"}"#)),
            TextDelta::ToolCall(ToolCall::new("call_3", "fs__read", r#"{"path":"c.md"}"#)),
        ]
    );
}

#[tokio::test]
async fn without_tools_the_wrapper_only_rewrites_the_history() {
    let fake = Arc::new(FakeTextDriver::with_deltas(&["{\"zvs_call_tool\": \"x\"}"]));
    let deltas = drain(
        with_tool_dialect(fake.clone(), Arc::new(DefaultDialect), None).stream(
            &GenerateRequest::new("m", vec![ChatMessage::user("x")]),
            &CancellationToken::new(),
        ),
    )
    .await;
    assert_eq!(deltas, [TextDelta::text("{\"zvs_call_tool\": \"x\"}")]);
    assert_eq!(fake.requests()[0].system, None);
    assert!(random_call_ids()().starts_with("call_"));
}
