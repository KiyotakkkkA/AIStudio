use std::future::ready;

use futures::StreamExt;
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;
use zvs_contract::AppError;

use crate::drivers::ai::errors::{cancelled, is_cancellation, request_failed};
use crate::drivers::ai::ports::{ChatMessage, ChatRole, ToolSpec};
use crate::drivers::ai::sse::FrameStream;
use crate::drivers::ai::transport::{ByteStream, RequestSpec, SharedTransport};

pub fn object(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect::<Map<String, Value>>(),
    )
}

pub fn field<'a>(value: Option<&'a Value>, key: &str) -> Option<&'a Value> {
    value.and_then(|value| value.get(key))
}

pub fn present(value: Option<&Value>) -> Option<&Value> {
    value.filter(|value| !value.is_null())
}

pub fn first_string(values: &[Option<&Value>]) -> Option<String> {
    values.iter().find_map(|value| {
        value
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    })
}

pub fn first_number(values: &[Option<&Value>]) -> Option<i64> {
    values.iter().find_map(|value| {
        let number = value.and_then(Value::as_number)?;
        number
            .as_i64()
            .or_else(|| number.as_f64().map(|float| float.trunc() as i64))
    })
}

pub fn decode(text: &str) -> Result<Value, AppError> {
    serde_json::from_str(text).map_err(|_| request_failed())
}

pub fn stringify(value: &Value) -> String {
    value.to_string()
}

pub fn normalise(error: AppError) -> AppError {
    if is_cancellation(&error) {
        cancelled()
    } else {
        error
    }
}

pub fn wire_tools(tools: &[ToolSpec]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|tool| {
                object([
                    ("type", Value::from("function")),
                    (
                        "function",
                        object([
                            ("name", Value::from(tool.name.clone())),
                            ("description", Value::from(tool.description.clone())),
                            ("parameters", tool.parameters.clone()),
                        ]),
                    ),
                ])
            })
            .collect(),
    )
}

pub fn plain_message(message: &ChatMessage) -> Value {
    object([
        ("role", Value::from(message.role.as_str())),
        ("content", Value::from(message.content.clone())),
    ])
}

pub fn is_assistant_call(message: &ChatMessage) -> bool {
    message.role == ChatRole::Assistant && !message.tool_calls.is_empty()
}

pub fn open_frames(
    transport: SharedTransport,
    spec: RequestSpec,
    cancel: CancellationToken,
    decode: fn(ByteStream) -> FrameStream,
) -> FrameStream {
    let guard = cancel.clone();
    futures::stream::once(async move {
        match transport.stream(&spec, &cancel).await {
            Ok(bytes) => decode(bytes),
            Err(error) => futures::stream::iter([Err(error)]).boxed(),
        }
    })
    .flatten()
    .map(move |frame| {
        frame.and_then(|payload| {
            if guard.is_cancelled() {
                Err(cancelled())
            } else {
                Ok(payload)
            }
        })
    })
    .scan(false, |failed, frame| {
        if *failed {
            return ready(None);
        }
        *failed = frame.is_err();
        ready(Some(frame))
    })
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn first_string_trims_and_skips_blank_or_non_string_values() {
        let blank = json!("  ");
        let number = json!(3);
        let padded = json!(" gpt ");
        assert_eq!(
            first_string(&[None, Some(&blank), Some(&number), Some(&padded)]),
            Some("gpt".to_owned())
        );
        assert_eq!(first_string(&[Some(&blank)]), None);
    }

    #[test]
    fn first_number_truncates_and_skips_non_numbers() {
        let text = json!("12");
        let float = json!(12.9);
        let negative = json!(-3.5);
        assert_eq!(first_number(&[Some(&text), Some(&float)]), Some(12));
        assert_eq!(first_number(&[Some(&negative)]), Some(-3));
        assert_eq!(first_number(&[None, Some(&text)]), None);
    }

    #[test]
    fn wire_objects_keep_insertion_order() {
        assert_eq!(
            object([("zeta", json!(1)), ("alpha", json!(2))]).to_string(),
            r#"{"zeta":1,"alpha":2}"#
        );
        let parsed: Value = serde_json::from_str(r#"{"query":"rotate","k":3}"#).expect("json");
        assert_eq!(stringify(&parsed), r#"{"query":"rotate","k":3}"#);
    }
}
