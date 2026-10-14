use std::net::SocketAddr;
use std::time::Duration;

use reqwest::{Client, Proxy};
use zvs_contract::{AppError, AppErrorCode};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
pub const USER_AGENT: &str = concat!("ZVSAIStudio/", env!("CARGO_PKG_VERSION"));
pub const CLIENT_FAILED_MESSAGE: &str = "Не удалось подготовить HTTP-клиент";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ProxyMode {
    #[default]
    System,
    Explicit(String),
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpClientOptions {
    pub proxy: ProxyMode,
    pub connect_timeout: Duration,
    pub resolve: Vec<(String, SocketAddr)>,
}

impl Default for HttpClientOptions {
    fn default() -> Self {
        Self {
            proxy: ProxyMode::System,
            connect_timeout: CONNECT_TIMEOUT,
            resolve: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpClient {
    client: Client,
}

impl HttpClient {
    pub fn new() -> Result<Self, AppError> {
        Self::with_options(HttpClientOptions::default())
    }

    pub fn with_options(options: HttpClientOptions) -> Result<Self, AppError> {
        let mut builder = Client::builder()
            .use_rustls_tls()
            .tls_built_in_native_certs(true)
            .user_agent(USER_AGENT)
            .connect_timeout(options.connect_timeout);
        builder = match options.proxy {
            ProxyMode::System => builder,
            ProxyMode::Explicit(url) => builder.proxy(Proxy::all(url).map_err(|_| failed())?),
            ProxyMode::Off => builder.no_proxy(),
        };
        for (domain, address) in &options.resolve {
            builder = builder.resolve(domain, *address);
        }
        builder
            .build()
            .map(|client| Self { client })
            .map_err(|_| failed())
    }

    pub const fn client(&self) -> &Client {
        &self.client
    }
}

fn failed() -> AppError {
    AppError::new(AppErrorCode::Unknown, CLIENT_FAILED_MESSAGE)
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::drivers::ai::mock::{MockServer, Reply};

    const CHILD_FLAG: &str = "ZVS_PROXY_CHILD_TARGET";

    #[test]
    fn the_shared_client_builds_with_native_roots_and_the_system_proxy() {
        assert!(HttpClient::new().is_ok());
        assert!(
            HttpClient::with_options(HttpClientOptions {
                proxy: ProxyMode::Off,
                ..HttpClientOptions::default()
            })
            .is_ok()
        );
        let refused = HttpClient::with_options(HttpClientOptions {
            proxy: ProxyMode::Explicit("::not a proxy::".to_owned()),
            ..HttpClientOptions::default()
        })
        .expect_err("refuses");
        assert_eq!(refused.message, CLIENT_FAILED_MESSAGE);
        assert!(USER_AGENT.starts_with("ZVSAIStudio/"));
    }

    #[tokio::test]
    async fn an_explicit_proxy_receives_the_request_in_absolute_form() {
        let proxy = MockServer::start().await;
        proxy.fallback(Reply::json(serde_json::json!({ "via": "proxy" })));
        let client = HttpClient::with_options(HttpClientOptions {
            proxy: ProxyMode::Explicit(proxy.origin()),
            ..HttpClientOptions::default()
        })
        .expect("a client");
        let response = client
            .client()
            .get("http://upstream.invalid/v1/models")
            .send()
            .await
            .expect("the proxy answers");
        assert_eq!(response.status(), 200);
        let calls = proxy.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].host.as_deref(), Some("upstream.invalid"));
        assert_eq!(calls[0].path, "/v1/models");
        assert_eq!(calls[0].header("user-agent").as_deref(), Some(USER_AGENT));
        assert_eq!(calls[0].header("cookie"), None);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_default_client_honours_the_proxy_the_operating_system_reports() {
        let proxy = MockServer::start().await;
        proxy.fallback(Reply::json(serde_json::json!({ "via": "system proxy" })));
        let executable = std::env::current_exe().expect("the test binary");
        let origin = proxy.origin();
        let output = tokio::task::spawn_blocking(move || {
            Command::new(executable)
                .args([
                    "drivers::ai::http::tests::system_proxy_child",
                    "--exact",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env_remove("http_proxy")
                .env_remove("all_proxy")
                .env_remove("no_proxy")
                .env_remove("REQUEST_METHOD")
                .env(CHILD_FLAG, "http://upstream.invalid/system")
                .env("HTTP_PROXY", origin)
                .output()
        })
        .await
        .expect("joins")
        .expect("the child runs");
        assert!(
            output.status.success(),
            "{}
{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let calls = proxy.calls();
        assert_eq!(calls.len(), 1, "{calls:?}");
        assert_eq!(calls[0].host.as_deref(), Some("upstream.invalid"));
        assert_eq!(calls[0].path, "/system");
    }

    #[tokio::test]
    #[ignore = "run by the_default_client_honours_the_proxy_the_operating_system_reports"]
    async fn system_proxy_child() {
        let Ok(target) = std::env::var(CHILD_FLAG) else {
            return;
        };
        let response = HttpClient::new()
            .expect("a client")
            .client()
            .get(target)
            .send()
            .await
            .expect("the system proxy answers");
        assert_eq!(response.status(), 200);
    }
}
