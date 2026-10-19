use serde::Deserialize;
use serde_json::{Number, Value};

#[derive(Deserialize)]
struct Summary {
    content: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Extra {
    summary_thought: Option<Summary>,
}

#[derive(Deserialize)]
struct Delta {
    content: Option<String>,
    phase: Option<String>,
    status: Option<String>,
    extra: Option<Extra>,
}

#[derive(Deserialize)]
struct Choice {
    delta: Option<Delta>,
}

#[derive(Deserialize)]
struct RawUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum VendorCode {
    Number(Number),
    Text(String),
}

#[derive(Deserialize)]
struct ErrorDetail {
    code: Option<VendorCode>,
    status: Option<i64>,
}

enum VendorError {
    Text,
    Detail(ErrorDetail),
}

impl<'de> Deserialize<'de> for VendorError {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::String(_) => Ok(Self::Text),
            detail @ Value::Object(_) => ErrorDetail::deserialize(detail)
                .map(Self::Detail)
                .map_err(serde::de::Error::custom),
            _ => Err(serde::de::Error::custom(
                "a vendor error is a string or an object",
            )),
        }
    }
}

#[derive(Deserialize)]
struct StreamFrame {
    choices: Option<Vec<Choice>>,
    usage: Option<RawUsage>,
    success: Option<bool>,
    status: Option<i64>,
    code: Option<VendorCode>,
    error: Option<VendorError>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

/// The vendor ended the turn with an error frame. Messages are left out: they are vendor text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VendorFailure {
    pub code: Option<Value>,
    pub status: Option<i64>,
    /// The code or status says the session is no longer accepted.
    pub auth: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnEvent {
    Reasoning(String),
    Text(String),
    Usage(Usage),
    /// The answer phase reported `finished`; a stream that ends without it was cut short.
    Finished,
    /// The turn failed; nothing after it is meaningful.
    Failure(VendorFailure),
}

fn code_json(code: &VendorCode) -> Value {
    match code {
        VendorCode::Number(number) => Value::Number(number.clone()),
        VendorCode::Text(text) => Value::String(text.clone()),
    }
}

fn is_auth_code(code: Option<&VendorCode>) -> bool {
    match code {
        Some(VendorCode::Number(number)) => {
            matches!(number.as_f64(), Some(value) if value == 401.0 || value == 403.0)
        }
        Some(VendorCode::Text(text)) => {
            if text == "401" || text == "403" {
                return true;
            }
            let lower = text.to_lowercase();
            ["unauth", "forbidden", "token", "login", "expire", "session"]
                .iter()
                .any(|needle| lower.contains(needle))
        }
        None => false,
    }
}

fn failure(frame: &StreamFrame) -> Option<VendorFailure> {
    let (nested_code, nested_status) = match &frame.error {
        Some(VendorError::Detail(detail)) => (detail.code.as_ref(), detail.status),
        _ => (None, None),
    };
    let code = nested_code.or(frame.code.as_ref());
    let status = nested_status.or(frame.status);
    let auth = matches!(status, Some(401 | 403)) || is_auth_code(code);
    let failed = frame.success == Some(false)
        || frame.error.is_some()
        || status.is_some_and(|status| status >= 400);
    (auth || failed).then(|| VendorFailure {
        code: code.map(code_json),
        status,
        auth,
    })
}

/// Turns the `data:` payloads of a completion stream into events. Reasoning summaries arrive
/// as whole snapshots; only their new suffixes are emitted.
#[derive(Debug, Default)]
pub struct TurnDecoder {
    emitted: Vec<String>,
}

impl TurnDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    fn summary_deltas(&mut self, next: &[String]) -> Vec<String> {
        let mut chunks = Vec::new();
        for (index, value) in next.iter().enumerate() {
            let seen = self
                .emitted
                .get(index)
                .map(String::as_str)
                .unwrap_or_default();
            if value == seen {
                continue;
            }
            chunks.push(value.strip_prefix(seen).unwrap_or(value).to_owned());
            if self.emitted.len() <= index {
                self.emitted.resize(index + 1, String::new());
            }
            self.emitted[index].clone_from(value);
        }
        chunks
    }

    /// Decode one payload. Payloads that are not frames (keep-alives, unknown JSON) yield nothing.
    pub fn push(&mut self, payload: &str) -> Vec<TurnEvent> {
        let Ok(value) = serde_json::from_str::<Value>(payload) else {
            return Vec::new();
        };
        let Ok(frame) = StreamFrame::deserialize(&value) else {
            return Vec::new();
        };
        if let Some(failure) = failure(&frame) {
            return vec![TurnEvent::Failure(failure)];
        }
        let mut events = Vec::new();
        if let Some(raw) = &frame.usage {
            events.push(TurnEvent::Usage(Usage {
                input_tokens: raw.input_tokens,
                output_tokens: raw.output_tokens,
                total_tokens: raw.total_tokens,
            }));
        }
        let Some(delta) = frame
            .choices
            .and_then(|choices| choices.into_iter().next())
            .and_then(|choice| choice.delta)
        else {
            return events;
        };
        match delta.phase.as_deref() {
            Some("thinking_summary") => {
                let thought = delta
                    .extra
                    .and_then(|extra| extra.summary_thought)
                    .and_then(|summary| summary.content)
                    .unwrap_or_default();
                events.extend(
                    self.summary_deltas(&thought)
                        .into_iter()
                        .map(TurnEvent::Reasoning),
                );
            }
            Some("answer") => {
                let content = delta.content.unwrap_or_default();
                if !content.is_empty() {
                    events.push(TurnEvent::Text(content));
                }
                if delta.status.as_deref() == Some("finished") {
                    events.push(TurnEvent::Finished);
                }
            }
            _ => {}
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn frame(delta: &Value) -> String {
        json!({ "choices": [{ "delta": delta }] }).to_string()
    }

    #[test]
    fn summaries_emit_suffixes_and_the_answer_finishes_the_turn() {
        let mut decoder = TurnDecoder::new();
        let thought = |parts: &[&str]| {
            frame(&json!({ "phase": "thinking_summary",
                           "extra": { "summary_thought": { "content": parts } } }))
        };
        assert_eq!(
            decoder.push(&thought(&["Ду"])),
            [TurnEvent::Reasoning("Ду".into())]
        );
        assert_eq!(
            decoder.push(&thought(&["Думаю"])),
            [TurnEvent::Reasoning("маю".into())]
        );
        assert_eq!(decoder.push(&thought(&["Думаю"])), []);
        assert_eq!(
            decoder.push(&thought(&["Иначе", "Ещё"])),
            [
                TurnEvent::Reasoning("Иначе".into()),
                TurnEvent::Reasoning("Ещё".into())
            ]
        );
        assert_eq!(
            decoder.push(&frame(&json!({ "phase": "answer", "content": "Привет" }))),
            [TurnEvent::Text("Привет".into())]
        );
        assert_eq!(
            decoder.push(
                &json!({ "usage": { "input_tokens": 3, "output_tokens": 2, "total_tokens": 5 },
                         "choices": [{ "delta": { "phase": "answer", "content": "", "status": "finished" } }] })
                .to_string()
            ),
            [
                TurnEvent::Usage(Usage {
                    input_tokens: Some(3),
                    output_tokens: Some(2),
                    total_tokens: Some(5)
                }),
                TurnEvent::Finished
            ]
        );
        assert_eq!(
            decoder.push(&frame(&json!({ "phase": "think", "content": "x" }))),
            []
        );
        assert_eq!(decoder.push("not json"), []);
    }

    #[test]
    fn error_frames_fail_the_turn_and_auth_codes_are_flagged() {
        let mut decoder = TurnDecoder::new();
        let failed = |value: Value| match TurnDecoder::new().push(&value.to_string()).as_slice() {
            [TurnEvent::Failure(failure)] => Some(failure.clone()),
            _ => None,
        };
        assert_eq!(
            failed(json!({ "success": false, "code": "RateLimited" })),
            Some(VendorFailure {
                code: Some(json!("RateLimited")),
                status: None,
                auth: false
            })
        );
        assert_eq!(
            failed(json!({ "error": { "code": 401, "message": "secret" } })),
            Some(VendorFailure {
                code: Some(json!(401)),
                status: None,
                auth: true
            })
        );
        assert_eq!(
            failed(json!({ "status": 403 })).map(|failure| failure.auth),
            Some(true)
        );
        assert_eq!(
            failed(json!({ "code": "TokenExpired" })).map(|failure| failure.auth),
            Some(true)
        );
        assert_eq!(
            failed(json!({ "error": "busy" })).map(|failure| failure.auth),
            Some(false)
        );
        assert_eq!(failed(json!({ "status": 200, "choices": [] })), None);
        assert_eq!(decoder.push(&json!({ "error": 5 }).to_string()), []);
    }
}
