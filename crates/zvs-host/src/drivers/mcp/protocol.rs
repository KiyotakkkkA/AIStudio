use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use serde_json::{Map, Value};
use zvs_contract::{McpCallResultDto, McpContentDto};

pub const CLIENT_PROTOCOL_VERSION: &str = "2025-06-18";

pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

pub const CLIENT_NAME: &str = "zvs-ai-studio";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    #[serde(default)]
    pub read_only_hint: Option<bool>,
    #[serde(default)]
    pub destructive_hint: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDescriptor {
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub input_schema: Option<Value>,
    #[serde(default)]
    pub annotations: Option<ToolAnnotations>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResourceDescriptor {
    pub uri: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPromptDescriptor {
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub arguments: Option<Vec<Value>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct McpDiscovery {
    pub tools: Vec<McpToolDescriptor>,
    pub resources: Vec<McpResourceDescriptor>,
    pub prompts: Vec<McpPromptDescriptor>,
}

static DESTRUCTIVE_VERBS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(^|[._\-/])(delete|destroy|drop|remove|erase|purge|truncate|wipe|rm|unlink|revoke|uninstall|reset|prune)",
    )
    .expect("a valid pattern")
});

pub fn infer_destructive(tool: &McpToolDescriptor) -> bool {
    let annotations = tool.annotations.clone().unwrap_or_default();
    if let Some(hint) = annotations.destructive_hint {
        return hint;
    }
    if annotations.read_only_hint == Some(true) {
        return false;
    }
    DESTRUCTIVE_VERBS.is_match(&tool.name)
}

pub fn infer_read_only(tool: &McpToolDescriptor) -> bool {
    tool.annotations
        .as_ref()
        .and_then(|annotations| annotations.read_only_hint)
        .unwrap_or(false)
}

pub fn call_result(result: &Value) -> McpCallResultDto {
    let content: Vec<McpContentDto> = result
        .get("content")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .map(|entry| {
                    let mut rest: Map<String, Value> =
                        entry.as_object().cloned().unwrap_or_default();
                    let content_type = rest
                        .remove("type")
                        .and_then(|value| value.as_str().map(str::to_owned))
                        .unwrap_or_else(|| "text".to_owned());
                    let text = rest
                        .remove("text")
                        .and_then(|value| value.as_str().map(str::to_owned));
                    McpContentDto {
                        content_type,
                        text,
                        data: if rest.is_empty() {
                            Value::Null
                        } else {
                            Value::Object(rest)
                        },
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let text = content
        .iter()
        .filter_map(|entry| entry.text.as_deref())
        .collect::<Vec<_>>()
        .join("\n");
    McpCallResultDto {
        content,
        text,
        structured: result
            .get("structuredContent")
            .cloned()
            .unwrap_or(Value::Null),
        is_error: result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn tool(name: &str, annotations: Value) -> McpToolDescriptor {
        serde_json::from_value(json!({ "name": name, "annotations": annotations }))
            .expect("a descriptor")
    }

    #[test]
    fn a_servers_own_hints_win_and_a_name_that_removes_something_is_destructive() {
        assert!(infer_destructive(&tool("deleteCollection", json!({}))));
        assert!(infer_destructive(&tool("files.rm", json!({}))));
        assert!(infer_destructive(&tool("PURGE_cache", json!({}))));
        assert!(!infer_destructive(&tool("search", json!({}))));
        assert!(!infer_destructive(&tool(
            "deleteCollection",
            json!({ "readOnlyHint": true })
        )));
        assert!(!infer_destructive(&tool(
            "delete",
            json!({ "destructiveHint": false })
        )));
        assert!(infer_destructive(&tool(
            "search",
            json!({ "destructiveHint": true })
        )));
        assert!(infer_read_only(&tool(
            "search",
            json!({ "readOnlyHint": true })
        )));
        assert!(!infer_read_only(&tool("search", json!({}))));
    }

    #[test]
    fn a_call_result_keeps_text_joins_it_and_moves_the_rest_into_data() {
        let result = call_result(&json!({
            "content": [
                { "type": "text", "text": "one" },
                { "type": "image", "data": "aGk=", "mimeType": "image/png" },
                { "type": "text", "text": "two" },
            ],
            "structuredContent": { "n": 1 },
        }));
        assert_eq!(result.text, "one\ntwo");
        assert_eq!(result.content[0].data, Value::Null);
        assert_eq!(result.content[1].text, None);
        assert_eq!(
            result.content[1].data,
            json!({ "data": "aGk=", "mimeType": "image/png" })
        );
        assert_eq!(result.structured, json!({ "n": 1 }));
        assert!(!result.is_error);
        assert!(call_result(&json!({ "isError": true })).is_error);
    }
}
