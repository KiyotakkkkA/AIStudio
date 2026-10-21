use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::json;

use super::*;

fn json_response(status: StatusCode, body: &Value) -> Response {
    (
        status,
        [("content-type", "application/json")],
        body.to_string(),
    )
        .into_response()
}

#[test]
fn challenge_parameters_are_read_quoted_and_bare() {
    let header = r#"Bearer realm="mcp", resource_metadata="http://testix.zvsd.ru/.well-known/oauth-protected-resource/mcp", scope=openid"#;
    assert_eq!(
        challenge_param(header, "resource_metadata").as_deref(),
        Some("http://testix.zvsd.ru/.well-known/oauth-protected-resource/mcp")
    );
    assert_eq!(challenge_param(header, "realm").as_deref(), Some("mcp"));
    assert_eq!(challenge_param(header, "scope").as_deref(), Some("openid"));
    assert_eq!(challenge_param(header, "error"), None);
}

#[test]
fn the_pkce_challenge_is_the_unpadded_base64url_sha256_of_the_verifier() {
    let pkce = Pkce::from_verifier("dBjftJeZ4CVP-mJ92IZJbYT6WQoC7LVQo_Xm4nJx0Ss".to_owned());
    assert_eq!(
        pkce.challenge,
        "eKucCl6sQpBdVoeEkq69Ew5tG-dqGrEx7NIAGq0B5PU"
    );
    let generated = Pkce::generate();
    assert!((43..=128).contains(&generated.verifier.len()));
    assert_ne!(generated.verifier, Pkce::generate().verifier);
}

#[test]
fn an_insecure_metadata_link_on_a_secure_host_is_tried_over_https_first() {
    let base = Url::parse("https://testix.zvsd.ru/mcp").expect("a url");
    assert_eq!(
        upgraded(
            "http://testix.zvsd.ru/.well-known/oauth-protected-resource/mcp",
            &base
        ),
        [
            "https://testix.zvsd.ru/.well-known/oauth-protected-resource/mcp",
            "http://testix.zvsd.ru/.well-known/oauth-protected-resource/mcp",
        ]
    );
    assert_eq!(
        well_known(&base, "oauth-protected-resource"),
        [
            "https://testix.zvsd.ru/.well-known/oauth-protected-resource/mcp",
            "https://testix.zvsd.ru/.well-known/oauth-protected-resource",
        ]
    );
}

#[test]
fn the_callback_yields_the_code_only_for_the_matching_state() {
    assert_eq!(
        parse_callback("/callback?code=abc&state=s1", "s1")
            .expect("a callback")
            .expect("a code"),
        "abc"
    );
    assert!(parse_callback("/callback?code=abc&state=other", "s1").is_none());
    assert!(parse_callback("/favicon.ico", "s1").is_none());
    let denied = parse_callback("/callback?error=access_denied&state=s1", "s1")
        .expect("a callback")
        .expect_err("denied");
    assert!(denied.message.contains("access_denied"));
}

#[test]
fn an_auth_required_error_carries_its_challenge() {
    let error = auth_required(Some("Bearer realm=\"mcp\""));
    assert!(is_auth_required(&error));
    assert_eq!(
        challenge_of(&error).as_deref(),
        Some("Bearer realm=\"mcp\"")
    );
    assert!(!is_auth_required(&AppError::new(
        AppErrorCode::McpUnavailable,
        "x"
    )));
}

#[test]
fn a_grant_expiring_within_a_minute_needs_refreshing() {
    let grant = OAuthGrant {
        issuer: "https://id".to_owned(),
        token_endpoint: "https://id/token".to_owned(),
        client_id: "c".to_owned(),
        client_secret: None,
        redirect_uri: "http://127.0.0.1:53682/callback".to_owned(),
        resource: "https://r/mcp".to_owned(),
        scope: None,
        access_token: "a".to_owned(),
        refresh_token: None,
        expires_at: Some(100_000),
    };
    assert!(!grant.expires_soon(30_000));
    assert!(grant.expires_soon(50_000));
    assert_eq!(grant.redirect_port(), Some(53682));
}

type Form = Vec<(String, String)>;

#[derive(Clone, Default)]
struct Fixture {
    base: Arc<Mutex<String>>,
    tokens: Arc<Mutex<Vec<Form>>>,
}

async fn resource_metadata(State(fixture): State<Fixture>) -> Response {
    let base = fixture.base.lock().expect("base").clone();
    json_response(
        StatusCode::OK,
        &json!({
            "resource": format!("{base}/mcp"),
            "authorization_servers": [format!("{base}/issuer")],
            "scopes_supported": ["openid", "profile"],
        }),
    )
}

async fn server_metadata(State(fixture): State<Fixture>) -> Response {
    let base = fixture.base.lock().expect("base").clone();
    json_response(
        StatusCode::OK,
        &json!({
            "issuer": format!("{base}/issuer"),
            "authorization_endpoint": format!("{base}/authorize"),
            "token_endpoint": format!("{base}/token"),
            "registration_endpoint": format!("{base}/register"),
            "scopes_supported": ["openid", "profile", "offline_access"],
            "code_challenge_methods_supported": ["S256"],
        }),
    )
}

async fn token(State(fixture): State<Fixture>, body: Bytes) -> Response {
    let form: Form = url::form_urlencoded::parse(&body).into_owned().collect();
    let grant_type = form
        .iter()
        .find(|(key, _)| key == "grant_type")
        .map(|(_, value)| value.clone())
        .unwrap_or_default();
    fixture.tokens.lock().expect("tokens").push(form);
    if grant_type == "refresh_token" {
        return json_response(
            StatusCode::OK,
            &json!({ "access_token": "access-2", "expires_in": 3600 }),
        );
    }
    json_response(
        StatusCode::OK,
        &json!({
            "access_token": "access-1",
            "refresh_token": "refresh-1",
            "expires_in": 60,
            "token_type": "Bearer",
        }),
    )
}

async fn serve() -> (String, Fixture) {
    let fixture = Fixture::default();
    let app = Router::new()
        .route(
            "/mcp",
            post(|| async {
                (
                    StatusCode::UNAUTHORIZED,
                    [(
                        "www-authenticate",
                        "Bearer realm=\"mcp\", resource_metadata=\"/missing\"",
                    )],
                )
            }),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            get(resource_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server/issuer",
            get(server_metadata),
        )
        .route(
            "/register",
            post(|| async {
                json_response(StatusCode::CREATED, &json!({ "client_id": "client-1" }))
            }),
        )
        .route("/token", post(token))
        .with_state(fixture.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
    let base = format!("http://{}", listener.local_addr().expect("an address"));
    *fixture.base.lock().expect("base") = base.clone();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, fixture)
}

#[tokio::test]
async fn discovery_follows_the_resource_to_its_authorization_server() {
    let (base, _) = serve().await;
    let http = reqwest::Client::new();
    let challenge = probe_challenge(&http, &format!("{base}/mcp"))
        .await
        .expect("a challenge");
    let discovery = discover(&http, &format!("{base}/mcp"), Some(&challenge))
        .await
        .expect("discovers");
    assert_eq!(discovery.resource, format!("{base}/mcp"));
    assert_eq!(discovery.issuer, format!("{base}/issuer"));
    assert_eq!(
        discovery.scope.as_deref(),
        Some("openid profile offline_access")
    );
    let pkce = Pkce::generate();
    let address = authorize_url(
        &discovery,
        "client-1",
        "http://127.0.0.1:1/callback",
        "s1",
        &pkce,
    )
    .expect("a url");
    let parsed = Url::parse(&address).expect("parses");
    let query: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
    assert_eq!(query["code_challenge"], pkce.challenge);
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(query["resource"], format!("{base}/mcp"));
    assert_eq!(query["state"], "s1");
}

#[tokio::test]
async fn the_loopback_callback_code_is_exchanged_and_refreshed() {
    let (base, fixture) = serve().await;
    let http = reqwest::Client::new();
    let discovery = discover(&http, &format!("{base}/mcp"), None)
        .await
        .expect("discovers");
    let loopback = Loopback::bind(None).await.expect("binds");
    let redirect_uri = loopback.redirect_uri.clone();
    let client = register(
        &http,
        discovery
            .server
            .registration_endpoint
            .as_deref()
            .expect("an endpoint"),
        &redirect_uri,
        "ZVS AI Studio",
        None,
    )
    .await
    .expect("registers");
    let browser = reqwest::Client::new();
    let target = redirect_uri.clone();
    tokio::spawn(async move {
        let _ = browser
            .get(format!("{target}?code=wrong&state=other"))
            .send()
            .await;
        let _ = browser
            .get(format!("{target}?code=code-1&state=s1"))
            .send()
            .await;
    });
    let code = loopback
        .wait_for_code("s1", Duration::from_secs(5), &CancellationToken::new())
        .await
        .expect("a code");
    assert_eq!(code, "code-1");
    let pkce = Pkce::generate();
    let grant = exchange_code(
        &http,
        CodeExchange {
            discovery: &discovery,
            client: &client,
            redirect_uri: &redirect_uri,
            code: &code,
            verifier: &pkce.verifier,
            now: 1_000,
        },
    )
    .await
    .expect("exchanges");
    assert_eq!(grant.client_id, "client-1");
    assert_eq!(grant.access_token, "access-1");
    assert_eq!(grant.expires_at, Some(61_000));
    let refreshed = refresh(&http, &grant, 2_000).await.expect("refreshes");
    assert_eq!(refreshed.access_token, "access-2");
    assert_eq!(refreshed.refresh_token.as_deref(), Some("refresh-1"));
    let forms = fixture.tokens.lock().expect("tokens").clone();
    assert!(forms[0].contains(&("code_verifier".to_owned(), pkce.verifier.clone())));
    assert!(forms[1].contains(&("refresh_token".to_owned(), "refresh-1".to_owned())));
}

#[tokio::test]
async fn waiting_for_the_callback_stops_when_cancelled() {
    let loopback = Loopback::bind(None).await.expect("binds");
    let cancel = CancellationToken::new();
    cancel.cancel();
    let error = loopback
        .wait_for_code("s1", Duration::from_secs(5), &cancel)
        .await
        .expect_err("cancelled");
    assert_eq!(error.message, SIGN_IN_CANCELLED);
}
