use serde::Deserialize;
use serde_json::{Number, Value};

#[derive(Deserialize)]
struct Summary {
    content: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Extra {
    summary_thought: Option<Summary>,
    web_search_info: Option<Vec<Value>>,
}

#[derive(Deserialize)]
struct RawSource {
    url: Option<String>,
    title: Option<String>,
    snippet: Option<String>,
    date: Option<String>,
    hostlogo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSource {
    pub url: String,
    pub title: String,
    pub snippet: String,
    pub date: Option<String>,
    pub icon: Option<String>,
}

fn web_address(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn present(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn web_source(row: &Value) -> Option<WebSource> {
    let raw = RawSource::deserialize(row).ok()?;
    let url = present(raw.url)?;
    if !web_address(&url) {
        return None;
    }
    Some(WebSource {
        title: present(raw.title).unwrap_or_else(|| url.clone()),
        snippet: present(raw.snippet).unwrap_or_default(),
        date: present(raw.date),
        icon: present(raw.hostlogo).filter(|icon| web_address(icon)),
        url,
    })
}

#[derive(Deserialize)]
struct RawCall {
    name: Option<String>,
    arguments: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCall {
    pub name: String,
    pub arguments: String,
}

impl ModelCall {
    fn from_raw(raw: RawCall) -> Option<Self> {
        let name = present(raw.name)?;
        let arguments = match raw.arguments {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(text)) => text,
            Some(other) => other.to_string(),
        };
        Some(Self { name, arguments })
    }

    fn continued_by(&self, next: &Self) -> bool {
        self.name == next.name && next.arguments.starts_with(&self.arguments)
    }
}

#[derive(Deserialize)]
struct Delta {
    content: Option<String>,
    phase: Option<String>,
    status: Option<String>,
    extra: Option<Extra>,
    function_call: Option<RawCall>,
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
    success: Option<bool>,
    status: Option<i64>,
    code: Option<VendorCode>,
    error: Option<VendorError>,
}

#[derive(Deserialize)]
struct Created {
    response_id: Option<String>,
    response_index: Option<Value>,
}

#[derive(Deserialize)]
struct StreamContent {
    #[serde(rename = "response.created")]
    created: Option<Created>,
    response_id: Option<String>,
    choices: Option<Vec<Choice>>,
    usage: Option<RawUsage>,
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
    Started(String),
    Reasoning(String),
    Text(String),
    WebSources(Vec<WebSource>),
    ToolCall(ModelCall),
    CallsEnded,
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
    response_id: Option<String>,
    call: Option<ModelCall>,
    ended: bool,
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

    fn thinking(&mut self, delta: Delta) -> Vec<TurnEvent> {
        let thought = delta
            .extra
            .and_then(|extra| extra.summary_thought)
            .and_then(|summary| summary.content)
            .unwrap_or_default();
        self.summary_deltas(&thought)
            .into_iter()
            .map(TurnEvent::Reasoning)
            .collect()
    }

    fn answer(&mut self, delta: Delta) -> Vec<TurnEvent> {
        if let Some(call) = delta.function_call.and_then(ModelCall::from_raw) {
            return self.calling(call);
        }
        if self.call.is_some() {
            return self.hand_over();
        }
        let mut events = Vec::new();
        let content = delta.content.unwrap_or_default();
        if !content.is_empty() {
            events.push(TurnEvent::Text(content));
        }
        if delta.status.as_deref() == Some("finished") {
            events.push(TurnEvent::Finished);
        }
        events
    }

    fn calling(&mut self, next: ModelCall) -> Vec<TurnEvent> {
        if self
            .call
            .as_ref()
            .is_some_and(|current| current.continued_by(&next))
        {
            self.call = Some(next);
            return Vec::new();
        }
        self.call
            .replace(next)
            .map(TurnEvent::ToolCall)
            .into_iter()
            .collect()
    }

    fn hand_over(&mut self) -> Vec<TurnEvent> {
        self.ended = true;
        self.call
            .take()
            .map(TurnEvent::ToolCall)
            .into_iter()
            .chain([TurnEvent::CallsEnded])
            .collect()
    }

    pub fn end(&mut self) -> Vec<TurnEvent> {
        if self.ended || self.call.is_none() {
            return Vec::new();
        }
        self.hand_over()
    }

    /// Decode one payload. Payloads that are not frames (keep-alives, unknown JSON) yield nothing.
    pub fn push(&mut self, payload: &str) -> Vec<TurnEvent> {
        if self.ended {
            return Vec::new();
        }
        let Ok(value) = serde_json::from_str::<Value>(payload) else {
            return Vec::new();
        };
        let Ok(frame) = StreamFrame::deserialize(&value) else {
            return Vec::new();
        };
        if let Some(failure) = failure(&frame) {
            return vec![TurnEvent::Failure(failure)];
        }
        let Ok(content) = StreamContent::deserialize(&value) else {
            return Vec::new();
        };
        if let Some(created) = &content.created {
            let primary = created
                .response_index
                .as_ref()
                .is_none_or(|index| index.as_str() == Some("0") || index.as_u64() == Some(0));
            return match created.response_id.as_deref().map(str::trim) {
                Some(id) if primary && !id.is_empty() && self.response_id.is_none() => {
                    self.response_id = Some(id.to_owned());
                    vec![TurnEvent::Started(id.to_owned())]
                }
                _ => Vec::new(),
            };
        }
        if let (Some(primary), Some(id)) = (&self.response_id, &content.response_id)
            && primary != id
        {
            return Vec::new();
        }
        let mut events = Vec::new();
        if let Some(raw) = &content.usage {
            events.push(TurnEvent::Usage(Usage {
                input_tokens: raw.input_tokens,
                output_tokens: raw.output_tokens,
                total_tokens: raw.total_tokens,
            }));
        }
        let Some(delta) = content
            .choices
            .and_then(|choices| choices.into_iter().next())
            .and_then(|choice| choice.delta)
        else {
            return events;
        };
        events.extend(match delta.phase.as_deref() {
            Some("thinking_summary") => self.thinking(delta),
            Some("answer") => self.answer(delta),
            Some("web_search") => web_search(delta),
            _ => Vec::new(),
        });
        events
    }
}

fn web_search(delta: Delta) -> Vec<TurnEvent> {
    if delta.status.as_deref() != Some("finished") {
        return Vec::new();
    }
    let sources: Vec<WebSource> = delta
        .extra
        .and_then(|extra| extra.web_search_info)
        .unwrap_or_default()
        .iter()
        .filter_map(web_source)
        .collect();
    if sources.is_empty() {
        return Vec::new();
    }
    vec![TurnEvent::WebSources(sources)]
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
    fn a_finished_web_search_yields_its_usable_results_in_order() {
        let mut decoder = TurnDecoder::new();
        let typing = frame(&json!({
            "phase": "web_search", "status": "typing", "content": "",
            "function_call": { "name": "web_search", "arguments": "{\"queries\": " },
        }));
        assert_eq!(decoder.push(&typing), []);
        let finished = frame(&json!({
            "phase": "web_search", "status": "finished", "content": "", "name": "web_search",
            "function_id": "call_1",
            "extra": {
                "display_position": "answer",
                "tool_result": { "docs": [], "used_id": 3 },
                "web_search_info": [
                    { "url": "https://a.example/x", "title": " Курс ", "snippet": "EUR 90",
                      "hostname": "", "hostlogo": "", "date": "3 days ago" },
                    { "url": "ftp://b.example", "title": "skip" },
                    { "url": "  ", "title": "skip" },
                    { "url": "http://c.example", "title": "", "snippet": "", "date": "",
                      "hostlogo": "https://cdn.example/c.png" },
                    { "url": "https://d.example", "title": "d", "hostlogo": "javascript:x" },
                    7,
                ],
            },
        }));
        assert_eq!(
            decoder.push(&finished),
            [TurnEvent::WebSources(vec![
                WebSource {
                    url: "https://a.example/x".into(),
                    title: "Курс".into(),
                    snippet: "EUR 90".into(),
                    date: Some("3 days ago".into()),
                    icon: None,
                },
                WebSource {
                    url: "http://c.example".into(),
                    title: "http://c.example".into(),
                    snippet: String::new(),
                    date: None,
                    icon: Some("https://cdn.example/c.png".into()),
                },
                WebSource {
                    url: "https://d.example".into(),
                    title: "d".into(),
                    snippet: String::new(),
                    date: None,
                    icon: None,
                },
            ])]
        );
        let empty = frame(&json!({
            "phase": "web_search", "status": "finished", "extra": { "web_search_info": [] },
        }));
        assert_eq!(decoder.push(&empty), []);
    }

    #[test]
    fn an_answer_finished_before_a_search_does_not_end_the_text() {
        let mut decoder = TurnDecoder::new();
        let answer = |content: &str, status: &str| {
            frame(&json!({ "phase": "answer", "content": content, "status": status }))
        };
        assert_eq!(
            decoder.push(&answer("Проверка.", "typing")),
            [TurnEvent::Text("Проверка.".into())]
        );
        assert_eq!(decoder.push(&answer("", "finished")), [TurnEvent::Finished]);
        assert_eq!(
            decoder.push(&answer("Итог [[1]].", "typing")),
            [TurnEvent::Text("Итог [[1]].".into())]
        );
    }

    #[test]
    fn only_the_primary_response_is_followed() {
        let mut decoder = TurnDecoder::new();
        let created = |id: &str, index: &str| {
            json!({ "response.created": { "chat_id": "c", "parent_id": "p",
                    "response_id": id, "response_index": index } })
            .to_string()
        };
        let answer = |id: &str, text: &str| {
            json!({ "response_id": id,
                    "choices": [{ "delta": { "phase": "answer", "content": text } }] })
            .to_string()
        };
        assert_eq!(decoder.push(&created("second", "1")), []);
        assert_eq!(
            decoder.push(&created("first", "0")),
            [TurnEvent::Started("first".into())]
        );
        assert_eq!(decoder.push(&created("again", "0")), []);
        assert_eq!(decoder.push(&answer("second", "нет")), []);
        assert_eq!(
            decoder.push(&answer("first", "да")),
            [TurnEvent::Text("да".into())]
        );
        assert_eq!(
            decoder.push(&frame(&json!({ "phase": "answer", "content": "!" }))),
            [TurnEvent::Text("!".into())]
        );
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

    fn call_frame(name: &str, arguments: &str) -> String {
        frame(
            &json!({ "phase": "answer", "status": "typing", "content": "",
                       "function_call": { "name": name, "arguments": arguments },
                       "extra": { "display_position": "answer" } }),
        )
    }

    fn vendor_reply(name: &str) -> String {
        frame(
            &json!({ "phase": "answer", "status": "typing", "name": name,
                       "content": format!("Tool {name} does not exists.") }),
        )
    }

    fn model_call(name: &str, arguments: &str) -> TurnEvent {
        TurnEvent::ToolCall(ModelCall {
            name: name.into(),
            arguments: arguments.into(),
        })
    }

    #[test]
    fn native_calls_are_assembled_from_argument_snapshots_and_handed_over() {
        let mut decoder = TurnDecoder::new();
        let mut events = Vec::new();
        for payload in [
            call_frame("browser__click", ""),
            call_frame("browser__click", r#"{"ref": "e4"#),
            call_frame("browser__click", r#"{"ref": "e42"}"#),
            call_frame("skill__load", ""),
            call_frame("skill__load", r#"{"name": "pdf"}"#),
            call_frame("skill__load", r#"{"name": "pdf"}"#),
            vendor_reply("browser__click"),
            vendor_reply("skill__load"),
            frame(&json!({ "phase": "answer", "status": "finished", "content": "" })),
            frame(&json!({ "phase": "answer", "status": "typing", "content": "I'm sorry" })),
        ] {
            events.extend(decoder.push(&payload));
        }
        assert_eq!(
            events,
            [
                model_call("browser__click", r#"{"ref": "e42"}"#),
                model_call("skill__load", r#"{"name": "pdf"}"#),
                TurnEvent::CallsEnded,
            ]
        );
        assert_eq!(decoder.end(), []);
    }

    #[test]
    fn a_repeated_tool_starts_a_new_call_when_its_arguments_restart() {
        let mut decoder = TurnDecoder::new();
        let mut events = Vec::new();
        for payload in [
            call_frame("fs__read", r#"{"path": "a.md"}"#),
            call_frame("fs__read", ""),
            call_frame("fs__read", r#"{"path": "b.md"}"#),
        ] {
            events.extend(decoder.push(&payload));
        }
        events.extend(decoder.end());
        assert_eq!(
            events,
            [
                model_call("fs__read", r#"{"path": "a.md"}"#),
                model_call("fs__read", r#"{"path": "b.md"}"#),
                TurnEvent::CallsEnded,
            ]
        );
    }

    #[test]
    fn text_before_a_native_call_is_kept_and_a_call_without_a_name_is_ignored() {
        let mut decoder = TurnDecoder::new();
        assert_eq!(
            decoder.push(&frame(
                &json!({ "phase": "answer", "content": "Открываю." })
            )),
            [TurnEvent::Text("Открываю.".into())]
        );
        assert_eq!(decoder.push(&call_frame(" ", "{}")), []);
        assert_eq!(decoder.end(), []);
        assert_eq!(
            decoder.push(&frame(&json!({ "phase": "answer", "content": "!" }))),
            [TurnEvent::Text("!".into())]
        );
    }

    #[test]
    fn failures_survive_malformed_optional_completion_fields() {
        for payload in [
            json!({ "status": 401, "choices": "unavailable" }),
            json!({ "status": 403, "usage": { "input_tokens": "unknown" } }),
            json!({ "success": false, "error": { "code": "TokenExpired" },
                    "choices": [{ "delta": { "content": [] } }] }),
        ] {
            assert!(
                matches!(
                    TurnDecoder::new().push(&payload.to_string()).as_slice(),
                    [TurnEvent::Failure(VendorFailure { auth: true, .. })]
                ),
                "{payload}"
            );
        }
        assert_eq!(
            TurnDecoder::new()
                .push(&json!({ "success": false, "code": "RateLimited", "usage": [] }).to_string()),
            [TurnEvent::Failure(VendorFailure {
                code: Some(json!("RateLimited")),
                status: None,
                auth: false,
            })]
        );
    }
}
