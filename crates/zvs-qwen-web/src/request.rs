use serde::Deserialize;
use serde_json::{Value, json};

use crate::protocol::{API_VERSION, CHAT_MODE, CHAT_TYPE};

/// The body of `POST /api/v2/chats/new`; `now_ms` is a Unix time in milliseconds.
pub fn new_chat_body(model: &str, now_ms: i64) -> Value {
    json!({
        "chatId": "",
        "chat_mode": CHAT_MODE,
        "chat_type": CHAT_TYPE,
        "models": [model],
        "project_id": "",
        "timestamp": now_ms,
    })
}

#[derive(Deserialize)]
struct ChatCreatedData {
    id: Option<String>,
}

#[derive(Deserialize)]
struct ChatCreated {
    success: Option<bool>,
    id: Option<String>,
    data: Option<ChatCreatedData>,
}

/// The id of the chat `chats/new` created, or `None` when the answer is not a created chat.
pub fn parse_chat_created(payload: &Value) -> Option<String> {
    let created = ChatCreated::deserialize(payload).ok()?;
    if created.success == Some(false) {
        return None;
    }
    let id = created.data.and_then(|data| data.id).or(created.id)?;
    let id = id.trim();
    (!id.is_empty()).then(|| id.to_owned())
}

/// One user turn in a freshly created vendor chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionTurn<'a> {
    pub chat_id: &'a str,
    pub model: &'a str,
    /// The whole conversation flattened into one user message.
    pub prompt: &'a str,
    pub thinking: bool,
    /// The client-side id of the message.
    pub fid: &'a str,
    pub now_ms: i64,
}

impl CompletionTurn<'_> {
    /// The body of `POST /api/v2/chat/completions?chat_id=…`, streamed in the `phase` schema.
    pub fn body(&self) -> Value {
        let timestamp = self.now_ms.div_euclid(1000);
        json!({
            "chatId": self.chat_id,
            "chat_id": self.chat_id,
            "chat_mode": CHAT_MODE,
            "chat_type": CHAT_TYPE,
            "incremental_output": true,
            "messages": [{
                "id": null,
                "fid": self.fid,
                "parentId": null,
                "parent_id": null,
                "childrenIds": [],
                "role": "user",
                "content": self.prompt,
                "user_action": "chat",
                "files": [],
                "timestamp": timestamp,
                "model": "",
                "models": [self.model],
                "chat_type": CHAT_TYPE,
                "sub_chat_type": CHAT_TYPE,
                "feature_config": {
                    "thinking_enabled": self.thinking,
                    "thinking_format": "summary",
                    "thinking_mode": if self.thinking { "Thinking" } else { "Fast" },
                    "auto_thinking": false,
                    "auto_search": false,
                    "output_schema": "phase",
                    "research_mode": "normal",
                },
                "extra": { "meta": { "subChatType": CHAT_TYPE } },
            }],
            "model": self.model,
            "parentId": null,
            "parent_id": null,
            "stream": true,
            "timestamp": timestamp,
            "version": API_VERSION,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_selects_thinking_or_fast_and_uses_seconds() {
        let turn = CompletionTurn {
            chat_id: "chat",
            model: "qwen3.7-plus",
            prompt: "Привет",
            thinking: true,
            fid: "fid",
            now_ms: 1_788_994_021_999,
        };
        let body = turn.body();
        assert_eq!(body["chat_id"], "chat");
        assert_eq!(body["timestamp"], 1_788_994_021);
        assert_eq!(body["version"], API_VERSION);
        let message = &body["messages"][0];
        assert_eq!(message["content"], "Привет");
        assert_eq!(message["feature_config"]["thinking_mode"], "Thinking");
        let fast = CompletionTurn {
            thinking: false,
            ..turn
        }
        .body();
        assert_eq!(
            fast["messages"][0]["feature_config"]["thinking_mode"],
            "Fast"
        );
        assert_eq!(
            fast["messages"][0]["feature_config"]["thinking_enabled"],
            false
        );
    }

    #[test]
    fn a_created_chat_yields_its_trimmed_id() {
        let new = new_chat_body("m", 5);
        assert_eq!(new["chat_mode"], CHAT_MODE);
        assert_eq!(new["models"], json!(["m"]));
        assert_eq!(
            parse_chat_created(&json!({ "success": true, "data": { "id": " a " } })).as_deref(),
            Some("a")
        );
        assert_eq!(
            parse_chat_created(&json!({ "id": "b" })).as_deref(),
            Some("b")
        );
        for refused in [
            json!({ "success": false, "data": { "id": "a" } }),
            json!({ "data": { "id": "  " } }),
            json!({ "data": { "id": 7 } }),
            json!({}),
        ] {
            assert_eq!(parse_chat_created(&refused), None, "{refused}");
        }
    }
}
