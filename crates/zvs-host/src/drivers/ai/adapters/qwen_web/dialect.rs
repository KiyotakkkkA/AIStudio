use std::sync::LazyLock;

use crate::drivers::ai::ports::{ChatMessage, ToolSpec};
use crate::drivers::ai::tool_dialect::{
    Alias, DefaultDialect, Grammar, INJECTED_CALL_KEY, ReplyParser, ToolDialect,
};

pub const QWEN_OPEN: &str = "<tool_call>";
pub const QWEN_CLOSE: &str = "</tool_call>";
pub const QWEN_NAME_KEY: &str = "name";

const ALIASES: &[Alias] = &[Alias {
    names: &[
        "code_interpreter",
        "python",
        "python_interpreter",
        "run_python",
        "execute_python",
        "execute_code",
        "run_code",
        "code_execution",
        "code_runner",
    ],
    requires: &["language", "source"],
    renames: &[
        ("code", "source"),
        ("script", "source"),
        ("program", "source"),
    ],
    defaults: &[("language", "python")],
}];

pub static QWEN_GRAMMAR: LazyLock<Grammar> = LazyLock::new(|| {
    Grammar::new(
        Some((QWEN_OPEN, QWEN_CLOSE)),
        &[INJECTED_CALL_KEY, QWEN_NAME_KEY],
        ALIASES,
    )
});

pub struct QwenDialect;

impl ToolDialect for QwenDialect {
    fn prompt(&self, tools: &[ToolSpec]) -> String {
        DefaultDialect.prompt(tools)
    }

    fn encode_history(&self, messages: &[ChatMessage], tools: &[ToolSpec]) -> Vec<ChatMessage> {
        DefaultDialect.encode_history(messages, tools)
    }

    fn parse_stream(&self, tools: &[ToolSpec]) -> ReplyParser {
        ReplyParser::new(&QWEN_GRAMMAR, tools)
    }
}

#[cfg(test)]
mod tests;
