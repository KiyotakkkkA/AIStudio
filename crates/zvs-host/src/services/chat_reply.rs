use std::collections::HashMap;

use zvs_contract::{ChatRoundDto, ChatToolCallDto, DeltaKind, Timestamp};

use super::chat_history::tokens;

pub const REPLY_FLUSH_MS: Timestamp = 250;
pub const TOOL_CALL_DOMAIN: &str = "tool-call";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplyChunk {
    pub content: String,
    pub reasoning: String,
    pub tokens: i64,
}

#[derive(Debug, Default)]
pub struct ReplyBuffers {
    pending: HashMap<String, (Timestamp, ReplyChunk)>,
}

impl ReplyBuffers {
    pub fn push(
        &mut self,
        id: &str,
        now: Timestamp,
        delta: &str,
        kind: Option<DeltaKind>,
    ) -> Option<ReplyChunk> {
        let (since, chunk) = self
            .pending
            .entry(id.to_owned())
            .or_insert_with(|| (now, ReplyChunk::default()));
        if kind == Some(DeltaKind::Reasoning) {
            chunk.reasoning.push_str(delta);
        } else {
            chunk.content.push_str(delta);
        }
        chunk.tokens += tokens(delta);
        if now - *since >= REPLY_FLUSH_MS {
            self.take(id)
        } else {
            None
        }
    }

    pub fn take(&mut self, id: &str) -> Option<ReplyChunk> {
        self.pending.remove(id).map(|(_, chunk)| chunk)
    }
}

pub fn apply_tool_call(calls: &mut Vec<ChatToolCallDto>, index: usize, call: ChatToolCallDto) {
    match calls.get_mut(index) {
        Some(slot) => *slot = call,
        None => calls.push(call),
    }
}

pub fn append_round(
    rounds: &mut Vec<ChatRoundDto>,
    calls: &[ChatToolCallDto],
    kind: Option<DeltaKind>,
    delta: &str,
) {
    let index = calls
        .iter()
        .map(|call| usize::try_from(call.round).map_or(0, |round| round + 1))
        .max()
        .unwrap_or(0);
    if rounds.len() <= index {
        rounds.resize_with(index + 1, ChatRoundDto::default);
    }
    let round = &mut rounds[index];
    if kind == Some(DeltaKind::Reasoning) {
        round.reasoning.push_str(delta);
    } else {
        round.text.push_str(delta);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_reply_is_written_a_bounded_number_of_times_and_loses_nothing() {
        let mut buffers = ReplyBuffers::default();
        let mut writes = Vec::new();
        for index in 0..400 {
            let kind = (index < 40).then_some(DeltaKind::Reasoning);
            if let Some(chunk) = buffers.push("a", index * 10, "tok ", kind) {
                writes.push(chunk);
            }
        }
        writes.extend(buffers.take("a"));

        assert!(writes.len() <= 400 * 10 / 250 + 1, "{}", writes.len());
        let content: String = writes.iter().map(|chunk| chunk.content.as_str()).collect();
        let reasoning: String = writes
            .iter()
            .map(|chunk| chunk.reasoning.as_str())
            .collect();
        assert_eq!(content, "tok ".repeat(360));
        assert_eq!(reasoning, "tok ".repeat(40));
        assert_eq!(
            writes.iter().map(|chunk| chunk.tokens).sum::<i64>(),
            400 * tokens("tok ")
        );
        assert_eq!(buffers.take("a"), None);
    }

    #[test]
    fn replies_buffer_apart_and_a_burst_waits_for_the_window() {
        let mut buffers = ReplyBuffers::default();
        assert_eq!(buffers.push("a", 0, "x", None), None);
        assert_eq!(buffers.push("b", 100, "y", None), None);
        assert_eq!(buffers.push("a", 249, "x", None), None);
        let flushed = buffers.push("a", 250, "x", None).expect("a flush");
        assert_eq!(flushed.content, "xxx");
        assert_eq!(
            buffers.take("b").map(|chunk| chunk.content).as_deref(),
            Some("y")
        );
    }
}
