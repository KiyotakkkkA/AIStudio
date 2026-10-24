use std::convert::Infallible;
use std::future::{Future, pending};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures::{StreamExt, stream};
use serde_json::{Value, json};
use tauri::window::WindowBuilder;
use tauri::{AppHandle, Manager};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppErrorCode, BrowserCommand};
use zvs_host::browser::BrowserLifecycle;
use zvs_host::drivers::ai::{AccountToken, SessionGateway, SessionRequest};
use zvs_host::platform::logger::silent_logger;
use zvs_host::testing::temporary_directory;
use zvs_studio_lib::browser::cdp::{CdpClient, Evaluate, WebviewCdp};
use zvs_studio_lib::browser::gateway::GatewayOptions;
use zvs_studio_lib::browser::{AccountGateway, BearerTokens, BrowserWorkspace, WebviewPages};

const STEP: Duration = Duration::from_secs(30);
const BIG: usize = 1024 * 1024 + 3;

#[derive(Default)]
struct Fixture {
    authorizations: Mutex<Vec<String>>,
    second: Notify,
    forever_closed: AtomicBool,
}

struct Closed(Arc<Fixture>);

impl Drop for Closed {
    fn drop(&mut self) {
        self.0.forever_closed.store(true, Ordering::SeqCst);
    }
}

fn big() -> Vec<u8> {
    (0..BIG).map(|index| (index * 31 % 251) as u8).collect()
}

fn html(body: &str) -> Response {
    ([(header::CONTENT_TYPE, "text/html")], body.to_owned()).into_response()
}

fn chunked(
    chunks: impl futures::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, "text/event-stream")
        .body(Body::from_stream(chunks))
        .expect("a response")
}

async fn echo(method: Method, headers: HeaderMap, body: String) -> Response {
    let text = |name: header::HeaderName| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    let reply = json!({
        "method": method.as_str(),
        "cookie": text(header::COOKIE),
        "authorization": text(header::AUTHORIZATION),
        "referer": text(header::REFERER),
        "body": body,
    });
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::SET_COOKIE, "echoed=1; Path=/"),
        ],
        reply.to_string(),
    )
        .into_response()
}

async fn me(State(fixture): State<Arc<Fixture>>, headers: HeaderMap) -> Response {
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    {
        fixture
            .authorizations
            .lock()
            .expect("log")
            .push(value.to_owned());
    }
    (
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"id":"user-1"}"#,
    )
        .into_response()
}

fn serve() -> (u16, Arc<Fixture>) {
    let fixture = Arc::new(Fixture::default());
    let state = Arc::clone(&fixture);
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("binds");
            sender
                .send(listener.local_addr().expect("an address").port())
                .expect("reports the port");
            let app = Router::new()
                .route("/robots.txt", get(|| async { "User-agent: *\n" }))
                .route(
                    "/login",
                    get(|| async {
                        (
                            [(header::SET_COOKIE, "sid=fixture-session; HttpOnly; Path=/")],
                            html("<title>login</title><script>fetch('/api/me', { headers: { Authorization: 'Bearer tab-token' } });</script>"),
                        )
                    }),
                )
                .route("/api/me", get(me))
                .route("/echo", get(echo).post(echo))
                .route(
                    "/third-party",
                    get(|headers: HeaderMap| async move {
                        let host = headers
                            .get(header::HOST)
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or_default()
                            .replace("localhost", "127.0.0.1");
                        html(&format!(
                            "<title>third</title><iframe src=\"http://{host}/framed\"></iframe>"
                        ))
                    }),
                )
                .route(
                    "/framed",
                    get(|| async {
                        html("<script>fetch('/api/me', { headers: { Authorization: 'Bearer framed' } });</script>")
                    }),
                )
                .route(
                    "/big",
                    get(|| async {
                        ([(header::CONTENT_TYPE, "application/octet-stream")], big()).into_response()
                    }),
                )
                .route(
                    "/busy",
                    get(|| async {
                        (
                            StatusCode::SERVICE_UNAVAILABLE,
                            [(header::CONTENT_TYPE, "application/json")],
                            r#"{"msg":"busy"}"#,
                        )
                    }),
                )
                .route(
                    "/sse",
                    get(|State(fixture): State<Arc<Fixture>>| async move {
                        let events = stream::unfold(0, move |step| {
                            let fixture = Arc::clone(&fixture);
                            async move {
                                match step {
                                    0 => Some((Ok(Bytes::from_static(b"data: one\n\n")), 1)),
                                    1 => {
                                        fixture.second.notified().await;
                                        Some((Ok(Bytes::from_static(b"data: two\n\n")), 2))
                                    }
                                    _ => None,
                                }
                            }
                        });
                        chunked(events)
                    }),
                )
                .route(
                    "/forever",
                    get(|State(fixture): State<Arc<Fixture>>| async move {
                        fixture.forever_closed.store(false, Ordering::SeqCst);
                        let guard = Arc::new(Closed(Arc::clone(&fixture)));
                        let ticks = stream::unfold(guard, |guard| async move {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            Some((Ok(Bytes::from_static(b"data: tick\n\n")), guard))
                        });
                        chunked(ticks)
                    }),
                )
                .route(
                    "/broken",
                    get(|| async {
                        let parts = stream::iter(vec![
                            Ok(Bytes::from_static(b"data: partial\n\n")),
                            Err(std::io::Error::other("connection lost")),
                        ])
                        .then(|part| async move {
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            part
                        });
                        chunked(parts)
                    }),
                )
                .route(
                    "/hang",
                    get(|| async {
                        pending::<()>().await;
                        Ok::<_, Infallible>("never")
                    }),
                )
                .with_state(state);
            axum::serve(listener, app).await.expect("serves");
        });
    });
    (receiver.recv().expect("a port"), fixture)
}

struct Report {
    failures: Vec<String>,
}

impl Report {
    fn check(&mut self, name: &str, passed: bool, detail: impl std::fmt::Debug) {
        if passed {
            println!("ok      {name}");
        } else {
            println!("FAILED  {name}: {detail:?}");
            self.failures.push(name.to_owned());
        }
    }
}

async fn within<T>(future: impl Future<Output = T>) -> Option<T> {
    tokio::time::timeout(STEP, future).await.ok()
}

async fn eventually(mut condition: impl FnMut() -> bool) -> bool {
    within(async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .is_some()
}

async fn text(gateway: &AccountGateway, url: &str) -> Result<(u16, String), String> {
    let response =
        within(gateway.fetch(url, SessionRequest::new("GET"), &CancellationToken::new()))
            .await
            .ok_or("timed out")?
            .map_err(|error| format!("{error:?}"))?;
    let status = response.status;
    let body = within(response.text())
        .await
        .ok_or("timed out")?
        .map_err(|error| format!("{error:?}"))?;
    Ok((status, body))
}

async fn open_tab(workspace: &BrowserWorkspace, url: String) -> Result<(), String> {
    let workspace = workspace.clone();
    tauri::async_runtime::spawn_blocking(move || {
        workspace
            .command(BrowserCommand::New { url: Some(url) })
            .map(drop)
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn main_world(app: &AppHandle, label: &str) -> Result<String, String> {
    let webview = app.get_webview(label).ok_or("no gateway webview")?;
    let cdp: CdpClient = WebviewCdp::client(webview, Duration::from_secs(10));
    let result = cdp
        .evaluate(Evaluate::value(
            "[typeof globalThis.__zvsGatewaySend, typeof globalThis.__zvsGatewayState, typeof globalThis.__TAURI_INTERNALS__, typeof globalThis.ipc, typeof globalThis.chrome?.webview].join(',')",
        ))
        .await
        .map_err(|error| error.to_string())?;
    cdp.close();
    Ok(result
        .value
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default())
}

#[allow(clippy::too_many_lines)]
async fn scenario(app: AppHandle, profile: PathBuf, port: u16, fixture: Arc<Fixture>) -> Report {
    let mut report = Report {
        failures: Vec::new(),
    };
    let origin = format!("http://127.0.0.1:{port}");
    let tokens = BearerTokens::default();
    let workspace = BrowserWorkspace::new(
        app.clone(),
        profile.clone(),
        BrowserLifecycle::new(),
        tokens.clone(),
        Arc::new(zvs_host::platform::events::EventBus::default()),
    );
    let options = GatewayOptions {
        head_timeout: Duration::from_secs(3),
        idle_timeout: Duration::from_secs(5),
        chunk_size: 16 * 1024,
        ..GatewayOptions::default()
    };
    let gateway = AccountGateway::with_options(
        Arc::new(WebviewPages::new(app.clone(), profile.clone())),
        tokens.clone(),
        options,
        silent_logger(),
    );

    let before = text(&gateway, &format!("{origin}/echo")).await;
    let cookie_before = before
        .as_ref()
        .ok()
        .and_then(|(_, body)| serde_json::from_str::<Value>(body).ok())
        .map(|value| value["cookie"].clone());
    report.check(
        "a gateway request runs before any login, without the session cookie",
        before.as_ref().is_ok_and(|(status, _)| *status == 200)
            && cookie_before
                .as_ref()
                .is_some_and(|cookie| !cookie.to_string().contains("sid=")),
        &before,
    );
    report.check(
        "the user agent is read from the profile",
        gateway.user_agent().contains("Edg/"),
        gateway.user_agent(),
    );

    let world = main_world(&app, "account-gateway-0").await;
    report.check(
        "the gateway page's own world has no binding, state or Tauri bridge",
        world.as_deref() == Ok("undefined,undefined,undefined,undefined,undefined"),
        &world,
    );

    let opened = open_tab(&workspace, format!("{origin}/login")).await;
    let captured = eventually(|| tokens.token(&origin).is_some()).await;
    report.check(
        "a site tab's own bearer header is captured",
        opened.is_ok()
            && captured
            && tokens.token(&origin) == Some(AccountToken::bearer("tab-token")),
        (&opened, tokens.token(&origin).map(|token| token.token_type)),
    );
    report.check(
        "the gateway exposes the captured token",
        gateway.account_token(&format!("{origin}/api/v1/auths/"))
            == Some(AccountToken::bearer("tab-token")),
        "",
    );

    let after = text(&gateway, &format!("{origin}/echo")).await;
    report.check(
        "a cookie set by logging in from a tab is sent by a gateway request",
        after
            .as_ref()
            .is_ok_and(|(_, body)| body.contains("sid=fixture-session")),
        after.as_ref().map(|(status, _)| *status),
    );

    let posted = within(
        gateway.fetch(
            &format!("{origin}/echo"),
            SessionRequest::new("POST")
                .with_header("authorization", "Bearer host-token")
                .with_header("content-type", "application/json")
                .with_header("referer", format!("{origin}/"))
                .with_header("cookie", "forged=1")
                .with_body(r#"{"stream":true}"#),
            &CancellationToken::new(),
        ),
    )
    .await;
    let posted = match posted {
        Some(Ok(response)) => {
            let set_cookie = response.headers.contains_key("set-cookie");
            response
                .text()
                .await
                .map(|body| (set_cookie, body))
                .map_err(|error| format!("{error:?}"))
        }
        other => Err(format!(
            "{:?}",
            other.map(|result| result.map(|response| response.status))
        )),
    };
    let echoed = posted
        .as_ref()
        .ok()
        .and_then(|(_, body)| serde_json::from_str::<Value>(body).ok())
        .unwrap_or_default();
    report.check(
        "a POST carries its method, body, headers and referrer, never a forged cookie or a Set-Cookie",
        posted.as_ref().is_ok_and(|(set_cookie, _)| !set_cookie)
            && echoed["method"] == "POST"
            && echoed["body"] == r#"{"stream":true}"#
            && echoed["authorization"] == "Bearer host-token"
            && echoed["referer"] == format!("{origin}/")
            && !echoed["cookie"].to_string().contains("forged"),
        &echoed,
    );
    report.check(
        "a gateway request does not overwrite the captured token",
        tokens.token(&origin) == Some(AccountToken::bearer("tab-token")),
        "",
    );

    let third = open_tab(&workspace, format!("http://localhost:{port}/third-party")).await;
    let framed = eventually(|| {
        fixture
            .authorizations
            .lock()
            .expect("log")
            .iter()
            .any(|value| value == "Bearer framed")
    })
    .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    report.check(
        "a third-party frame's request does not set the token",
        third.is_ok() && framed && tokens.token(&origin) == Some(AccountToken::bearer("tab-token")),
        (&third, framed),
    );

    let large = within(async {
        let response = gateway
            .fetch(
                &format!("{origin}/big"),
                SessionRequest::new("GET"),
                &CancellationToken::new(),
            )
            .await?;
        response.bytes().await
    })
    .await;
    report.check(
        "a large body is reassembled byte for byte",
        matches!(&large, Some(Ok(bytes)) if *bytes == big()),
        large.as_ref().map(|result| result.as_ref().map(Vec::len)),
    );

    let busy = text(&gateway, &format!("{origin}/busy")).await;
    report.check(
        "a non-2xx status keeps its body",
        busy == Ok((503, r#"{"msg":"busy"}"#.to_owned())),
        &busy,
    );

    let sse = within(async {
        let response = gateway
            .fetch(
                &format!("{origin}/sse"),
                SessionRequest::new("GET"),
                &CancellationToken::new(),
            )
            .await
            .map_err(|error| format!("{error:?}"))?;
        let mut body = response.body;
        let first = body
            .next()
            .await
            .ok_or("no first event")?
            .map_err(|error| format!("{error:?}"))?;
        fixture.second.notify_one();
        let second = body
            .next()
            .await
            .ok_or("no second event")?
            .map_err(|error| format!("{error:?}"))?;
        let end = body.next().await.is_none();
        Ok::<_, String>((first, second, end))
    })
    .await;
    report.check(
        "an event stream arrives incrementally",
        matches!(&sse, Some(Ok((first, second, true))) if first == b"data: one\n\n" && second == b"data: two\n\n"),
        &sse,
    );

    let cancel = CancellationToken::new();
    let cancelled = within(async {
        let response = gateway
            .fetch(
                &format!("{origin}/forever"),
                SessionRequest::new("GET"),
                &cancel,
            )
            .await
            .map_err(|error| format!("{error:?}"))?;
        let mut body = response.body;
        body.next()
            .await
            .ok_or("no tick")?
            .map_err(|error| format!("{error:?}"))?;
        cancel.cancel();
        let error = body.next().await.ok_or("ended")?.expect_err("cancelled");
        Ok::<_, String>(error.code)
    })
    .await;
    let closed = eventually(|| fixture.forever_closed.load(Ordering::SeqCst)).await;
    report.check(
        "cancelling mid-stream ends the stream and closes the connection",
        matches!(cancelled, Some(Ok(AppErrorCode::RunCancelled))) && closed,
        (&cancelled, closed),
    );

    let broken = within(async {
        let response = gateway
            .fetch(
                &format!("{origin}/broken"),
                SessionRequest::new("GET"),
                &CancellationToken::new(),
            )
            .await
            .map_err(|error| format!("{error:?}"))?;
        Ok::<_, String>(match response.bytes().await {
            Ok(_) => "completed".to_owned(),
            Err(error) => format!("{:?}", error.code),
        })
    })
    .await;
    report.check(
        "a connection lost mid-body is a network failure",
        matches!(&broken, Some(Ok(code)) if code == "ProviderUnreachable"),
        &broken,
    );

    let hang = within(gateway.fetch(
        &format!("{origin}/hang"),
        SessionRequest::new("GET"),
        &CancellationToken::new(),
    ))
    .await;
    report.check(
        "a request with no response times out",
        matches!(&hang, Some(Err(error)) if error.code == AppErrorCode::ProviderUnreachable && error.details.as_ref().is_some_and(|details| details.contains_key("timeoutSeconds"))),
        hang.as_ref().map(|result| result.as_ref().map(|response| response.status)),
    );

    let reloaded = app
        .get_webview("account-gateway-0")
        .ok_or_else(|| "no gateway webview".to_owned())
        .and_then(|webview| webview.reload().map_err(|error| error.to_string()));
    tokio::time::sleep(Duration::from_millis(500)).await;
    let recreated = text(&gateway, &format!("{origin}/echo")).await;
    let replaced = eventually(|| app.get_webview("account-gateway-0").is_none()).await;
    report.check(
        "a gateway page that navigated is replaced by a new one",
        reloaded.is_ok()
            && recreated.as_ref().is_ok_and(|(status, _)| *status == 200)
            && replaced
            && app.get_webview("account-gateway-1").is_some(),
        (
            &reloaded,
            recreated.as_ref().map(|(status, _)| *status),
            replaced,
        ),
    );

    let refused = gateway
        .fetch(
            "https://example.com/",
            SessionRequest::new("GET"),
            &CancellationToken::new(),
        )
        .await;
    report.check(
        "an origin outside the allow-list is refused",
        refused.is_err(),
        refused.map(|response| response.status),
    );

    if std::env::var_os("ZVS_LIVE_VENDOR").is_some() {
        for endpoint in [
            "https://chat.qwen.ai/api/v1/auths/",
            "https://chat.z.ai/api/v1/auths/",
        ] {
            let live = within(gateway.fetch(
                endpoint,
                SessionRequest::new("GET").with_header("accept", "application/json"),
                &CancellationToken::new(),
            ))
            .await;
            match live {
                Some(Ok(response)) => println!(
                    "live    {endpoint} answered {} ({:?})",
                    response.status,
                    response.headers.get("content-type")
                ),
                other => println!(
                    "live    {endpoint} failed: {:?}",
                    other.map(|result| result.err())
                ),
            }
        }
    }

    gateway.dispose();
    report
}

fn main() {
    let (port, fixture) = serve();
    let directory = temporary_directory("gateway-e2e-");
    let profile = directory.join("browser-work");
    tauri::Builder::default()
        .setup(move |app| {
            WindowBuilder::new(app, "studio")
                .visible(false)
                .inner_size(1024.0, 768.0)
                .build()?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let report = scenario(handle.clone(), profile, port, fixture).await;
                let failed = report.failures.len();
                println!("{} checks failed", failed);
                handle.exit(i32::from(failed > 0));
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("the e2e app builds")
        .run(move |_, event| {
            if let tauri::RunEvent::Exit = event {
                let _ = &directory;
            }
        });
}
