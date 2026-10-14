use std::collections::VecDeque;

use futures::StreamExt;
use futures::stream::BoxStream;
use zvs_contract::AppError;

use super::transport::ByteStream;

pub const SSE_DONE: &str = "[DONE]";

pub type FrameStream = BoxStream<'static, Result<String, AppError>>;

#[derive(Debug, Default)]
pub struct Utf8Decoder {
    pending: Vec<u8>,
}

impl Utf8Decoder {
    pub fn decode(&mut self, chunk: &[u8]) -> String {
        self.pending.extend_from_slice(chunk);
        let mut text = String::new();
        let mut start = 0;
        while start < self.pending.len() {
            match std::str::from_utf8(&self.pending[start..]) {
                Ok(valid) => {
                    text.push_str(valid);
                    start = self.pending.len();
                }
                Err(error) => {
                    let valid = start + error.valid_up_to();
                    text.push_str(
                        std::str::from_utf8(&self.pending[start..valid]).unwrap_or_default(),
                    );
                    match error.error_len() {
                        Some(length) => {
                            text.push(char::REPLACEMENT_CHARACTER);
                            start = valid + length;
                        }
                        None => {
                            start = valid;
                            break;
                        }
                    }
                }
            }
        }
        self.pending.drain(..start);
        text
    }

    pub fn finish(&mut self) -> String {
        let mut text = self.decode(&[]);
        if !self.pending.is_empty() {
            self.pending.clear();
            text.push(char::REPLACEMENT_CHARACTER);
        }
        text
    }
}

pub trait FrameDecoder: Send + 'static {
    fn push(&mut self, chunk: &[u8]) -> Vec<String>;

    fn finish(&mut self) -> Option<String>;

    fn exhausted(&self) -> bool {
        false
    }
}

#[derive(Debug, Default)]
pub struct SseDecoder {
    text: Utf8Decoder,
    buffer: String,
    done: bool,
}

impl SseDecoder {
    fn next_boundary(&self) -> Option<(usize, usize)> {
        let double = self.buffer.find("\n\n");
        let windows = self.buffer.find("\r\n\r\n");
        match (windows, double) {
            (Some(windows), Some(double)) if windows < double => Some((windows, 4)),
            (Some(windows), None) => Some((windows, 4)),
            (_, Some(double)) => Some((double, 2)),
            (None, None) => None,
        }
    }
}

impl FrameDecoder for SseDecoder {
    fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        if self.done {
            return Vec::new();
        }
        let decoded = self.text.decode(chunk);
        self.buffer.push_str(&decoded);
        let mut payloads = Vec::new();
        while let Some((index, length)) = self.next_boundary() {
            let frame = self.buffer[..index].to_owned();
            self.buffer.drain(..index + length);
            if let Some(payload) = frame_data(&frame) {
                if payload == SSE_DONE {
                    self.done = true;
                    self.buffer.clear();
                    return payloads;
                }
                payloads.push(payload);
            }
        }
        payloads
    }

    fn finish(&mut self) -> Option<String> {
        if self.done {
            return None;
        }
        let rest = self.text.finish();
        self.buffer.push_str(&rest);
        let payload = frame_data(&std::mem::take(&mut self.buffer))?;
        (payload != SSE_DONE).then_some(payload)
    }

    fn exhausted(&self) -> bool {
        self.done
    }
}

#[derive(Debug, Default)]
pub struct NdjsonDecoder {
    text: Utf8Decoder,
    buffer: String,
}

impl FrameDecoder for NdjsonDecoder {
    fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        let decoded = self.text.decode(chunk);
        self.buffer.push_str(&decoded);
        let Some(last) = self.buffer.rfind('\n') else {
            return Vec::new();
        };
        let complete = self.buffer[..last].to_owned();
        self.buffer.drain(..=last);
        complete
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line).trim())
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()
    }

    fn finish(&mut self) -> Option<String> {
        let rest = self.text.finish();
        self.buffer.push_str(&rest);
        let line = std::mem::take(&mut self.buffer);
        let trimmed = line.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    }
}

fn frame_data(frame: &str) -> Option<String> {
    let parts = frame
        .split('\n')
        .map(|raw| raw.strip_suffix('\r').unwrap_or(raw).trim_end())
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|data| data.strip_prefix(' ').unwrap_or(data))
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }
    let joined = parts.join("\n");
    (!joined.is_empty()).then_some(joined)
}

struct Framing<D> {
    inner: Option<ByteStream>,
    decoder: D,
    queue: VecDeque<String>,
}

pub fn frames<D: FrameDecoder>(bytes: ByteStream, decoder: D) -> FrameStream {
    let state = Framing {
        inner: Some(bytes),
        decoder,
        queue: VecDeque::new(),
    };
    futures::stream::unfold(state, |mut state| async move {
        loop {
            if let Some(frame) = state.queue.pop_front() {
                return Some((Ok(frame), state));
            }
            let inner = state.inner.as_mut()?;
            match inner.next().await {
                Some(Ok(chunk)) => {
                    state.queue.extend(state.decoder.push(&chunk));
                    if state.decoder.exhausted() {
                        state.inner = None;
                    }
                }
                Some(Err(error)) => {
                    state.inner = None;
                    state.queue.clear();
                    return Some((Err(error), state));
                }
                None => {
                    state.inner = None;
                    state.queue.extend(state.decoder.finish());
                }
            }
        }
    })
    .boxed()
}

pub fn sse_data(bytes: ByteStream) -> FrameStream {
    frames(bytes, SseDecoder::default())
}

pub fn ndjson_lines(bytes: ByteStream) -> FrameStream {
    frames(bytes, NdjsonDecoder::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::ai::errors::cancelled;
    use zvs_contract::AppErrorCode;

    fn bytes(chunks: Vec<Result<Vec<u8>, AppError>>) -> ByteStream {
        futures::stream::iter(chunks).boxed()
    }

    fn texts(chunks: &[&str]) -> ByteStream {
        bytes(
            chunks
                .iter()
                .map(|chunk| Ok(chunk.as_bytes().to_vec()))
                .collect(),
        )
    }

    async fn collect(stream: FrameStream) -> Vec<Result<String, AppError>> {
        stream.collect().await
    }

    async fn payloads(stream: FrameStream) -> Vec<String> {
        collect(stream)
            .await
            .into_iter()
            .map(|payload| payload.expect("a payload"))
            .collect()
    }

    #[tokio::test]
    async fn the_sse_reader_tolerates_split_chunks_crlf_frames_comments_and_a_missing_terminator() {
        let seen = payloads(sse_data(texts(&[
            ": ping\r\n\r\ndata: {\"a\":1}\r\n\r\ndata: {\"b\"",
            ":2}\n\nevent: other\ndata: {\"c\":3}",
        ])))
        .await;
        assert_eq!(seen, [r#"{"a":1}"#, r#"{"b":2}"#, r#"{"c":3}"#]);
    }

    #[tokio::test]
    async fn multi_line_data_is_joined_and_empty_frames_are_skipped() {
        let seen = payloads(sse_data(texts(&[
            "data: first\ndata:second\r\ndata:  third  \n\n",
            "data:\n\n",
            "id: 7\nretry: 10\n\n",
            "data: last\n\n",
        ])))
        .await;
        assert_eq!(seen, ["first\nsecond\n third", "last"]);
    }

    #[tokio::test]
    async fn done_ends_the_stream_and_nothing_after_it_is_read() {
        let seen = payloads(sse_data(texts(&[
            "data: one\n\ndata: [DONE]\n\ndata: ignored\n\n",
            "data: never\n\n",
        ])))
        .await;
        assert_eq!(seen, ["one"]);
        let trailing = payloads(sse_data(texts(&["data: one\n\ndata: [DONE]"]))).await;
        assert_eq!(trailing, ["one"]);
    }

    #[tokio::test]
    async fn the_earlier_of_the_two_boundaries_wins() {
        let seen = payloads(sse_data(texts(&[
            "data: a\r\n\r\ndata: b\n\ndata: c\r\n\r\n",
        ])))
        .await;
        assert_eq!(seen, ["a", "b", "c"]);
    }

    #[tokio::test]
    async fn a_multibyte_character_split_across_chunks_survives() {
        let text = "data: привет\n\n".as_bytes();
        let split = 9;
        let seen = payloads(sse_data(bytes(vec![
            Ok(text[..split].to_vec()),
            Ok(text[split..].to_vec()),
        ])))
        .await;
        assert_eq!(seen, ["привет"]);
    }

    #[tokio::test]
    async fn a_transport_error_is_passed_through_and_ends_the_stream() {
        let seen = collect(sse_data(bytes(vec![
            Ok(b"data: a\n\ndata: b".to_vec()),
            Err(cancelled()),
            Ok(b"\n\ndata: c\n\n".to_vec()),
        ])))
        .await;
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].as_deref().expect("a payload"), "a");
        assert_eq!(
            seen[1].as_ref().expect_err("an error").code,
            AppErrorCode::RunCancelled
        );
    }

    #[tokio::test]
    async fn ndjson_lines_are_split_trimmed_and_flushed() {
        let seen = payloads(ndjson_lines(texts(&[
            "{\"a\":1}\r\n  \n{\"b\"",
            ":2}\n{\"c\":3}  ",
        ])))
        .await;
        assert_eq!(seen, [r#"{"a":1}"#, r#"{"b":2}"#, r#"{"c":3}"#]);
    }

    #[test]
    fn invalid_utf8_becomes_a_replacement_character() {
        let mut decoder = Utf8Decoder::default();
        assert_eq!(decoder.decode(b"a\xffb"), "a\u{fffd}b");
        assert_eq!(decoder.decode(&[0xd0]), "");
        assert_eq!(decoder.finish(), "\u{fffd}");
        assert_eq!(decoder.decode(&[0xd0]), "");
        assert_eq!(decoder.decode(&[0xbf]), "п");
    }
}
