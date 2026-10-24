use serde::Deserialize;
use serde_json::{Value, json};

use crate::protocol::MESSAGE_VERSION;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChat<'a> {
    pub model: &'a str,
    pub prompt: &'a str,
    pub message_id: &'a str,
    pub thinking: bool,
    pub now_ms: i64,
}

impl NewChat<'_> {
    pub fn body(&self) -> Value {
        json!({
            "chat": {
                "id": "",
                "title": "New Chat",
                "models": [self.model],
                "params": {},
                "history": {
                    "messages": {
                        self.message_id: {
                            "id": self.message_id,
                            "parentId": null,
                            "childrenIds": [],
                            "role": "user",
                            "content": self.prompt,
                            "timestamp": self.now_ms.div_euclid(1000),
                            "models": [self.model],
                        },
                    },
                    "currentId": self.message_id,
                },
                "tags": [],
                "flags": [],
                "features": [],
                "mcp_servers": [],
                "enable_thinking": self.thinking,
                "auto_web_search": false,
                "message_version": MESSAGE_VERSION,
                "extra": {},
                "timestamp": self.now_ms,
            },
        })
    }
}

#[derive(Deserialize)]
struct ChatCreated {
    id: Option<String>,
}

pub fn parse_chat_created(payload: &Value) -> Option<String> {
    let created = ChatCreated::deserialize(payload).ok()?;
    let id = created.id?;
    let id = id.trim();
    (!id.is_empty()).then(|| id.to_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnMessage<'a> {
    pub role: &'a str,
    pub content: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionTurn<'a> {
    pub chat_id: &'a str,
    pub model: &'a str,
    pub messages: &'a [TurnMessage<'a>],
    pub thinking: bool,
    pub reasoning_effort: Option<&'a str>,
    pub message_id: &'a str,
    pub parent_id: Option<&'a str>,
    pub response_id: &'a str,
}

impl CompletionTurn<'_> {
    pub fn signature_prompt(&self) -> &str {
        self.messages
            .iter()
            .rev()
            .find(|message| message.role == "user")
            .map_or("", |message| message.content.trim())
    }

    pub fn body(&self) -> Value {
        let mut features = json!({
            "image_generation": false,
            "web_search": false,
            "auto_web_search": false,
            "preview_mode": false,
            "flags": [],
            "vlm_tools_enable": false,
            "vlm_web_search_enable": false,
            "vlm_website_mode": false,
            "enable_thinking": self.thinking,
        });
        if let Some(effort) = self.reasoning_effort.filter(|_| self.thinking) {
            features["reasoning_effort"] = json!(effort);
        }
        let messages: Vec<Value> = self
            .messages
            .iter()
            .map(|message| json!({ "role": message.role, "content": message.content }))
            .collect();
        json!({
            "stream": true,
            "model": self.model,
            "messages": messages,
            "signature_prompt": self.signature_prompt(),
            "params": {},
            "extra": {},
            "features": features,
            "variables": {},
            "chat_id": self.chat_id,
            "id": self.response_id,
            "current_user_message_id": self.message_id,
            "current_user_message_parent_id": self.parent_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HISTORY: [TurnMessage<'static>; 4] = [
        TurnMessage {
            role: "system",
            content: "Отвечай кратко",
        },
        TurnMessage {
            role: "user",
            content: "Привет",
        },
        TurnMessage {
            role: "assistant",
            content: "Здравствуйте",
        },
        TurnMessage {
            role: "user",
            content: " Как дела? ",
        },
    ];

    fn turn() -> CompletionTurn<'static> {
        CompletionTurn {
            chat_id: "chat",
            model: "glm-5.2",
            messages: &HISTORY,
            thinking: true,
            reasoning_effort: Some("high"),
            message_id: "user-message",
            parent_id: Some("previous-answer"),
            response_id: "answer",
        }
    }

    #[test]
    fn a_turn_carries_the_dialog_and_signs_the_last_user_message() {
        let body = turn().body();
        assert_eq!(body["messages"].as_array().map(Vec::len), Some(4));
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][3]["content"], " Как дела? ");
        assert_eq!(body["signature_prompt"], "Как дела?");
        assert_eq!(body["current_user_message_id"], "user-message");
        assert_eq!(body["current_user_message_parent_id"], "previous-answer");
        assert_eq!(body["id"], "answer");
        assert_eq!(body["features"]["reasoning_effort"], "high");
        assert_eq!(body["features"]["flags"], json!([]));
        assert!(body.get("captcha_verify_param").is_none());
    }

    #[test]
    fn a_fast_first_turn_drops_the_effort_and_has_no_parent() {
        let body = CompletionTurn {
            thinking: false,
            parent_id: None,
            messages: &HISTORY[1..2],
            ..turn()
        }
        .body();
        assert_eq!(body["features"]["enable_thinking"], false);
        assert!(body["features"].get("reasoning_effort").is_none());
        assert_eq!(body["current_user_message_parent_id"], Value::Null);
        assert_eq!(body["signature_prompt"], "Привет");
    }

    #[test]
    fn a_new_chat_holds_the_first_message_and_its_id_is_read_back() {
        let body = NewChat {
            model: "glm-5.2",
            prompt: "Привет",
            message_id: "m",
            thinking: true,
            now_ms: 1_792_867_619_851,
        }
        .body();
        assert_eq!(body["chat"]["history"]["currentId"], "m");
        assert_eq!(
            body["chat"]["history"]["messages"]["m"]["timestamp"],
            1_792_867_619
        );
        assert_eq!(body["chat"]["timestamp"], 1_792_867_619_851_i64);
        assert_eq!(
            parse_chat_created(&json!({ "id": " a ", "chat": {} })).as_deref(),
            Some("a")
        );
        for refused in [json!({ "id": "  " }), json!({ "id": 7 }), json!({})] {
            assert_eq!(parse_chat_created(&refused), None, "{refused}");
        }
    }
}
