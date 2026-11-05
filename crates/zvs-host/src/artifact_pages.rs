use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use url::Url;
use zvs_contract::{AppError, AppErrorCode};

pub const PAGE_POLICY: &str = "default-src 'none'; script-src 'unsafe-inline'; \
    style-src 'unsafe-inline'; img-src data: blob:; font-src data:; connect-src 'none'; \
    form-action 'none'; base-uri 'none'; frame-ancestors 'none'";
pub const LIVE_PAGE_POLICY: &str = "default-src 'none'; script-src 'unsafe-inline'; \
    style-src 'unsafe-inline'; img-src data: blob:; font-src data:; connect-src 'self'; \
    form-action 'none'; base-uri 'none'; frame-ancestors 'none'";
pub const PAGES_UNAVAILABLE: &str = "Встроенный браузер недоступен";
pub const NOT_A_PAGE: &str = "Этот файл нельзя открыть во встроенном браузере";
pub const PAGE_SERVER_FAILED: &str = "Не удалось подготовить файл для встроенного браузера";
const BODY_MAX: usize = 4 * 1024;

const PAGE_TYPES: &[(&str, &str)] = &[
    ("html", "text/html; charset=utf-8"),
    ("htm", "text/html; charset=utf-8"),
    ("svg", "image/svg+xml"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("pdf", "application/pdf"),
];

#[async_trait]
pub trait PageOpener: Send + Sync {
    async fn open(&self, url: &str) -> Result<(), AppError>;
}

pub type SharedPageOpener = Arc<dyn PageOpener>;

pub type PageResolver = Arc<dyn Fn(&str) -> Result<PathBuf, AppError> + Send + Sync>;

#[async_trait]
pub trait LivePages: Send + Sync {
    fn is_live(&self, id: &str) -> bool;
    fn state(&self, id: &str) -> Result<Value, AppError>;
    fn data(&self, id: &str) -> Result<Vec<u8>, AppError>;
    async fn refresh(&self, id: &str, automatic: bool) -> Result<Value, AppError>;
    fn set_interval(&self, id: &str, interval: Option<u32>) -> Result<Value, AppError>;
}

pub type SharedLivePages = Arc<dyn LivePages>;

pub fn page_type(name: &str) -> Option<&'static str> {
    let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
    PAGE_TYPES
        .iter()
        .find(|(known, _)| *known == extension)
        .map(|(_, content_type)| *content_type)
}

fn failed(cause: impl ToString) -> AppError {
    AppError::new(AppErrorCode::Unknown, PAGE_SERVER_FAILED).with_detail("raw", cause.to_string())
}

#[derive(Clone)]
struct Served {
    token: String,
    host: String,
    resolve: PageResolver,
    live: Option<SharedLivePages>,
}

impl Served {
    fn admits(&self, token: &str, headers: &HeaderMap) -> bool {
        let host = headers
            .get(header::HOST)
            .and_then(|value| value.to_str().ok());
        token == self.token && host == Some(self.host.as_str())
    }

    fn same_origin(&self, headers: &HeaderMap) -> bool {
        headers
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
            .is_none_or(|origin| origin == format!("http://{}", self.host))
    }
}

pub struct PageServer {
    token: String,
    address: SocketAddr,
}

impl PageServer {
    pub async fn start(
        resolve: PageResolver,
        live: Option<SharedLivePages>,
    ) -> Result<Self, AppError> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(failed)?;
        let address = listener.local_addr().map_err(failed)?;
        let token = uuid::Uuid::new_v4().simple().to_string();
        let state = Served {
            token: token.clone(),
            host: address.to_string(),
            resolve,
            live,
        };
        let app = Router::new()
            .route("/:token/:run/:name", get(serve))
            .route(
                "/:token/:run/:name/:action",
                get(read_live).post(write_live),
            )
            .fallback(|| async { StatusCode::NOT_FOUND })
            .with_state(state);
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok(Self { token, address })
    }

    pub fn url(&self, id: &str) -> Result<String, AppError> {
        let (run, name) = id
            .split_once('/')
            .ok_or_else(|| AppError::new(AppErrorCode::ValidationFailed, NOT_A_PAGE))?;
        let mut url = Url::parse(&format!("http://{}/", self.address)).map_err(failed)?;
        url.path_segments_mut()
            .map_err(|()| failed("not a base url"))?
            .clear()
            .push(&self.token)
            .push(run)
            .push(name);
        Ok(url.into())
    }
}

fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

fn secured(mut response: Response, content_type: &'static str, policy: &'static str) -> Response {
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(policy),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

async fn serve(
    State(state): State<Served>,
    UrlPath((token, run, name)): UrlPath<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    if !state.admits(&token, &headers) {
        return not_found();
    }
    let Some(content_type) = page_type(&name) else {
        return not_found();
    };
    let id = format!("{run}/{name}");
    let Ok(path) = (state.resolve)(&id) else {
        return not_found();
    };
    let Ok(Ok(bytes)) = tokio::task::spawn_blocking(move || std::fs::read(path)).await else {
        return not_found();
    };
    let live = state.live.as_ref().is_some_and(|live| live.is_live(&id));
    let policy = if live { LIVE_PAGE_POLICY } else { PAGE_POLICY };
    secured(bytes.into_response(), content_type, policy)
}

fn answer(result: Result<Value, AppError>) -> Response {
    let (status, body) = match result {
        Ok(value) => (StatusCode::OK, value),
        Err(error) => {
            let status = match error.code {
                AppErrorCode::NotFound => StatusCode::NOT_FOUND,
                AppErrorCode::PermissionDenied => StatusCode::FORBIDDEN,
                AppErrorCode::ValidationFailed => StatusCode::BAD_REQUEST,
                AppErrorCode::Conflict => StatusCode::CONFLICT,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                json!({ "error": error.message, "code": error.code }),
            )
        }
    };
    secured(
        (status, body.to_string()).into_response(),
        "application/json; charset=utf-8",
        PAGE_POLICY,
    )
}

fn live_page(
    state: &Served,
    token: &str,
    id: String,
    headers: &HeaderMap,
) -> Option<(SharedLivePages, String)> {
    if !state.admits(token, headers) {
        return None;
    }
    let live = state.live.clone()?;
    (state.resolve)(&id).ok()?;
    live.is_live(&id).then_some((live, id))
}

async fn read_live(
    State(state): State<Served>,
    UrlPath((token, run, name, action)): UrlPath<(String, String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let Some((live, id)) = live_page(&state, &token, format!("{run}/{name}"), &headers) else {
        return not_found();
    };
    match action.as_str() {
        "state" => answer(live.state(&id)),
        "data" => match live.data(&id) {
            Ok(bytes) => secured(
                bytes.into_response(),
                "application/json; charset=utf-8",
                PAGE_POLICY,
            ),
            Err(error) => answer(Err(error)),
        },
        _ => not_found(),
    }
}

async fn write_live(
    State(state): State<Served>,
    UrlPath((token, run, name, action)): UrlPath<(String, String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some((live, id)) = live_page(&state, &token, format!("{run}/{name}"), &headers) else {
        return not_found();
    };
    if !state.same_origin(&headers) || body.len() > BODY_MAX {
        return StatusCode::FORBIDDEN.into_response();
    }
    let input: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    match action.as_str() {
        "refresh" => answer(live.refresh(&id, input["automatic"] == json!(true)).await),
        "interval" => {
            let interval = match &input["interval"] {
                Value::Null => None,
                value => match value.as_u64().and_then(|value| u32::try_from(value).ok()) {
                    Some(value) => Some(value),
                    None => {
                        return answer(Err(zvs_contract::validate::invalid("interval", "type")));
                    }
                },
            };
            answer(live.set_interval(&id, interval))
        }
        _ => not_found(),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::Mutex;

    use super::*;

    fn send(
        method: &str,
        url: &str,
        host: Option<&str>,
        origin: Option<&str>,
        body: &str,
    ) -> String {
        let parsed = Url::parse(url).expect("a url");
        let address = format!(
            "{}:{}",
            parsed.host_str().expect("a host"),
            parsed.port().expect("a port")
        );
        let mut stream = TcpStream::connect(&address).expect("connects");
        let origin = origin
            .map(|origin| format!("Origin: {origin}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "{method} {} HTTP/1.1\r\nHost: {}\r\n{origin}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            parsed.path(),
            host.unwrap_or(&address),
            body.len()
        );
        stream.write_all(request.as_bytes()).expect("writes");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("reads");
        response
    }

    fn fetch(url: &str, host: Option<&str>) -> String {
        send("GET", url, host, None, "")
    }

    fn resolver(base: PathBuf) -> PageResolver {
        Arc::new(move |id: &str| {
            let (_, name) = id.split_once('/').expect("an id");
            let path = base.join(name);
            if path.is_file() {
                Ok(path)
            } else {
                Err(AppError::new(AppErrorCode::NotFound, "missing"))
            }
        })
    }

    #[test]
    fn pages_are_typed_by_extension() {
        assert_eq!(page_type("Отчёт.HTML"), Some("text/html; charset=utf-8"));
        assert_eq!(page_type("chart.svg"), Some("image/svg+xml"));
        assert_eq!(page_type("table.xlsx"), None);
        assert_eq!(page_type("noextension"), None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_page_is_served_only_with_the_token_and_the_loopback_host() {
        let folder = tempfile::tempdir().expect("a folder");
        let page = folder.path().join("Продажи 2026.html");
        std::fs::write(&page, "<p>ok</p>").expect("writes");
        std::fs::write(folder.path().join("data.csv"), "a,b").expect("writes");
        let server = PageServer::start(resolver(folder.path().to_path_buf()), None)
            .await
            .expect("starts");
        let url = server.url("dashboards/Продажи 2026.html").expect("a url");
        assert!(url.starts_with("http://127.0.0.1:"));
        let address = url.clone();
        let response = tokio::task::spawn_blocking(move || fetch(&address, None))
            .await
            .expect("fetches");
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.contains("content-security-policy: default-src 'none'"));
        assert!(response.contains("connect-src 'none'"));
        assert!(response.ends_with("<p>ok</p>"));

        let rebound = url.clone();
        let response = tokio::task::spawn_blocking(move || fetch(&rebound, Some("evil.test")))
            .await
            .expect("fetches");
        assert!(response.starts_with("HTTP/1.1 404"), "{response}");

        let wrong = url.replace(&server.token, "0".repeat(32).as_str());
        let response = tokio::task::spawn_blocking(move || fetch(&wrong, None))
            .await
            .expect("fetches");
        assert!(response.starts_with("HTTP/1.1 404"), "{response}");

        let data = server.url("dashboards/data.csv").expect("a url");
        let response = tokio::task::spawn_blocking(move || fetch(&data, None))
            .await
            .expect("fetches");
        assert!(response.starts_with("HTTP/1.1 404"), "{response}");

        let state = format!("{url}/state");
        let response = tokio::task::spawn_blocking(move || fetch(&state, None))
            .await
            .expect("fetches");
        assert!(response.starts_with("HTTP/1.1 404"), "{response}");
    }

    #[derive(Default)]
    struct FakeLive {
        refreshed: Mutex<Vec<(String, bool)>>,
        intervals: Mutex<Vec<Option<u32>>>,
    }

    #[async_trait]
    impl LivePages for FakeLive {
        fn is_live(&self, id: &str) -> bool {
            id == "dashboards/live.html"
        }

        fn state(&self, _id: &str) -> Result<Value, AppError> {
            Ok(json!({ "interval": 60 }))
        }

        fn data(&self, _id: &str) -> Result<Vec<u8>, AppError> {
            Ok(br#"{"title":"x"}"#.to_vec())
        }

        async fn refresh(&self, id: &str, automatic: bool) -> Result<Value, AppError> {
            self.refreshed
                .lock()
                .expect("a lock")
                .push((id.to_owned(), automatic));
            Err(AppError::new(AppErrorCode::PermissionDenied, "approve"))
        }

        fn set_interval(&self, _id: &str, interval: Option<u32>) -> Result<Value, AppError> {
            self.intervals.lock().expect("a lock").push(interval);
            Ok(json!({ "interval": interval }))
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_live_page_may_reach_only_its_own_endpoints() {
        let folder = tempfile::tempdir().expect("a folder");
        std::fs::write(folder.path().join("live.html"), "<p>live</p>").expect("writes");
        std::fs::write(folder.path().join("static.html"), "<p>static</p>").expect("writes");
        let live = Arc::new(FakeLive::default());
        let shared: SharedLivePages = live.clone();
        let server = PageServer::start(resolver(folder.path().to_path_buf()), Some(shared))
            .await
            .expect("starts");
        let page = server.url("dashboards/live.html").expect("a url");
        let fixed = server.url("dashboards/static.html").expect("a url");
        let origin = format!(
            "http://{}",
            Url::parse(&page)
                .expect("a url")
                .host_str()
                .map_or_else(String::new, |host| {
                    format!(
                        "{host}:{}",
                        Url::parse(&page).expect("a url").port().expect("a port")
                    )
                })
        );
        let run = move |method: &'static str,
                        url: String,
                        origin: Option<String>,
                        body: &'static str| {
            tokio::task::spawn_blocking(move || send(method, &url, None, origin.as_deref(), body))
        };

        let response = run("GET", page.clone(), None, "").await.expect("fetches");
        assert!(response.contains("connect-src 'self'"), "{response}");
        let response = run("GET", fixed.clone(), None, "").await.expect("fetches");
        assert!(response.contains("connect-src 'none'"), "{response}");
        let response = run("GET", format!("{fixed}/state"), None, "")
            .await
            .expect("fetches");
        assert!(response.starts_with("HTTP/1.1 404"), "{response}");

        let response = run("GET", format!("{page}/state"), None, "")
            .await
            .expect("fetches");
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.ends_with(r#"{"interval":60}"#), "{response}");
        let response = run("GET", format!("{page}/data"), None, "")
            .await
            .expect("fetches");
        assert!(response.ends_with(r#"{"title":"x"}"#), "{response}");

        let response = run(
            "POST",
            format!("{page}/refresh"),
            Some(origin.clone()),
            r#"{"automatic":true}"#,
        )
        .await
        .expect("fetches");
        assert!(response.starts_with("HTTP/1.1 403"), "{response}");
        assert!(response.contains(r#""error":"approve""#), "{response}");
        let response = run(
            "POST",
            format!("{page}/refresh"),
            Some("http://evil.test".to_owned()),
            "{}",
        )
        .await
        .expect("fetches");
        assert!(response.starts_with("HTTP/1.1 403"), "{response}");
        assert_eq!(
            *live.refreshed.lock().expect("a lock"),
            vec![("dashboards/live.html".to_owned(), true)]
        );

        let response = run(
            "POST",
            format!("{page}/interval"),
            Some(origin.clone()),
            r#"{"interval":300}"#,
        )
        .await
        .expect("fetches");
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        let response = run(
            "POST",
            format!("{page}/interval"),
            Some(origin),
            r#"{"interval":"x"}"#,
        )
        .await
        .expect("fetches");
        assert!(response.starts_with("HTTP/1.1 400"), "{response}");
        assert_eq!(*live.intervals.lock().expect("a lock"), vec![Some(300)]);
    }
}
