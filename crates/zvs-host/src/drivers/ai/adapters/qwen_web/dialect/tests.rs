use std::sync::Arc;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::drivers::ai::mock::{FakeTextDriver, search_tool};
use crate::drivers::ai::ports::{GenerateRequest, TextDelta};
use crate::drivers::ai::tool_dialect::tests::{drain, read_tool};
use crate::drivers::ai::tool_dialect::{InjectedParse, with_tool_dialect};

const QWEN_SPEC_CALL: &str = r###"{"zvs_call_tool": "code_interpreter", "arguments": {"code": "import json\n\nspec = {\n "title": "Реестр инфраструктуры",\n "datasets": {\n "summary": {\n "path": "C:/Users/kirza/Downloads/infra_summary.csv"\n }\n },\n "sections": [{"title": "Итоги", "items": [{"type": "kpi", "hero": True}]}]\n}\n\nwith open(r"C:\Users\kirza\Downloads\infra_spec.json", "w", encoding="utf-8") as f:\n json.dump(spec, f, indent=2, ensure_ascii=False)\n\nprint("Spec written successfully")"}}"###;

fn code_tool(name: &str) -> ToolSpec {
    ToolSpec {
        name: name.to_owned(),
        description: "Run a program".to_owned(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": { "language": { "type": "string" }, "source": { "type": "string" } },
            "required": ["language", "source"],
        }),
    }
}

fn call(name: &str, arguments: &str) -> InjectedParse {
    InjectedParse::Call {
        name: name.to_owned(),
        arguments: arguments.to_owned(),
    }
}

fn parse_with(tools: &[ToolSpec], text: &str) -> Vec<InjectedParse> {
    QwenDialect.parse_stream(tools).parse(text).calls
}

fn parse(text: &str) -> Vec<InjectedParse> {
    parse_with(&[search_tool(), code_tool("code__run")], text)
}

#[test]
fn a_tool_call_tag_with_a_name_key_is_a_call() {
    assert_eq!(
        parse(r#"<tool_call>{"name": "docs__search", "arguments": {"query": "b"}}</tool_call>"#),
        [call("docs__search", r#"{"query":"b"}"#)]
    );
    assert_eq!(
        parse(
            "Читаю.\n<tool_call>\n{\"name\": \"docs__search\", \"arguments\": {\"query\": \"a\"}}\n</tool_call>\n<tool_call>\n{\"name\": \"docs__search\", \"arguments\": {\"query\": \"b\"}}\n</tool_call>"
        ),
        [
            call("docs__search", r#"{"query":"a"}"#),
            call("docs__search", r#"{"query":"b"}"#)
        ]
    );
    let bare = QwenDialect
        .parse_stream(&[search_tool()])
        .parse(r#"{"name": "docs__search", "arguments": {}}"#);
    assert!(bare.calls.is_empty());
}

#[test]
fn a_code_interpreter_habit_resolves_to_the_offered_tool_that_takes_a_program() {
    assert_eq!(
        parse(&format!(
            r#"{{"{INJECTED_CALL_KEY}": "python", "arguments": {{"code": "print(1)"}}}}"#
        )),
        [call(
            "code__run",
            r#"{"source":"print(1)","language":"python"}"#
        )]
    );
    assert_eq!(
        parse_with(
            &[search_tool(), code_tool("code__run_2")],
            &format!(r#"{{"{INJECTED_CALL_KEY}": "run_code", "arguments": {{"script": "1"}}}}"#),
        ),
        [call("code__run_2", r#"{"source":"1","language":"python"}"#)]
    );
    for tools in [
        vec![search_tool()],
        vec![code_tool("code__run"), code_tool("sandbox__run")],
    ] {
        assert_eq!(
            parse_with(
                &tools,
                &format!(r#"{{"{INJECTED_CALL_KEY}": "python", "arguments": {{"code": "1"}}}}"#),
            ),
            [InjectedParse::Malformed {
                name: "python".to_owned(),
                arguments: r#"{"code":"1"}"#.to_owned()
            }]
        );
    }
}

#[test]
fn qwen_code_with_unescaped_quotes_is_recovered_as_a_code_run_call() {
    let calls = parse(QWEN_SPEC_CALL);
    let [InjectedParse::Call { name, arguments }] = calls.as_slice() else {
        panic!("the call is recovered");
    };
    assert_eq!(name, "code__run");
    let arguments: Value = serde_json::from_str(arguments).expect("json");
    assert_eq!(arguments["language"], "python");
    let source = arguments["source"].as_str().expect("the source");
    assert!(source.starts_with("import json\n\nspec = {\n \"title\": \"Реестр инфраструктуры\","));
    assert!(source.contains(r#"with open(r"C:\Users\kirza\Downloads\infra_spec.json", "w""#));
    assert!(source.ends_with("print(\"Spec written successfully\")"));
}

#[test]
fn a_recovered_call_followed_by_a_read_keeps_both() {
    let text = format!(
        "{QWEN_SPEC_CALL}\n{{\"{INJECTED_CALL_KEY}\": \"docs__search\", \"arguments\": {{\"query\": \"q\"}}}}"
    );
    let calls = parse(&text);
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(matches!(&calls[0], InjectedParse::Call { name, .. } if name == "code__run"));
    assert_eq!(calls[1], call("docs__search", r#"{"query":"q"}"#));
}

#[test]
fn only_the_qwen_tag_is_held_beyond_the_default_markers() {
    for tail in ["<", "<tool", "<tool_call", "```json\n", "{ \"zvs_c"] {
        assert!(QWEN_GRAMMAR.could_be_marker(tail), "{tail}");
    }
    for tail in ["<b>", "<tool_x", "{\"name\"", "["] {
        assert!(!QWEN_GRAMMAR.could_be_marker(tail), "{tail}");
    }
}

#[tokio::test]
async fn the_qwen_reply_from_the_field_runs_code_instead_of_showing_json() {
    let half = QWEN_SPEC_CALL.len() / 2;
    let cut = (0..=half)
        .rev()
        .find(|index| QWEN_SPEC_CALL.is_char_boundary(*index))
        .expect("a boundary");
    let fake = Arc::new(FakeTextDriver::with_deltas(&[
        &QWEN_SPEC_CALL[..cut],
        &QWEN_SPEC_CALL[cut..],
    ]));
    let request = GenerateRequest {
        tools: vec![search_tool(), read_tool(), code_tool("code__run")],
        ..GenerateRequest::new(
            "m",
            vec![crate::drivers::ai::ports::ChatMessage::user("dashboard")],
        )
    };
    let deltas = drain(
        with_tool_dialect(
            fake,
            Arc::new(QwenDialect),
            Some(Arc::new(|| "call_c".to_owned())),
        )
        .stream(&request, &CancellationToken::new()),
    )
    .await;
    assert_eq!(deltas.len(), 1);
    let TextDelta::ToolCall(call) = &deltas[0] else {
        panic!("a tool call, not text: {deltas:?}");
    };
    assert_eq!(call.name, "code__run");
    assert!(call.arguments.contains("\"language\":\"python\""));
}
