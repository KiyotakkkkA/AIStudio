use serde::Deserialize;
use serde_json::Value;

use crate::page::CAPTCHA_REQUIRED_CODE;

const COMPLETION_TYPE: &str = "chat:completion";
const REASONING_CLOSE: &str = "</details>";

#[derive(Deserialize)]
struct PromptDetails {
    cached_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct RawUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
    prompt_tokens_details: Option<PromptDetails>,
}

#[derive(Deserialize)]
struct RawError {
    code: Option<Value>,
    error_code: Option<Value>,
    captcha_error_type: Option<String>,
}

#[derive(Deserialize)]
struct Completion {
    phase: Option<String>,
    delta_content: Option<String>,
    edit_content: Option<String>,
    done: Option<bool>,
    usage: Option<RawUsage>,
    error: Option<RawError>,
    data: Option<Box<Completion>>,
}

#[derive(Deserialize)]
struct StreamFrame {
    #[serde(rename = "type")]
    kind: Option<String>,
    data: Option<Completion>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cached_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VendorFailure {
    pub code: Option<String>,
    pub captcha: bool,
    pub auth: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnEvent {
    Reasoning(String),
    Text(String),
    Usage(Usage),
    Finished,
    Failure(VendorFailure),
}

fn code_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_owned()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn failure(error: &RawError) -> VendorFailure {
    let code = error
        .error_code
        .as_ref()
        .and_then(code_text)
        .or_else(|| error.code.as_ref().and_then(code_text));
    let lower = code.as_deref().unwrap_or_default().to_ascii_lowercase();
    VendorFailure {
        captcha: error.captcha_error_type.is_some()
            || code.as_deref() == Some(CAPTCHA_REQUIRED_CODE),
        auth: lower == "401"
            || lower == "403"
            || ["unauth", "forbidden", "token", "login", "expire"]
                .iter()
                .any(|needle| lower.contains(needle)),
        code,
    }
}

fn find_error(completion: &Completion) -> Option<&RawError> {
    completion
        .error
        .as_ref()
        .or_else(|| completion.data.as_deref().and_then(find_error))
}

fn strip_reasoning_markup(chunk: &str) -> String {
    let mut rest = chunk;
    if let Some(start) = rest.find("<details")
        && let Some(end) = rest[start..].find('>')
    {
        rest = &rest[start + end + 1..];
    }
    if let Some(start) = rest.find("<summary>")
        && let Some(end) = rest[start..].find("</summary>")
    {
        rest = &rest[start + end + "</summary>".len()..];
    }
    let text = rest.replace("\n> ", "\n");
    text.strip_prefix("> ")
        .map(str::to_owned)
        .unwrap_or(text)
        .trim_start_matches('\n')
        .to_owned()
}

#[derive(Debug, Default)]
pub struct TurnDecoder {
    started: bool,
}

impl TurnDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, payload: &str) -> Vec<TurnEvent> {
        let Ok(frame) = serde_json::from_str::<StreamFrame>(payload) else {
            return Vec::new();
        };
        if frame.kind.as_deref() != Some(COMPLETION_TYPE) {
            return Vec::new();
        }
        let Some(completion) = frame.data else {
            return Vec::new();
        };
        if let Some(error) = find_error(&completion) {
            return vec![TurnEvent::Failure(failure(error))];
        }
        let mut events = Vec::new();
        if let Some(raw) = &completion.usage {
            events.push(TurnEvent::Usage(Usage {
                input_tokens: raw.prompt_tokens,
                output_tokens: raw.completion_tokens,
                total_tokens: raw.total_tokens,
                cached_tokens: raw
                    .prompt_tokens_details
                    .as_ref()
                    .and_then(|details| details.cached_tokens),
            }));
        }
        match completion.phase.as_deref() {
            Some("thinking") => {
                let chunk = completion.delta_content.unwrap_or_default();
                let reasoning = if self.started {
                    chunk.replace("\n> ", "\n")
                } else {
                    strip_reasoning_markup(&chunk)
                };
                self.started = true;
                if !reasoning.is_empty() {
                    events.push(TurnEvent::Reasoning(reasoning));
                }
            }
            Some("answer") => {
                let text = match completion.edit_content {
                    Some(edit) => match edit.rfind(REASONING_CLOSE) {
                        Some(end) => edit[end + REASONING_CLOSE.len()..]
                            .trim_start_matches('\n')
                            .to_owned(),
                        None => edit,
                    },
                    None => completion.delta_content.unwrap_or_default(),
                };
                if !text.is_empty() {
                    events.push(TurnEvent::Text(text));
                }
            }
            _ => {}
        }
        if completion.done == Some(true) || completion.phase.as_deref() == Some("done") {
            events.push(TurnEvent::Finished);
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn frame(data: &Value) -> String {
        json!({ "type": "chat:completion", "data": data }).to_string()
    }

    #[test]
    fn an_observed_turn_yields_usage_text_and_the_finish() {
        let mut decoder = TurnDecoder::new();
        assert_eq!(
            decoder.push(&frame(&json!({ "phase": "other", "usage": {
                "prompt_tokens": 21_601, "completion_tokens": 3, "total_tokens": 21_604,
                "prompt_tokens_details": { "cached_tokens": 21_440 } } }))),
            [TurnEvent::Usage(Usage {
                input_tokens: Some(21_601),
                output_tokens: Some(3),
                total_tokens: Some(21_604),
                cached_tokens: Some(21_440),
            })]
        );
        assert_eq!(
            decoder.push(&frame(
                &json!({ "delta_content": "Привет", "phase": "answer" })
            )),
            [TurnEvent::Text("Привет".into())]
        );
        assert_eq!(
            decoder.push(&frame(&json!({ "phase": "done", "done": true }))),
            [TurnEvent::Finished]
        );
        assert_eq!(decoder.push("[DONE]"), []);
        assert_eq!(
            decoder.push(&json!({ "type": "chat:title", "data": "x" }).to_string()),
            []
        );
    }

    #[test]
    fn reasoning_markup_is_stripped_and_the_edit_keeps_only_the_answer() {
        let mut decoder = TurnDecoder::new();
        assert_eq!(
            decoder.push(&frame(&json!({ "phase": "thinking",
                "delta_content": "<details type=\"reasoning\" done=\"false\">\n> Счи" }))),
            [TurnEvent::Reasoning("Счи".into())]
        );
        assert_eq!(
            decoder.push(&frame(
                &json!({ "phase": "thinking", "delta_content": "таю\n> ещё" })
            )),
            [TurnEvent::Reasoning("таю\nещё".into())]
        );
        assert_eq!(
            decoder.push(&frame(&json!({ "phase": "answer", "edit_index": 40,
                "edit_content": "\n</details>\nОтвет" }))),
            [TurnEvent::Text("Ответ".into())]
        );
    }

    #[test]
    fn the_observed_captcha_refusal_is_a_captcha_failure() {
        let error = json!({ "captcha_error_type": "missing_param",
            "code": "FRONTEND_CAPTCHA_REQUIRED", "detail": "secret",
            "error_code": "FRONTEND_CAPTCHA_REQUIRED" });
        let refusal = frame(
            &json!({ "data": { "done": true, "error": error }, "done": true, "error": error }),
        );
        assert_eq!(
            TurnDecoder::new().push(&refusal),
            [TurnEvent::Failure(VendorFailure {
                code: Some(CAPTCHA_REQUIRED_CODE.to_owned()),
                captcha: true,
                auth: false,
            })]
        );
        let nested = frame(&json!({ "data": { "error": { "code": "TokenExpired" } } }));
        assert_eq!(
            TurnDecoder::new().push(&nested),
            [TurnEvent::Failure(VendorFailure {
                code: Some("TokenExpired".to_owned()),
                captcha: false,
                auth: true,
            })]
        );
    }
}
