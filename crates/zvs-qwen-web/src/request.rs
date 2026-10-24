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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingMode {
    Auto,
    Thinking,
    Fast,
}

impl ThinkingMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Thinking => "Thinking",
            Self::Fast => "Fast",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionTurn<'a> {
    pub chat_id: &'a str,
    pub model: &'a str,
    pub prompt: &'a str,
    pub thinking: ThinkingMode,
    pub parent_id: Option<&'a str>,
    pub fid: &'a str,
    pub answer_fid: &'a str,
    pub now_ms: i64,
}

impl CompletionTurn<'_> {
    pub fn body(&self) -> Value {
        let timestamp = self.now_ms.div_euclid(1000);
        let parent = self.parent_id.map_or(Value::Null, |id| json!(id));
        json!({
            "chatId": self.chat_id,
            "chat_id": self.chat_id,
            "chat_mode": CHAT_MODE,
            "chat_type": CHAT_TYPE,
            "incremental_output": true,
            "messages": [{
                "id": null,
                "fid": self.fid,
                "parentId": parent,
                "parent_id": parent,
                "childrenIds": [self.answer_fid],
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
                    "thinking_enabled": self.thinking != ThinkingMode::Fast,
                    "thinking_format": "summary",
                    "thinking_mode": self.thinking.as_str(),
                    "auto_thinking": self.thinking == ThinkingMode::Auto,
                    "auto_search": false,
                    "output_schema": "phase",
                    "research_mode": "normal",
                },
                "extra": { "meta": { "subChatType": CHAT_TYPE } },
            }],
            "model": self.model,
            "parentId": self.parent_id.unwrap_or_default(),
            "parent_id": parent,
            "stream": true,
            "timestamp": timestamp,
            "version": API_VERSION,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(thinking: ThinkingMode, parent_id: Option<&'static str>) -> CompletionTurn<'static> {
        CompletionTurn {
            chat_id: "chat",
            model: "qwen3.7-plus",
            prompt: "Привет",
            thinking,
            parent_id,
            fid: "fid",
            answer_fid: "answer",
            now_ms: 1_788_994_021_999,
        }
    }

    #[test]
    fn a_first_turn_has_no_parent_and_names_its_answer_in_advance() {
        let body = turn(ThinkingMode::Auto, None).body();
        assert_eq!(body["chat_id"], "chat");
        assert_eq!(body["timestamp"], 1_788_994_021);
        assert_eq!(body["version"], API_VERSION);
        assert_eq!(body["parentId"], "");
        assert_eq!(body["parent_id"], Value::Null);
        let message = &body["messages"][0];
        assert_eq!(message["content"], "Привет");
        assert_eq!(message["parent_id"], Value::Null);
        assert_eq!(message["childrenIds"], json!(["answer"]));
    }

    #[test]
    fn a_follow_up_turn_hangs_off_the_previous_answer() {
        let body = turn(ThinkingMode::Auto, Some("response")).body();
        assert_eq!(body["parentId"], "response");
        assert_eq!(body["parent_id"], "response");
        assert_eq!(body["messages"][0]["parentId"], "response");
        assert_eq!(body["messages"][0]["parent_id"], "response");
    }

    #[test]
    fn each_thinking_mode_sets_the_flags_the_client_sends() {
        for (mode, enabled, auto) in [
            (ThinkingMode::Auto, true, true),
            (ThinkingMode::Thinking, true, false),
            (ThinkingMode::Fast, false, false),
        ] {
            let config = &turn(mode, None).body()["messages"][0]["feature_config"];
            assert_eq!(config["thinking_mode"], mode.as_str());
            assert_eq!(config["thinking_enabled"], enabled);
            assert_eq!(config["auto_thinking"], auto);
            assert_eq!(config["auto_search"], false);
        }
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
