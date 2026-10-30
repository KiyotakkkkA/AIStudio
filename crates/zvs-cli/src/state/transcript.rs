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
    Shell(Box<ShellRun>),
    Keys(Vec<(&'static str, Vec<(String, String)>)>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellEnd {
    Exited(i32),
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellRun {
    pub command: String,
    pub output: Vec<String>,
    pub end: Option<ShellEnd>,
}

pub const SHELL_LINES_KEPT: usize = 2000;

impl ShellRun {
    pub fn new(command: &str) -> Self {
        Self {
            command: command.to_owned(),
            output: Vec::new(),
            end: None,
        }
    }

    pub fn push(&mut self, line: String) {
        self.output.push(line);
        if self.output.len() > SHELL_LINES_KEPT {
            let excess = self.output.len() - SHELL_LINES_KEPT;
            self.output.drain(..excess);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    Text { text: String, reasoning: String },
    Tool(Box<ChatToolCallDto>),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Live {
    pub segments: Vec<Segment>,
}

impl Live {
    pub fn attached(text: String, reasoning: String, tools: Vec<ChatToolCallDto>) -> Self {
        let mut segments: Vec<Segment> = tools
            .into_iter()
            .map(|call| Segment::Tool(Box::new(call)))
            .collect();
        if !text.is_empty() || !reasoning.is_empty() {
            segments.push(Segment::Text { text, reasoning });
        }
        Self { segments }
    }

    fn token(&mut self, delta: &str, reasoning: bool) {
        if !matches!(self.segments.last(), Some(Segment::Text { .. })) {
            self.segments.push(Segment::Text {
                text: String::new(),
                reasoning: String::new(),
            });
        }
        if let Some(Segment::Text {
            text,
            reasoning: thought,
        }) = self.segments.last_mut()
        {
            if reasoning {
                thought.push_str(delta);
            } else {
                text.push_str(delta);
            }
        }
    }

    fn tool(&mut self, call: ChatToolCallDto) {
        let known = self.segments.iter_mut().find_map(|segment| match segment {
            Segment::Tool(known) if known.id == call.id => Some(known),
            _ => None,
        });
        match known {
            Some(known) => **known = call,
            None => self.segments.push(Segment::Tool(Box::new(call))),
        }
    }

    pub fn tools(&self) -> impl Iterator<Item = &ChatToolCallDto> {
        self.segments.iter().filter_map(|segment| match segment {
            Segment::Tool(call) => Some(call.as_ref()),
            Segment::Text { .. } => None,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Transcript {
    pub entries: Vec<Entry>,
    pub live: Option<Live>,
}

fn tool_call(step: &serde_json::Map<String, Value>) -> Option<ChatToolCallDto> {
    if step.get("domain").and_then(Value::as_str) != Some("tool-call") {
        return None;
    }
    serde_json::from_value(step.get("call")?.clone()).ok()
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
            HostEventDraft::Token { delta, kind } if !delta.is_empty() => {
                live.token(delta, *kind == Some(DeltaKind::Reasoning));
            }
            HostEventDraft::Step { step } => {
                if let Some(call) = tool_call(step) {
                    live.tool(call);
                }
            }
            _ => {}
        }
    }

    pub fn finish(&mut self) {
        let Some(live) = self.live.take() else {
            return;
        };
        for segment in live.segments {
            match segment {
                Segment::Tool(call) => self.entries.push(Entry::Tool(call)),
                Segment::Text { text, reasoning } => {
                    if !text.is_empty() || !reasoning.is_empty() {
                        self.entries.push(Entry::Assistant { text, reasoning });
                    }
                }
            }
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
        transcript.absorb(&token("проверю ещё", Some(DeltaKind::Reasoning)));
        let live = transcript.live.clone().expect("live");
        let tools: Vec<_> = live.tools().collect();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].status, ChatToolCallStatus::Succeeded);
        assert_eq!(live.segments.len(), 3);

        transcript.finish();
        assert!(transcript.live.is_none());
        assert_eq!(transcript.entries.len(), 4);
        assert_eq!(
            transcript.entries[1],
            Entry::Assistant {
                text: String::new(),
                reasoning: "думаю".to_owned()
            }
        );
        assert!(matches!(&transcript.entries[2], Entry::Tool(call) if call.name == "read_file"));
        assert_eq!(
            transcript.entries[3],
            Entry::Assistant {
                text: "Готово.".to_owned(),
                reasoning: "проверю ещё".to_owned()
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
