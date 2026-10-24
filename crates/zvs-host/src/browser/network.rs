use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use url::Url;
use zvs_contract::{
    BrowserHeader, BrowserNetworkDetail, BrowserNetworkEntry, BrowserNetworkLog,
    BrowserRequestState,
};

use crate::platform::paths::utc_iso;

pub const NETWORK_ENTRY_LIMIT: usize = 1000;
pub const POST_DATA_LIMIT: usize = 64 * 1024;
pub const BODY_LIMIT: usize = 1024 * 1024;
pub const NO_NETWORK_ENTRY: &str = "Unknown network entry";
pub const NO_RECORDING_TAB: &str = "No tab to record";

const REDACTED_HEADERS: [&str; 5] = [
    "authorization",
    "cookie",
    "proxy-authorization",
    "set-cookie",
    "x-csrf-token",
];

pub type Headers = Vec<(String, String)>;

#[derive(Debug, Clone, PartialEq)]
pub struct ResponseInfo {
    pub status: u16,
    pub status_text: String,
    pub mime_type: String,
    pub headers: Headers,
    pub from_cache: bool,
    pub encoded_length: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequestInfo {
    pub request_id: String,
    pub url: String,
    pub method: String,
    pub resource_type: String,
    pub headers: Headers,
    pub post_data: Option<String>,
    pub timestamp: f64,
    pub wall_time: f64,
    pub redirect_response: Option<ResponseInfo>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NetworkEvent {
    RequestWillBeSent(Box<RequestInfo>),
    RequestHeaders {
        request_id: String,
        headers: Headers,
    },
    ResponseReceived {
        request_id: String,
        resource_type: String,
        response: Box<ResponseInfo>,
    },
    ServedFromCache {
        request_id: String,
    },
    LoadingFinished {
        request_id: String,
        timestamp: f64,
        encoded_length: f64,
    },
    LoadingFailed {
        request_id: String,
        timestamp: f64,
        error: String,
        canceled: bool,
    },
}

pub fn is_redacted_header(name: &str) -> bool {
    REDACTED_HEADERS
        .iter()
        .any(|redacted| redacted.eq_ignore_ascii_case(name))
}

pub fn redact(headers: &[(String, String)]) -> Vec<BrowserHeader> {
    let mut redacted: Vec<BrowserHeader> = headers
        .iter()
        .map(|(name, value)| {
            let hidden = is_redacted_header(name);
            BrowserHeader {
                name: name.clone(),
                value: if hidden { String::new() } else { value.clone() },
                redacted: hidden,
            }
        })
        .collect();
    redacted.sort_by_key(|header| header.name.to_ascii_lowercase());
    redacted
}

pub fn cap(text: &str, limit: usize) -> (String, bool) {
    if text.len() <= limit {
        return (text.to_owned(), false);
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_owned(), true)
}

#[derive(Debug, Clone)]
struct Record {
    id: String,
    request_id: String,
    hop: bool,
    url: String,
    method: String,
    resource_type: String,
    request_headers: Headers,
    post_data: Option<String>,
    started: f64,
    wall_time: f64,
    state: BrowserRequestState,
    response: Option<ResponseInfo>,
    finished: Option<f64>,
    size: Option<f64>,
    from_cache: bool,
    error: Option<String>,
}

impl Record {
    fn new(request: RequestInfo) -> Self {
        Self {
            id: request.request_id.clone(),
            request_id: request.request_id,
            hop: false,
            url: request.url,
            method: request.method,
            resource_type: request.resource_type,
            request_headers: request.headers,
            post_data: request.post_data.map(|data| cap(&data, POST_DATA_LIMIT).0),
            started: request.timestamp,
            wall_time: request.wall_time,
            state: BrowserRequestState::Pending,
            response: None,
            finished: None,
            size: None,
            from_cache: false,
            error: None,
        }
    }

    fn entry(&self) -> BrowserNetworkEntry {
        BrowserNetworkEntry {
            id: self.id.clone(),
            url: self.url.clone(),
            method: self.method.clone(),
            resource_type: self.resource_type.clone(),
            state: self.state,
            status: self.response.as_ref().map(|response| response.status),
            mime_type: self
                .response
                .as_ref()
                .map(|response| response.mime_type.clone())
                .filter(|mime| !mime.is_empty()),
            started_at: (self.wall_time * 1000.0).round(),
            duration: self
                .finished
                .map(|finished| ((finished - self.started) * 1000.0).max(0.0)),
            size: self.size,
            from_cache: self.from_cache,
            error: self.error.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkLog {
    recording: bool,
    records: VecDeque<Record>,
    live: HashMap<String, String>,
    hops: HashMap<String, u32>,
    revision: u64,
    dropped: u64,
    limit: usize,
}

impl Default for NetworkLog {
    fn default() -> Self {
        Self::with_limit(NETWORK_ENTRY_LIMIT)
    }
}

impl NetworkLog {
    pub fn with_limit(limit: usize) -> Self {
        Self {
            recording: false,
            records: VecDeque::new(),
            live: HashMap::new(),
            hops: HashMap::new(),
            revision: 0,
            dropped: 0,
            limit: limit.max(1),
        }
    }

    pub const fn recording(&self) -> bool {
        self.recording
    }

    pub fn set_recording(&mut self, recording: bool) {
        if self.recording != recording {
            self.recording = recording;
            self.revision += 1;
        }
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.live.clear();
        self.hops.clear();
        self.dropped = 0;
        self.revision += 1;
    }

    fn live_mut(&mut self, request_id: &str) -> Option<&mut Record> {
        let id = self.live.get(request_id)?;
        self.records
            .iter_mut()
            .rev()
            .find(|record| &record.id == id)
    }

    fn push(&mut self, record: Record) {
        self.live
            .insert(record.request_id.clone(), record.id.clone());
        self.records.push_back(record);
        while self.records.len() > self.limit {
            if let Some(oldest) = self.records.pop_front() {
                if self.live.get(&oldest.request_id) == Some(&oldest.id) {
                    self.live.remove(&oldest.request_id);
                }
                self.dropped += 1;
            }
        }
    }

    pub fn apply(&mut self, event: NetworkEvent) -> bool {
        if !self.recording {
            return false;
        }
        let changed = match event {
            NetworkEvent::RequestWillBeSent(request) => {
                let request = *request;
                if let Some(response) = request.redirect_response.clone() {
                    let count = self.hops.entry(request.request_id.clone()).or_insert(0);
                    *count += 1;
                    let hop_id = format!("{}:{}", request.request_id, count);
                    let timestamp = request.timestamp;
                    if let Some(record) = self.live_mut(&request.request_id) {
                        record.id = hop_id;
                        record.hop = true;
                        record.size = response.encoded_length;
                        record.response = Some(response);
                        record.state = BrowserRequestState::Finished;
                        record.finished = Some(timestamp);
                    }
                }
                self.push(Record::new(request));
                true
            }
            NetworkEvent::RequestHeaders {
                request_id,
                headers,
            } => self.live_mut(&request_id).is_some_and(|record| {
                for (name, value) in headers {
                    match record
                        .request_headers
                        .iter_mut()
                        .find(|(known, _)| known.eq_ignore_ascii_case(&name))
                    {
                        Some(existing) => existing.1 = value,
                        None => record.request_headers.push((name, value)),
                    }
                }
                true
            }),
            NetworkEvent::ResponseReceived {
                request_id,
                resource_type,
                response,
            } => self.live_mut(&request_id).is_some_and(|record| {
                record.from_cache |= response.from_cache;
                if !resource_type.is_empty() {
                    record.resource_type = resource_type;
                }
                record.response = Some(*response);
                true
            }),
            NetworkEvent::ServedFromCache { request_id } => {
                self.live_mut(&request_id).is_some_and(|record| {
                    record.from_cache = true;
                    true
                })
            }
            NetworkEvent::LoadingFinished {
                request_id,
                timestamp,
                encoded_length,
            } => self.live_mut(&request_id).is_some_and(|record| {
                record.state = BrowserRequestState::Finished;
                record.finished = Some(timestamp);
                record.size = Some(encoded_length);
                true
            }),
            NetworkEvent::LoadingFailed {
                request_id,
                timestamp,
                error,
                canceled,
            } => self.live_mut(&request_id).is_some_and(|record| {
                record.state = BrowserRequestState::Failed;
                record.finished = Some(timestamp);
                record.error = Some(if canceled && error.is_empty() {
                    "canceled".to_owned()
                } else {
                    error
                });
                true
            }),
        };
        if changed {
            self.revision += 1;
        }
        changed
    }

    pub fn snapshot(&self, tab_id: Option<String>) -> BrowserNetworkLog {
        BrowserNetworkLog {
            tab_id,
            recording: self.recording,
            revision: self.revision,
            dropped: self.dropped,
            entries: self.records.iter().map(Record::entry).collect(),
        }
    }

    pub fn detail(&self, id: &str) -> Option<(BrowserNetworkDetail, Option<String>)> {
        self.records
            .iter()
            .find(|record| record.id == id)
            .map(Self::record_detail)
    }

    pub fn details(&self) -> Vec<(BrowserNetworkDetail, Option<String>)> {
        self.records.iter().map(Self::record_detail).collect()
    }

    fn record_detail(record: &Record) -> (BrowserNetworkDetail, Option<String>) {
        let body_source = (!record.hop && record.state == BrowserRequestState::Finished)
            .then(|| record.request_id.clone());
        let detail = BrowserNetworkDetail {
            entry: record.entry(),
            status_text: record
                .response
                .as_ref()
                .map(|response| response.status_text.clone())
                .filter(|text| !text.is_empty()),
            request_headers: redact(&record.request_headers),
            response_headers: record
                .response
                .as_ref()
                .map_or_else(Vec::new, |response| redact(&response.headers)),
            post_data: record.post_data.clone(),
            body: None,
        };
        (detail, body_source)
    }
}

fn har_headers(headers: &[BrowserHeader]) -> Vec<Value> {
    headers
        .iter()
        .filter(|header| !header.redacted)
        .map(|header| json!({ "name": header.name, "value": header.value }))
        .collect()
}

fn header_value<'a>(headers: &'a [BrowserHeader], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|header| !header.redacted && header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str())
}

fn har_entry(detail: &BrowserNetworkDetail) -> Value {
    let entry = &detail.entry;
    let query: Vec<Value> = Url::parse(&entry.url)
        .map(|url| {
            url.query_pairs()
                .map(|(name, value)| json!({ "name": name, "value": value }))
                .collect()
        })
        .unwrap_or_default();
    let mut request = json!({
        "method": entry.method,
        "url": entry.url,
        "httpVersion": "",
        "cookies": [],
        "headers": har_headers(&detail.request_headers),
        "queryString": query,
        "headersSize": -1,
        "bodySize": detail.post_data.as_ref().map_or(0, String::len),
    });
    if let Some(text) = &detail.post_data {
        request["postData"] = json!({
            "mimeType": header_value(&detail.request_headers, "content-type").unwrap_or(""),
            "text": text,
        });
    }
    let mut content = json!({
        "size": entry.size.unwrap_or(0.0),
        "mimeType": entry.mime_type.as_deref().unwrap_or(""),
    });
    if let Some(body) = &detail.body {
        content["text"] = json!(body.text);
        if body.base64 {
            content["encoding"] = json!("base64");
        }
    }
    let time = entry.duration.unwrap_or(0.0);
    let mut har = json!({
        "startedDateTime": utc_iso(entry.started_at as i64),
        "time": time,
        "request": request,
        "response": {
            "status": entry.status.unwrap_or(0),
            "statusText": detail.status_text.as_deref().unwrap_or(""),
            "httpVersion": "",
            "cookies": [],
            "headers": har_headers(&detail.response_headers),
            "content": content,
            "redirectURL": header_value(&detail.response_headers, "location").unwrap_or(""),
            "headersSize": -1,
            "bodySize": entry.size.unwrap_or(-1.0),
        },
        "cache": {},
        "timings": { "send": 0, "wait": time, "receive": 0 },
        "_resourceType": entry.resource_type,
    });
    if let Some(error) = &entry.error {
        har["_error"] = json!(error);
    }
    har
}

pub fn har(details: &[BrowserNetworkDetail], creator: &str, version: &str) -> Value {
    json!({
        "log": {
            "version": "1.2",
            "creator": { "name": creator, "version": version },
            "pages": [],
            "entries": details.iter().map(har_entry).collect::<Vec<_>>(),
        }
    })
}

#[async_trait]
pub trait NetworkCapture: Send + Sync {
    async fn start(&self, tab_id: Option<String>) -> Result<BrowserNetworkLog, String>;

    async fn stop(&self, tab_id: String) -> Result<BrowserNetworkLog, String>;

    async fn clear(&self, tab_id: String) -> Result<BrowserNetworkLog, String>;

    async fn log(&self, tab_id: String) -> Result<BrowserNetworkLog, String>;

    async fn detail(
        &self,
        tab_id: String,
        entry_id: String,
    ) -> Result<BrowserNetworkDetail, String>;

    async fn har(&self, tab_id: String) -> Result<Value, String>;
}

pub type SharedNetworkCapture = Arc<dyn NetworkCapture>;

pub async fn record_for(
    capture: &dyn NetworkCapture,
    tab_id: Option<String>,
    window: Duration,
) -> Result<BrowserNetworkLog, String> {
    let started = capture.start(tab_id).await?;
    let tab_id = started.tab_id.ok_or(NO_RECORDING_TAB)?;
    capture.clear(tab_id.clone()).await?;
    tokio::time::sleep(window).await;
    capture.stop(tab_id).await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn request(id: &str, url: &str, timestamp: f64) -> RequestInfo {
        RequestInfo {
            request_id: id.to_owned(),
            url: url.to_owned(),
            method: "GET".to_owned(),
            resource_type: "Document".to_owned(),
            headers: vec![("Accept".to_owned(), "*/*".to_owned())],
            post_data: None,
            timestamp,
            wall_time: 1_800_000_000.0 + timestamp,
            redirect_response: None,
        }
    }

    fn response(status: u16) -> ResponseInfo {
        ResponseInfo {
            status,
            status_text: "OK".to_owned(),
            mime_type: "text/html".to_owned(),
            headers: vec![
                ("Set-Cookie".to_owned(), "sid=secret".to_owned()),
                ("Content-Type".to_owned(), "text/html".to_owned()),
            ],
            from_cache: false,
            encoded_length: None,
        }
    }

    fn recording() -> NetworkLog {
        let mut log = NetworkLog::default();
        log.set_recording(true);
        log
    }

    #[test]
    fn nothing_is_kept_while_not_recording() {
        let mut log = NetworkLog::default();
        assert!(!log.apply(NetworkEvent::RequestWillBeSent(Box::new(request(
            "1",
            "https://a.example/",
            1.0
        )))));
        assert!(log.snapshot(None).entries.is_empty());
    }

    #[test]
    fn a_request_is_folded_from_sent_to_finished() {
        let mut log = recording();
        log.apply(NetworkEvent::RequestWillBeSent(Box::new(request(
            "1",
            "https://a.example/",
            10.0,
        ))));
        log.apply(NetworkEvent::RequestHeaders {
            request_id: "1".to_owned(),
            headers: vec![
                ("accept".to_owned(), "text/html".to_owned()),
                ("Cookie".to_owned(), "sid=secret".to_owned()),
            ],
        });
        log.apply(NetworkEvent::ResponseReceived {
            request_id: "1".to_owned(),
            resource_type: "Document".to_owned(),
            response: Box::new(response(200)),
        });
        log.apply(NetworkEvent::LoadingFinished {
            request_id: "1".to_owned(),
            timestamp: 10.25,
            encoded_length: 512.0,
        });
        let snapshot = log.snapshot(Some("tab".to_owned()));
        assert_eq!(snapshot.tab_id.as_deref(), Some("tab"));
        let entry = &snapshot.entries[0];
        assert_eq!(entry.state, BrowserRequestState::Finished);
        assert_eq!(entry.status, Some(200));
        assert_eq!(entry.duration, Some(250.0));
        assert_eq!(entry.size, Some(512.0));
        assert_eq!(entry.started_at, 1_800_000_010_000.0);

        let (detail, body) = log.detail("1").expect("known");
        assert_eq!(body.as_deref(), Some("1"));
        assert_eq!(
            detail.request_headers,
            vec![
                BrowserHeader {
                    name: "Accept".to_owned(),
                    value: "text/html".to_owned(),
                    redacted: false,
                },
                BrowserHeader {
                    name: "Cookie".to_owned(),
                    value: String::new(),
                    redacted: true,
                },
            ]
        );
        assert!(
            detail
                .response_headers
                .iter()
                .all(|header| !header.value.contains("secret"))
        );
        assert_eq!(detail.status_text.as_deref(), Some("OK"));
    }

    #[test]
    fn a_redirect_becomes_its_own_finished_hop() {
        let mut log = recording();
        log.apply(NetworkEvent::RequestWillBeSent(Box::new(request(
            "7",
            "http://a.example/",
            1.0,
        ))));
        let mut next = request("7", "https://a.example/", 1.5);
        next.redirect_response = Some(response(301));
        log.apply(NetworkEvent::RequestWillBeSent(Box::new(next)));
        log.apply(NetworkEvent::LoadingFailed {
            request_id: "7".to_owned(),
            timestamp: 2.0,
            error: "net::ERR_ABORTED".to_owned(),
            canceled: false,
        });
        let entries = log.snapshot(None).entries;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "7:1");
        assert_eq!(entries[0].status, Some(301));
        assert_eq!(entries[0].state, BrowserRequestState::Finished);
        assert_eq!(entries[1].id, "7");
        assert_eq!(entries[1].state, BrowserRequestState::Failed);
        assert_eq!(entries[1].error.as_deref(), Some("net::ERR_ABORTED"));
        assert_eq!(log.detail("7:1").expect("hop").1, None);
        assert_eq!(log.detail("7").expect("failed").1, None);
    }

    #[test]
    fn the_oldest_entries_are_dropped_past_the_limit() {
        let mut log = NetworkLog::with_limit(2);
        log.set_recording(true);
        for id in ["1", "2", "3"] {
            log.apply(NetworkEvent::RequestWillBeSent(Box::new(request(
                id,
                "https://a.example/",
                1.0,
            ))));
        }
        assert!(!log.apply(NetworkEvent::LoadingFinished {
            request_id: "1".to_owned(),
            timestamp: 2.0,
            encoded_length: 1.0,
        }));
        let snapshot = log.snapshot(None);
        assert_eq!(snapshot.dropped, 1);
        assert_eq!(
            snapshot
                .entries
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["2", "3"]
        );
        log.clear();
        assert_eq!(log.snapshot(None).dropped, 0);
        assert!(log.snapshot(None).entries.is_empty());
    }

    #[test]
    fn post_data_and_bodies_are_capped_on_a_char_boundary() {
        let mut big = request("1", "https://a.example/", 1.0);
        big.post_data = Some("я".repeat(POST_DATA_LIMIT));
        let mut log = recording();
        log.apply(NetworkEvent::RequestWillBeSent(Box::new(big)));
        let (detail, _) = log.detail("1").expect("known");
        assert!(detail.post_data.expect("kept").len() <= POST_DATA_LIMIT);
        assert_eq!(cap("ab", 5), ("ab".to_owned(), false));
        assert_eq!(cap("яя", 3), ("я".to_owned(), true));
    }

    #[test]
    fn credentials_headers_are_redacted_case_insensitively() {
        for name in [
            "Authorization",
            "COOKIE",
            "set-cookie",
            "Proxy-Authorization",
        ] {
            assert!(is_redacted_header(name), "{name}");
        }
        assert!(!is_redacted_header("Content-Type"));
    }

    #[derive(Default)]
    struct FakeCapture {
        calls: Mutex<Vec<String>>,
    }

    fn empty(tab_id: &str, recording: bool) -> BrowserNetworkLog {
        BrowserNetworkLog {
            tab_id: Some(tab_id.to_owned()),
            recording,
            revision: 0,
            dropped: 0,
            entries: Vec::new(),
        }
    }

    #[async_trait]
    impl NetworkCapture for FakeCapture {
        async fn start(&self, tab_id: Option<String>) -> Result<BrowserNetworkLog, String> {
            self.calls
                .lock()
                .expect("lock")
                .push(format!("start:{}", tab_id.as_deref().unwrap_or("active")));
            Ok(empty("tab", true))
        }

        async fn stop(&self, tab_id: String) -> Result<BrowserNetworkLog, String> {
            self.calls
                .lock()
                .expect("lock")
                .push(format!("stop:{tab_id}"));
            Ok(empty(&tab_id, false))
        }

        async fn clear(&self, tab_id: String) -> Result<BrowserNetworkLog, String> {
            self.calls
                .lock()
                .expect("lock")
                .push(format!("clear:{tab_id}"));
            Ok(empty(&tab_id, true))
        }

        async fn log(&self, tab_id: String) -> Result<BrowserNetworkLog, String> {
            Ok(empty(&tab_id, true))
        }

        async fn detail(
            &self,
            _tab_id: String,
            _entry_id: String,
        ) -> Result<BrowserNetworkDetail, String> {
            Err(NO_NETWORK_ENTRY.to_owned())
        }

        async fn har(&self, _tab_id: String) -> Result<Value, String> {
            Ok(har(&[], "test", "0"))
        }
    }

    #[test]
    fn a_har_log_carries_requests_without_credentials() {
        let mut log = recording();
        let mut sent = request("1", "https://a.example/api?q=1&lang=ru", 10.0);
        sent.method = "POST".to_owned();
        sent.post_data = Some("{\"a\":1}".to_owned());
        sent.headers = vec![
            ("Content-Type".to_owned(), "application/json".to_owned()),
            ("Authorization".to_owned(), "Bearer secret".to_owned()),
        ];
        log.apply(NetworkEvent::RequestWillBeSent(Box::new(sent)));
        log.apply(NetworkEvent::ResponseReceived {
            request_id: "1".to_owned(),
            resource_type: "Fetch".to_owned(),
            response: Box::new(response(200)),
        });
        log.apply(NetworkEvent::LoadingFinished {
            request_id: "1".to_owned(),
            timestamp: 10.5,
            encoded_length: 7.0,
        });
        let mut details: Vec<BrowserNetworkDetail> = log
            .details()
            .into_iter()
            .map(|(detail, _)| detail)
            .collect();
        details[0].body = Some(zvs_contract::BrowserNetworkBody {
            text: "ok".to_owned(),
            base64: false,
            truncated: false,
        });
        let archive = har(&details, "ZVS AI Studio", "1.0.0");
        let entry = &archive["log"]["entries"][0];
        assert_eq!(archive["log"]["version"], "1.2");
        assert_eq!(archive["log"]["creator"]["name"], "ZVS AI Studio");
        assert_eq!(entry["startedDateTime"], "2027-01-15T08:00:10.000Z");
        assert_eq!(entry["time"], 500.0);
        assert_eq!(entry["request"]["method"], "POST");
        assert_eq!(entry["request"]["queryString"][1]["value"], "ru");
        assert_eq!(entry["request"]["postData"]["mimeType"], "application/json");
        assert_eq!(entry["response"]["status"], 200);
        assert_eq!(entry["response"]["content"]["text"], "ok");
        assert_eq!(entry["_resourceType"], "Fetch");
        let text = archive.to_string();
        assert!(!text.contains("secret"), "{text}");
        assert!(!text.contains("Authorization"), "{text}");
    }

    #[tokio::test(start_paused = true)]
    async fn recording_for_a_window_starts_clears_waits_and_stops() {
        let capture = FakeCapture::default();
        let started = tokio::time::Instant::now();
        let log = record_for(&capture, None, Duration::from_secs(30))
            .await
            .expect("records");
        assert!(started.elapsed() >= Duration::from_secs(30));
        assert!(!log.recording);
        assert_eq!(
            *capture.calls.lock().expect("lock"),
            vec!["start:active", "clear:tab", "stop:tab"]
        );
    }
}
