use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tokio::net::TcpListener;
use url::Url;
use zvs_contract::{AppError, AppErrorCode};

pub const PAGE_POLICY: &str = "default-src 'none'; script-src 'unsafe-inline'; \
    style-src 'unsafe-inline'; img-src data: blob:; font-src data:; connect-src 'none'; \
    form-action 'none'; base-uri 'none'; frame-ancestors 'none'";
pub const PAGES_UNAVAILABLE: &str = "Встроенный браузер недоступен";
pub const NOT_A_PAGE: &str = "Этот файл нельзя открыть во встроенном браузере";
pub const PAGE_SERVER_FAILED: &str = "Не удалось подготовить файл для встроенного браузера";

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
}

pub struct PageServer {
    token: String,
    address: SocketAddr,
}

impl PageServer {
    pub async fn start(resolve: PageResolver) -> Result<Self, AppError> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(failed)?;
        let address = listener.local_addr().map_err(failed)?;
        let token = uuid::Uuid::new_v4().simple().to_string();
        let state = Served {
            token: token.clone(),
            host: address.to_string(),
            resolve,
        };
        let app = Router::new()
            .route("/:token/:run/:name", get(serve))
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

async fn serve(
    State(state): State<Served>,
    UrlPath((token, run, name)): UrlPath<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    if token != state.token || host != Some(state.host.as_str()) {
        return not_found();
    }
    let Some(content_type) = page_type(&name) else {
        return not_found();
    };
    let Ok(path) = (state.resolve)(&format!("{run}/{name}")) else {
        return not_found();
    };
    let Ok(Ok(bytes)) = tokio::task::spawn_blocking(move || std::fs::read(path)).await else {
        return not_found();
    };
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(PAGE_POLICY),
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

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    use super::*;

    fn fetch(url: &str, host: Option<&str>) -> String {
        let parsed = Url::parse(url).expect("a url");
        let address = format!(
            "{}:{}",
            parsed.host_str().expect("a host"),
            parsed.port().expect("a port")
        );
        let mut stream = TcpStream::connect(&address).expect("connects");
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            parsed.path(),
            host.unwrap_or(&address)
        );
        stream.write_all(request.as_bytes()).expect("writes");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("reads");
        response
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
        let base = folder.path().to_path_buf();
        let resolve: PageResolver = Arc::new(move |id: &str| {
            let (_, name) = id.split_once('/').expect("an id");
            let path = base.join(name);
            if path.is_file() {
                Ok(path)
            } else {
                Err(AppError::new(AppErrorCode::NotFound, "missing"))
            }
        });
        let server = PageServer::start(resolve).await.expect("starts");
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
    }
}
