use serde_json::Value;
use zvs_contract::{ChatToolCallDto, DeltaKind, HostEventDraft, MessageDto, MessageRole};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    User(String),
    Assistant { text: String, reasoning: String },
    Tool(Box<ChatToolCallDto>),
    Notice { tone: Tone, text: String },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Live {
    pub text: String,
    pub reasoning: String,
    pub tools: Vec<ChatToolCallDto>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Transcript {
    pub entries: Vec<Entry>,
    pub live: Option<Live>,
}

fn tool_call(step: &serde_json::Map<String, Value>) -> Option<(usize, ChatToolCallDto)> {
    if step.get("domain").and_then(Value::as_str) != Some("tool-call") {
        return None;
    }
    let index = usize::try_from(step.get("index")?.as_u64()?).ok()?;
    let call = serde_json::from_value(step.get("call")?.clone()).ok()?;
    Some((index, call))
}

impl Transcript {
    pub fn from_messages(messages: &[MessageDto]) -> Self {
        let mut transcript = Self::default();
        for message in messages {
            match message.role {
                MessageRole::User => transcript
                    .entries
                    .push(Entry::User(message.content.clone())),
                MessageRole::Assistant => {
                    transcript.entries.extend(
                        message
                            .tool_calls
                            .iter()
                            .cloned()
                            .map(|call| Entry::Tool(Box::new(call))),
                    );
                    if !message.content.is_empty() || !message.reasoning.is_empty() {
                        transcript.entries.push(Entry::Assistant {
                            text: message.content.clone(),
                            reasoning: message.reasoning.clone(),
                        });
                    }
                }
            }
        }
        transcript
    }

    pub fn push_user(&mut self, text: &str) {
        self.entries.push(Entry::User(text.to_owned()));
    }

    pub fn notice(&mut self, tone: Tone, text: impl Into<String>) {
        self.entries.push(Entry::Notice {
            tone,
            text: text.into(),
        });
    }

    pub fn start(&mut self) {
        self.live = Some(Live::default());
    }

    pub fn absorb(&mut self, body: &HostEventDraft) {
        let Some(live) = self.live.as_mut() else {
            return;
        };
        match body {
            HostEventDraft::Token { delta, kind } => {
                if *kind == Some(DeltaKind::Reasoning) {
                    live.reasoning.push_str(delta);
                } else {
                    live.text.push_str(delta);
                }
            }
            HostEventDraft::Step { step } => {
                if let Some((index, call)) = tool_call(step) {
                    if index < live.tools.len() {
                        live.tools[index] = call;
                    } else {
                        live.tools.push(call);
                    }
                }
            }
            _ => {}
        }
    }

    pub fn finish(&mut self) {
        let Some(live) = self.live.take() else {
            return;
        };
        self.entries.extend(
            live.tools
                .into_iter()
                .map(|call| Entry::Tool(Box::new(call))),
        );
        if !live.text.is_empty() || !live.reasoning.is_empty() {
            self.entries.push(Entry::Assistant {
                text: live.text,
                reasoning: live.reasoning,
            });
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zvs_contract::ChatToolCallStatus;

    use super::*;

    fn call(status: &str) -> Value {
        json!({
            "id": "c1",
            "key": "files.read",
            "name": "read_file",
            "round": 0,
            "arguments": { "path": "a.txt" },
            "status": status,
        })
    }

    fn step(index: u64, status: &str) -> HostEventDraft {
        let Value::Object(step) =
            json!({ "domain": "tool-call", "index": index, "call": call(status) })
        else {
            unreachable!()
        };
        HostEventDraft::Step { step }
    }

    fn token(delta: &str, kind: Option<DeltaKind>) -> HostEventDraft {
        HostEventDraft::Token {
            delta: delta.to_owned(),
            kind,
        }
    }

    #[test]
    fn a_turn_streams_text_reasoning_and_updates_tool_calls_in_place() {
        let mut transcript = Transcript::default();
        transcript.push_user("прочитай файл");
        transcript.start();
        transcript.absorb(&token("думаю", Some(DeltaKind::Reasoning)));
        transcript.absorb(&step(0, "running"));
        transcript.absorb(&step(0, "succeeded"));
        transcript.absorb(&token("Готово", None));
        transcript.absorb(&token(".", Some(DeltaKind::Text)));
        let live = transcript.live.clone().expect("live");
        assert_eq!(live.tools.len(), 1);
        assert_eq!(live.tools[0].status, ChatToolCallStatus::Succeeded);

        transcript.finish();
        assert!(transcript.live.is_none());
        assert_eq!(transcript.entries.len(), 3);
        assert!(matches!(&transcript.entries[1], Entry::Tool(call) if call.name == "read_file"));
        assert_eq!(
            transcript.entries[2],
            Entry::Assistant {
                text: "Готово.".to_owned(),
                reasoning: "думаю".to_owned()
            }
        );
    }

    #[test]
    fn events_without_a_live_turn_and_foreign_steps_are_ignored() {
        let mut transcript = Transcript::default();
        transcript.absorb(&token("lost", None));
        assert!(transcript.entries.is_empty());
        transcript.start();
        let Value::Object(other) = json!({ "domain": "retrieval", "index": 0 }) else {
            unreachable!()
        };
        transcript.absorb(&HostEventDraft::Step { step: other });
        transcript.finish();
        assert!(transcript.entries.is_empty());
    }
}
