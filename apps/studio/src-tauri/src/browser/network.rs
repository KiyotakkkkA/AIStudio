use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use zvs_contract::{BrowserNetworkBody, BrowserNetworkDetail, BrowserNetworkLog};
use zvs_host::browser::network::{
    BODY_LIMIT, Headers, NO_NETWORK_ENTRY, POST_DATA_LIMIT, RequestInfo, ResponseInfo, cap,
};
use zvs_host::browser::{NetworkEvent, NetworkLog};

use crate::browser::cdp::{CdpTransport, DEFAULT_CALL_TIMEOUT};

pub const NETWORK_EVENTS: [&str; 6] = [
    "Network.requestWillBeSent",
    "Network.requestWillBeSentExtraInfo",
    "Network.responseReceived",
    "Network.requestServedFromCache",
    "Network.loadingFinished",
    "Network.loadingFailed",
];

const NO_REPLY: &str = "DevTools did not reply";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CdpRequest {
    url: String,
    #[serde(default)]
    url_fragment: Option<String>,
    method: String,
    #[serde(default)]
    headers: BTreeMap<String, Value>,
    #[serde(default)]
    post_data: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CdpResponse {
    status: f64,
    #[serde(default)]
    status_text: String,
    #[serde(default)]
    mime_type: String,
    #[serde(default)]
    headers: BTreeMap<String, Value>,
    #[serde(default)]
    from_disk_cache: bool,
    #[serde(default)]
    from_prefetch_cache: bool,
    #[serde(default)]
    from_service_worker: bool,
    #[serde(default)]
    encoded_data_length: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestWillBeSent {
    request_id: String,
    request: CdpRequest,
    timestamp: f64,
    wall_time: f64,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    redirect_response: Option<CdpResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtraInfo {
    request_id: String,
    #[serde(default)]
    headers: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseReceived {
    request_id: String,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    response: CdpResponse,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestOnly {
    request_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadingFinished {
    request_id: String,
    timestamp: f64,
    #[serde(default)]
    encoded_data_length: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadingFailed {
    request_id: String,
    timestamp: f64,
    #[serde(default)]
    error_text: String,
    #[serde(default)]
    canceled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseBody {
    body: String,
    #[serde(default)]
    base64_encoded: bool,
}

fn headers(raw: BTreeMap<String, Value>) -> Headers {
    raw.into_iter()
        .map(|(name, value)| {
            let value = match value {
                Value::String(text) => text,
                other => other.to_string(),
            };
            (name, value)
        })
        .collect()
}

fn response(raw: CdpResponse) -> ResponseInfo {
    ResponseInfo {
        status: raw.status.round().clamp(0.0, f64::from(u16::MAX)) as u16,
        status_text: raw.status_text,
        mime_type: raw.mime_type,
        headers: headers(raw.headers),
        from_cache: raw.from_disk_cache || raw.from_prefetch_cache || raw.from_service_worker,
        encoded_length: raw.encoded_data_length,
    }
}

pub fn decode(event: &str, payload: &str) -> Option<NetworkEvent> {
    match event {
        "Network.requestWillBeSent" => {
            let sent: RequestWillBeSent = serde_json::from_str(payload).ok()?;
            let url = match sent.request.url_fragment {
                Some(fragment) => format!("{}{fragment}", sent.request.url),
                None => sent.request.url,
            };
            Some(NetworkEvent::RequestWillBeSent(Box::new(RequestInfo {
                request_id: sent.request_id,
                url,
                method: sent.request.method,
                resource_type: sent.kind.unwrap_or_else(|| "Other".to_owned()),
                headers: headers(sent.request.headers),
                post_data: sent.request.post_data,
                timestamp: sent.timestamp,
                wall_time: sent.wall_time,
                redirect_response: sent.redirect_response.map(response),
            })))
        }
        "Network.requestWillBeSentExtraInfo" => {
            let extra: ExtraInfo = serde_json::from_str(payload).ok()?;
            Some(NetworkEvent::RequestHeaders {
                request_id: extra.request_id,
                headers: headers(extra.headers),
            })
        }
        "Network.responseReceived" => {
            let received: ResponseReceived = serde_json::from_str(payload).ok()?;
            Some(NetworkEvent::ResponseReceived {
                request_id: received.request_id,
                resource_type: received.kind.unwrap_or_default(),
                response: Box::new(response(received.response)),
            })
        }
        "Network.requestServedFromCache" => {
            let served: RequestOnly = serde_json::from_str(payload).ok()?;
            Some(NetworkEvent::ServedFromCache {
                request_id: served.request_id,
            })
        }
        "Network.loadingFinished" => {
            let finished: LoadingFinished = serde_json::from_str(payload).ok()?;
            Some(NetworkEvent::LoadingFinished {
                request_id: finished.request_id,
                timestamp: finished.timestamp,
                encoded_length: finished.encoded_data_length,
            })
        }
        "Network.loadingFailed" => {
            let failed: LoadingFailed = serde_json::from_str(payload).ok()?;
            Some(NetworkEvent::LoadingFailed {
                request_id: failed.request_id,
                timestamp: failed.timestamp,
                error: failed.error_text,
                canceled: failed.canceled,
            })
        }
        _ => None,
    }
}

fn call(
    transport: &dyn CdpTransport,
    method: &str,
    params: &Value,
    timeout: Duration,
) -> Result<Value, String> {
    let (reply, answer) = mpsc::channel();
    transport.call(
        method,
        params.to_string(),
        Box::new(move |outcome| {
            let _ = reply.send(outcome);
        }),
    );
    let text = answer
        .recv_timeout(timeout)
        .map_err(|_| NO_REPLY.to_owned())??;
    if text.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

pub struct NetworkRecorder {
    transport: Arc<dyn CdpTransport>,
    log: Arc<Mutex<NetworkLog>>,
    listening: AtomicBool,
    timeout: Duration,
}

impl NetworkRecorder {
    pub fn new(transport: Arc<dyn CdpTransport>) -> Self {
        Self::with_timeout(transport, DEFAULT_CALL_TIMEOUT)
    }

    pub fn with_timeout(transport: Arc<dyn CdpTransport>, timeout: Duration) -> Self {
        Self {
            transport,
            log: Arc::default(),
            listening: AtomicBool::new(false),
            timeout,
        }
    }

    fn listen(&self) -> Result<(), String> {
        if self.listening.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        for event in NETWORK_EVENTS {
            let log = Arc::downgrade(&self.log);
            let listened = self.transport.listen(
                event,
                Arc::new(move |payload| {
                    if let (Some(log), Some(decoded)) = (log.upgrade(), decode(event, &payload)) {
                        lock(&log).apply(decoded);
                    }
                }),
            );
            if let Err(error) = listened {
                self.listening.store(false, Ordering::SeqCst);
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn start(&self) -> Result<(), String> {
        self.listen()?;
        call(
            self.transport.as_ref(),
            "Network.enable",
            &json!({ "maxPostDataSize": POST_DATA_LIMIT }),
            self.timeout,
        )?;
        lock(&self.log).set_recording(true);
        Ok(())
    }

    pub fn stop(&self) {
        lock(&self.log).set_recording(false);
    }

    pub fn clear(&self) {
        lock(&self.log).clear();
    }

    pub fn snapshot(&self, tab_id: Option<String>) -> BrowserNetworkLog {
        lock(&self.log).snapshot(tab_id)
    }

    fn body(&self, request_id: &str) -> Option<BrowserNetworkBody> {
        call(
            self.transport.as_ref(),
            "Network.getResponseBody",
            &json!({ "requestId": request_id }),
            self.timeout,
        )
        .ok()
        .and_then(|value| serde_json::from_value::<ResponseBody>(value).ok())
        .map(|body| {
            let (text, truncated) = cap(&body.body, BODY_LIMIT);
            BrowserNetworkBody {
                text,
                base64: body.base64_encoded,
                truncated,
            }
        })
    }

    pub fn detail(&self, id: &str) -> Result<BrowserNetworkDetail, String> {
        let (mut detail, source) = lock(&self.log).detail(id).ok_or(NO_NETWORK_ENTRY)?;
        if let Some(request_id) = source {
            detail.body = self.body(&request_id);
        }
        Ok(detail)
    }

    pub fn export(&self) -> Vec<BrowserNetworkDetail> {
        let details = lock(&self.log).details();
        details
            .into_iter()
            .map(|(mut detail, source)| {
                if let Some(request_id) = source {
                    detail.body = self.body(&request_id);
                }
                detail
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use zvs_contract::BrowserRequestState;

    use super::*;
    use crate::browser::cdp::testing::FakeCdp;

    fn recorder_for(fake: &Arc<FakeCdp>) -> NetworkRecorder {
        NetworkRecorder::with_timeout(fake.clone(), Duration::from_secs(5))
    }

    fn sent(id: &str) -> Value {
        json!({
            "requestId": id,
            "loaderId": "L",
            "documentURL": "https://a.example/",
            "request": {
                "url": "https://a.example/api",
                "urlFragment": "#top",
                "method": "POST",
                "headers": { "Content-Type": "application/json" },
                "postData": "{\"q\":1}",
            },
            "timestamp": 100.0,
            "wallTime": 1_800_000_000.0,
            "type": "Fetch",
            "initiator": { "type": "script" },
        })
    }

    #[test]
    fn cdp_events_decode_into_network_events() {
        let Some(NetworkEvent::RequestWillBeSent(request)) =
            decode("Network.requestWillBeSent", &sent("1").to_string())
        else {
            panic!("decodes");
        };
        assert_eq!(request.url, "https://a.example/api#top");
        assert_eq!(request.method, "POST");
        assert_eq!(request.resource_type, "Fetch");
        assert_eq!(request.post_data.as_deref(), Some("{\"q\":1}"));
        assert_eq!(
            decode(
                "Network.responseReceived",
                &json!({
                    "requestId": "1",
                    "timestamp": 100.1,
                    "type": "Fetch",
                    "response": {
                        "url": "https://a.example/api",
                        "status": 201,
                        "statusText": "Created",
                        "headers": { "Content-Type": "application/json" },
                        "mimeType": "application/json",
                        "fromDiskCache": true,
                        "connectionReused": false,
                    },
                })
                .to_string(),
            ),
            Some(NetworkEvent::ResponseReceived {
                request_id: "1".to_owned(),
                resource_type: "Fetch".to_owned(),
                response: Box::new(ResponseInfo {
                    status: 201,
                    status_text: "Created".to_owned(),
                    mime_type: "application/json".to_owned(),
                    headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
                    from_cache: true,
                    encoded_length: None,
                }),
            })
        );
        assert_eq!(decode("Network.loadingFinished", "not json"), None);
        assert_eq!(decode("Network.dataReceived", "{}"), None);
    }

    #[test]
    fn recording_enables_the_domain_and_folds_events_in_order() {
        let fake = FakeCdp::shared();
        fake.handle(|method, params| match method {
            "Network.enable" => {
                assert_eq!(params["maxPostDataSize"], POST_DATA_LIMIT);
                Some(Ok(json!({})))
            }
            "Network.getResponseBody" => Some(Ok(json!({ "body": "ok", "base64Encoded": false }))),
            _ => None,
        });
        let recorder = recorder_for(&fake);
        fake.emit("Network.requestWillBeSent", &sent("0"));
        recorder.start().expect("starts");
        recorder.start().expect("starts again");
        assert_eq!(
            fake.listens.lock().expect("lock").len(),
            NETWORK_EVENTS.len()
        );
        fake.emit("Network.requestWillBeSent", &sent("1"));
        fake.emit(
            "Network.loadingFinished",
            &json!({ "requestId": "1", "timestamp": 100.5, "encodedDataLength": 42 }),
        );
        let log = recorder.snapshot(Some("tab".to_owned()));
        assert!(log.recording);
        assert_eq!(log.entries.len(), 1);
        assert_eq!(log.entries[0].state, BrowserRequestState::Finished);
        assert_eq!(log.entries[0].duration, Some(500.0));

        let detail = recorder.detail("1").expect("detail");
        assert_eq!(
            detail.body,
            Some(BrowserNetworkBody {
                text: "ok".to_owned(),
                base64: false,
                truncated: false,
            })
        );
        assert_eq!(detail.post_data.as_deref(), Some("{\"q\":1}"));
        assert!(recorder.detail("missing").is_err());
        let exported = recorder.export();
        assert_eq!(exported.len(), 1);
        assert_eq!(
            exported[0].body.as_ref().map(|body| body.text.as_str()),
            Some("ok")
        );

        recorder.stop();
        fake.emit("Network.requestWillBeSent", &sent("2"));
        assert_eq!(recorder.snapshot(None).entries.len(), 1);
        recorder.clear();
        assert!(recorder.snapshot(None).entries.is_empty());
    }

    #[test]
    fn a_refused_enable_leaves_recording_off_and_a_missing_body_is_absent() {
        let fake = FakeCdp::shared();
        fake.handle(|method, _| match method {
            "Network.enable" => Some(Err("0x80004005".to_owned())),
            _ => None,
        });
        let recorder = recorder_for(&fake);
        assert!(recorder.start().is_err());
        assert!(!recorder.snapshot(None).recording);

        let fake = FakeCdp::shared();
        fake.handle(|method, _| match method {
            "Network.enable" => Some(Ok(json!({}))),
            "Network.getResponseBody" => {
                Some(Err(r#"{"code":-32000,"message":"No resource"}"#.to_owned()))
            }
            _ => None,
        });
        let recorder = recorder_for(&fake);
        recorder.start().expect("starts");
        fake.emit("Network.requestWillBeSent", &sent("1"));
        fake.emit(
            "Network.loadingFinished",
            &json!({ "requestId": "1", "timestamp": 101.0, "encodedDataLength": 0 }),
        );
        assert_eq!(recorder.detail("1").expect("detail").body, None);
    }
}
