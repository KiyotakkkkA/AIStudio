use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use tauri::ipc::Channel;
use tauri::{Runtime, State, Webview};

use crate::window::STUDIO;

const VALIDATION_MESSAGE: &str = "Некорректные данные запроса";

#[derive(Default)]
pub struct EventChannel(Mutex<Option<Channel<Value>>>);

#[tauri::command]
pub async fn ipc_call<R: Runtime>(
    webview: Webview<R>,
    channel: String,
    payload: Option<Value>,
) -> Result<Value, String> {
    ensure_studio(&webview)?;
    Ok(dispatch(
        &channel,
        &payload.unwrap_or(Value::Null),
        now_millis(),
    ))
}

#[tauri::command]
pub async fn events_subscribe<R: Runtime>(
    webview: Webview<R>,
    events: State<'_, EventChannel>,
    channel: Channel<Value>,
) -> Result<(), String> {
    ensure_studio(&webview)?;
    let mut slot = events
        .0
        .lock()
        .map_err(|_| "Канал событий недоступен".to_owned())?;
    *slot = Some(channel);
    Ok(())
}

fn ensure_studio<R: Runtime>(webview: &Webview<R>) -> Result<(), String> {
    if webview.label() == STUDIO {
        Ok(())
    } else {
        Err(format!(
            "Вебвью «{}» не может вызывать хост",
            webview.label()
        ))
    }
}

fn dispatch(channel: &str, payload: &Value, now: i64) -> Value {
    match channel {
        "system.ping" => ping(payload, now),
        _ => failure(
            "NOT_IMPLEMENTED",
            &format!("Канал «{channel}» ещё не перенесён в Tauri-хост"),
        ),
    }
}

fn ping(payload: &Value, now: i64) -> Value {
    match payload.get("sentAt").and_then(Value::as_i64) {
        Some(sent_at) if sent_at >= 0 => json!({
            "ok": true,
            "data": { "pong": true, "hostTime": now, "roundTripHint": now - sent_at },
        }),
        _ => failure("VALIDATION_FAILED", VALIDATION_MESSAGE),
    }
}

fn failure(code: &str, message: &str) -> Value {
    json!({ "ok": false, "error": { "code": code, "message": message } })
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_answers_with_host_time_and_round_trip_hint() {
        let response = dispatch("system.ping", &json!({ "sentAt": 1_000 }), 1_250);
        assert_eq!(
            response,
            json!({ "ok": true, "data": { "pong": true, "hostTime": 1_250, "roundTripHint": 250 } })
        );
    }

    #[test]
    fn ping_rejects_a_payload_without_sent_at() {
        for payload in [
            Value::Null,
            json!({}),
            json!({ "sentAt": "now" }),
            json!({ "sentAt": -1 }),
        ] {
            let response = dispatch("system.ping", &payload, 1_250);
            assert_eq!(response["ok"], json!(false));
            assert_eq!(response["error"]["code"], json!("VALIDATION_FAILED"));
            assert_eq!(response["error"]["message"], json!(VALIDATION_MESSAGE));
        }
    }

    #[test]
    fn every_other_channel_is_not_implemented() {
        for channel in ["providers.list", "system.demoStream", "unknown"] {
            let response = dispatch(channel, &Value::Null, 0);
            assert_eq!(response["ok"], json!(false));
            assert_eq!(response["error"]["code"], json!("NOT_IMPLEMENTED"));
            assert!(
                response["error"]["message"]
                    .as_str()
                    .is_some_and(|message| message.contains(channel))
            );
        }
    }
}
