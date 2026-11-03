use super::attachments::{Attachment, AttachmentLabel};
use std::collections::BTreeMap;

use serde_json::Value;
use zvs_contract::{
    ChatToolCallDto, DeltaKind, HostEventDraft, McpServerDetailDto, MessageDto, MessageRole, RunId,
    SUBAGENT_DOMAIN,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    User(String),
    Attachments(Vec<AttachmentLabel>),
    Assistant { text: String, reasoning: String },
    Tool(Box<ChatToolCallDto>),
    Notice { tone: Tone, text: String },
    Shell(Box<ShellRun>),
    Server(Box<McpServerDetailDto>),
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

pub const AGENT_DELEGATE: &str = "agent.delegate";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Live {
    pub segments: Vec<Segment>,
    pub children: BTreeMap<String, Vec<ChatToolCallDto>>,
}

pub fn subagent_run(call: &ChatToolCallDto) -> Option<&str> {
    if call.key != AGENT_DELEGATE {
        return None;
    }
    call.meta.as_ref()?.get("runId")?.as_str()
}

pub fn subagent_running(call: &ChatToolCallDto) -> bool {
    call.key == AGENT_DELEGATE
        && call
            .meta
            .as_ref()
            .and_then(|meta| meta.get("status"))
            .and_then(Value::as_str)
            .is_some_and(|status| status == "running" || status == "queued")
}

pub fn subagent_status(call: &ChatToolCallDto) -> &'static str {
    use crate::copy;
    match call
        .meta
        .as_ref()
        .and_then(|meta| meta.get("status"))
        .and_then(Value::as_str)
    {
        Some("running") => copy::SUBAGENT_RUNNING,
        Some("queued") => copy::SUBAGENT_QUEUED,
        Some("partial") => copy::SUBAGENT_PARTIAL,
        Some("cancelled") => copy::SUBAGENT_STOPPED,
        Some("failed") => copy::SUBAGENT_FAILED,
        _ => copy::SUBAGENT_DONE,
    }
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
        Self {
            segments,
            children: BTreeMap::new(),
        }
    }

    fn child(&mut self, step: &serde_json::Map<String, Value>) {
        let (Some(run), Some(index), Some(Ok(call))) = (
            step.get("runId").and_then(Value::as_str),
            step.get("index")
                .and_then(Value::as_u64)
                .and_then(|index| usize::try_from(index).ok()),
            step.get("call")
                .map(|call| serde_json::from_value::<ChatToolCallDto>(call.clone())),
        ) else {
            return;
        };
        let calls = self.children.entry(run.to_owned()).or_default();
        let index = index.min(calls.len());
        if index == calls.len() {
            calls.push(call);
        } else {
            calls[index] = call;
        }
    }

    pub fn children_of(&self, call: &ChatToolCallDto) -> &[ChatToolCallDto] {
        subagent_run(call)
            .and_then(|run| self.children.get(run))
            .map_or(&[], Vec::as_slice)
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
                MessageRole::User => {
                    transcript.push_user(&message.content);
                    if !message.images.is_empty() {
                        transcript.entries.push(Entry::Attachments(
                            message
                                .images
                                .iter()
                                .map(|image| AttachmentLabel {
                                    name: image.name.clone(),
                                    image: true,
                                })
                                .collect(),
                        ));
                    }
                }
                MessageRole::Event => {
                    if let Some(event) = &message.event {
                        transcript.notice(Tone::Info, crate::copy::process_event(event));
                    }
                }
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

    pub fn push_attachments(&mut self, attachments: &[Attachment]) {
        if !attachments.is_empty() {
            self.entries.push(Entry::Attachments(
                attachments.iter().map(Attachment::label).collect(),
            ));
        }
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
                } else if step.get("domain").and_then(Value::as_str) == Some(SUBAGENT_DOMAIN) {
                    live.child(step);
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

    pub fn subagents(&self) -> Vec<ChatToolCallDto> {
        let mut calls: Vec<ChatToolCallDto> = self
            .entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Tool(call) if subagent_run(call).is_some() => Some(call.as_ref().clone()),
                _ => None,
            })
            .collect();
        if let Some(live) = &self.live {
            calls.extend(
                live.tools()
                    .filter(|call| subagent_run(call).is_some())
                    .cloned(),
            );
        }
        calls
    }

    pub fn newest_subagent(&self) -> Option<RunId> {
        self.live
            .as_ref()?
            .tools()
            .filter(|call| subagent_running(call))
            .filter_map(subagent_run)
            .filter_map(RunId::parse)
            .last()
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
