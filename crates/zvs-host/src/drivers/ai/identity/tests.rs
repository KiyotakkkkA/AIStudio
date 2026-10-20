use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{AccountFamily, AppErrorCode, SecretScope};

use super::*;
use crate::data::repositories::{AccountDraft, AccountPatch, SecretFilter};
use crate::data::schema::AccountRow;
use crate::data::{Database, Repositories};
use crate::drivers::ai::errors::{Headers, TIMED_OUT_MESSAGE, UNREACHABLE_MESSAGE};
use crate::drivers::ai::transport::{
    AccountCredentials, AccountTransport, AccountTransportOptions, RequestSpec,
    SharedSessionGateway, Transport,
};
use crate::platform::clock::FakeClock;
use crate::platform::logger::silent_logger;
use crate::services::crypto::{CryptoService, Plaintext};
use crate::services::secrets::{SecretCreateInput, SecretService};
use crate::testing::{
    FakeCrypto, FakeReply, FakeSessionGateway, MemoryLogger, TemporaryDatabase, temporary_database,
};

const PARTITION: &str = "persist:browser-work";
const QWEN_CHAT: &str = "https://chat.qwen.ai/api/v2/chat/completions";

fn qwen_payload() -> Value {
    json!({
        "id": "qwen-9",
        "email": "k***@mail.ru",
        "name": "Кирилл",
        "profile_image_url": "data:image/png;base64,AAAA",
        "token": "qwen-token-1",
        "token_type": "Bearer",
        "expires_at": 4_102_444_800_i64,
    })
}

struct Fixture {
    database: TemporaryDatabase,
    secrets: Arc<SecretService>,
    session: Arc<FakeSessionGateway>,
}

impl Fixture {
    fn new() -> Self {
        let database = temporary_database();
        let secrets = Arc::new(SecretService::new(
            database.shared(),
            Arc::new(CryptoService::new(FakeCrypto::shared())),
            Arc::new(FakeClock::default()),
            silent_logger(),
        ));
        Self {
            database,
            secrets,
            session: FakeSessionGateway::shared(),
        }
    }

    fn shared_session(&self) -> SharedSessionGateway {
        self.session.clone()
    }

    fn account(&self, family: AccountFamily, external_id: &str) -> AccountRow {
        self.database
            .transaction(|transaction| {
                Ok(Repositories::new(transaction)
                    .accounts
                    .create(&AccountDraft::new(family, PARTITION, external_id, 100))?)
            })
            .expect("an account")
    }

    fn update(&self, id: &str, patch: &AccountPatch) {
        self.database
            .transaction(|transaction| {
                Ok(Repositories::new(transaction)
                    .accounts
                    .update(id, patch, 200)?)
            })
            .expect("updates");
    }

    fn find(&self, id: &str) -> Option<AccountRow> {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).accounts.find_by_id(id)?)
            })
            .expect("reads")
    }

    fn stale_secret(&self) -> String {
        let mut input = SecretCreateInput::new(
            ACCOUNT_TOKEN_SECRET_TYPE,
            "qwen-web:qwen-9",
            SecretScope::Personal,
        );
        input.value = Some(Plaintext::new("qwen-token-stale"));
        self.secrets.create(input).expect("a secret").id
    }

    fn store(
        &self,
        account_id: &str,
        now: i64,
        logger: Option<Arc<MemoryLogger>>,
    ) -> AccountCredentialStore {
        let mut options = store::AccountCredentialStoreOptions::new(
            account_id,
            self.database.shared(),
            Arc::clone(&self.secrets),
            self.shared_session(),
            Arc::new(FakeClock::new(now)),
        );
        if let Some(logger) = logger {
            options.logger = logger;
        }
        AccountCredentialStore::new(options)
    }

    fn transport(
        &self,
        base_url: &str,
        credentials: Option<Arc<AccountCredentialStore>>,
    ) -> AccountTransport {
        let mut options = AccountTransportOptions::new(base_url, self.shared_session());
        options.credentials = credentials.map(|store| store as _);
        AccountTransport::new(options)
    }
}

fn html(status: u16) -> FakeReply {
    FakeReply::text(
        status,
        "text/html; charset=utf-8",
        "<!doctype html><title>Sign in</title>",
    )
}

fn token() -> CancellationToken {
    CancellationToken::new()
}

fn raw_row(database: &Database, id: &str) -> String {
    database
        .with_connection(|connection| {
            Ok(connection.query_row(
                &format!("SELECT {} FROM account WHERE id = ?1", AccountRow::COLUMNS),
                [id],
                |row| {
                    let mut values = Vec::new();
                    for index in 0..14 {
                        values.push(format!(
                            "{:?}",
                            row.get::<_, rusqlite::types::Value>(index)?
                        ));
                    }
                    Ok(values.join("|"))
                },
            )?)
        })
        .expect("a row")
}

#[test]
fn the_vendor_payload_collapses_onto_one_account_identity() {
    let qwen = map_qwen_identity(&qwen_payload()).expect("maps");
    assert_eq!(
        qwen.identity,
        AccountIdentity {
            external_id: "qwen-9".to_owned(),
            email_masked: Some("k***@mail.ru".to_owned()),
            display_name: Some("Кирилл".to_owned()),
            avatar_url: Some("data:image/png;base64,AAAA".to_owned()),
            expires_at: Some(4_102_444_800),
        }
    );
    assert_eq!(qwen.credential, Some(AccountToken::bearer("qwen-token-1")));

    let sparse = map_qwen_identity(&json!({ "id": 7 })).expect("maps");
    assert_eq!(
        sparse,
        ProbeResult {
            identity: AccountIdentity::new("7"),
            credential: None,
        }
    );
    let typed = map_qwen_identity(
        &json!({ "id": "1", "token": " t ", "token_type": "JWT", "email": "  " }),
    )
    .expect("maps");
    assert_eq!(typed.identity.email_masked, None);
    assert_eq!(
        typed.credential,
        Some(AccountToken {
            token: "t".to_owned(),
            token_type: "JWT".to_owned()
        })
    );
    for malformed in [
        json!({ "email": "nobody" }),
        json!({ "id": "  " }),
        json!({ "id": "1", "expires_at": 0 }),
        json!({ "id": "1", "expires_at": 1.5 }),
        json!({ "id": "1", "email": 5 }),
        json!(["qwen-9"]),
    ] {
        let error = map_qwen_identity(&malformed).expect_err("refuses");
        assert_eq!(error.code, AppErrorCode::ValidationFailed, "{malformed}");
        assert_eq!(error.message, MALFORMED_IDENTITY_MESSAGE);
    }
    assert_eq!(
        identity_probe(AccountFamily::QwenWeb).endpoint(),
        QWEN_IDENTITY_ENDPOINT
    );
    assert_eq!(
        AccountFamily::ALL
            .iter()
            .map(|family| identity_probe(*family).family())
            .collect::<Vec<_>>(),
        AccountFamily::ALL
    );
    assert_eq!(
        identity_probe(AccountFamily::QwenWeb).login_url(),
        QWEN_LOGIN_URL
    );
}

#[tokio::test]
async fn a_login_redirect_to_html_is_a_session_expiry_not_a_parse_error() {
    let session = FakeSessionGateway::shared();
    session.queue(QWEN_IDENTITY_ENDPOINT, vec![html(200)]);
    let error = identity_probe(AccountFamily::QwenWeb)
        .probe(session.as_ref(), &token())
        .await
        .expect_err("expired");
    assert_eq!(error.code, AppErrorCode::ProviderSessionExpired);
    assert_eq!(
        error
            .details
            .as_ref()
            .and_then(|details| details.get("partition")),
        Some(&json!(crate::browser::BROWSER_PARTITION))
    );

    session.reset();
    session.queue(
        QWEN_IDENTITY_ENDPOINT,
        vec![FakeReply::json(401, &json!({ "code": 40_100 }))],
    );
    assert_eq!(
        identity_probe(AccountFamily::QwenWeb)
            .probe(session.as_ref(), &token())
            .await
            .expect_err("expired")
            .code,
        AppErrorCode::ProviderSessionExpired
    );

    session.reset();
    session.queue(
        QWEN_IDENTITY_ENDPOINT,
        vec![FakeReply::Failure(zvs_contract::AppError::new(
            AppErrorCode::ProviderUnreachable,
            UNREACHABLE_MESSAGE,
        ))],
    );
    let unreachable = identity_probe(AccountFamily::QwenWeb)
        .probe(session.as_ref(), &token())
        .await
        .expect_err("unreachable");
    assert!(matches!(
        unreachable.code,
        AppErrorCode::ProviderUnreachable | AppErrorCode::Unknown
    ));

    session.reset();
    session.queue(
        QWEN_IDENTITY_ENDPOINT,
        vec![FakeReply::text(200, "application/json", "not json")],
    );
    assert_eq!(
        identity_probe(AccountFamily::QwenWeb)
            .probe(session.as_ref(), &token())
            .await
            .expect_err("malformed")
            .code,
        AppErrorCode::ValidationFailed
    );
}

#[tokio::test]
async fn the_probe_sends_the_captured_bearer_and_keeps_it_when_the_vendor_returns_none() {
    let session = FakeSessionGateway::shared();
    session.observe(
        QWEN_IDENTITY_ENDPOINT,
        AccountToken::bearer("browser-token"),
    );
    session.queue(
        QWEN_IDENTITY_ENDPOINT,
        vec![FakeReply::json(200, &json!({ "id": "qwen-9" }))],
    );
    let result = identity_probe(AccountFamily::QwenWeb)
        .probe(session.as_ref(), &token())
        .await
        .expect("probes");
    assert_eq!(
        result.credential,
        Some(AccountToken::bearer("browser-token"))
    );
    let sent = session.last_request().expect("a request").request;
    assert_eq!(sent.method, "GET");
    assert_eq!(sent.header("authorization"), Some("Bearer browser-token"));
    assert_eq!(sent.header("accept"), Some("application/json"));
    assert_eq!(sent.header("origin"), Some("https://chat.qwen.ai"));
    assert_eq!(sent.header("referer"), Some("https://chat.qwen.ai/"));
    assert_eq!(sent.header("cookie"), None);
}

#[tokio::test(start_paused = true)]
async fn an_identity_probe_gives_up_after_twenty_seconds_and_honours_cancellation() {
    let session = FakeSessionGateway::shared();
    session.queue(QWEN_IDENTITY_ENDPOINT, vec![FakeReply::Stall]);
    let error = identity_probe(AccountFamily::QwenWeb)
        .probe(session.as_ref(), &token())
        .await
        .expect_err("times out");
    assert_eq!(error.message, TIMED_OUT_MESSAGE);
    assert!(session.scopes()[0].is_cancelled());

    let cancel = token();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        trigger.cancel();
    });
    let error = identity_probe(AccountFamily::QwenWeb)
        .probe(session.as_ref(), &cancel)
        .await
        .expect_err("cancelled");
    assert_eq!(error.code, AppErrorCode::RunCancelled);
}

#[tokio::test]
async fn a_captured_token_reaches_secret_service_and_nothing_else() {
    let fixture = Fixture::new();
    let logger = MemoryLogger::shared();
    fixture.session.queue(
        QWEN_IDENTITY_ENDPOINT,
        vec![FakeReply::json(200, &qwen_payload())],
    );
    let account = fixture.account(AccountFamily::QwenWeb, "qwen-9");
    let store = fixture.store(&account.id, 1_000_000, Some(logger.clone()));

    let identity = store.probe(&token()).await.expect("probes");
    assert_eq!(
        identity,
        AccountIdentity {
            external_id: "qwen-9".to_owned(),
            email_masked: Some("k***@mail.ru".to_owned()),
            display_name: Some("Кирилл".to_owned()),
            avatar_url: Some("data:image/png;base64,AAAA".to_owned()),
            expires_at: Some(4_102_444_800),
        }
    );
    let stored = fixture.find(&account.id).expect("stored");
    let secret_id = stored.token_secret_id.clone().expect("a token secret");
    assert_eq!(stored.token_expires_at, Some(4_102_444_800));
    assert_eq!(stored.status, "linked");
    assert_eq!(stored.last_checked_at, Some(1_000_000));
    assert!(!format!("{stored:?}").contains("qwen-token-1"));
    assert!(!raw_row(&fixture.database, &account.id).contains("qwen-token-1"));
    assert!(!format!("{:?}", logger.entries()).contains("qwen-token-1"));
    assert_eq!(
        fixture
            .secrets
            .resolve(&secret_id)
            .expect("resolves")
            .expose(),
        "qwen-token-1"
    );
    let usage = fixture.secrets.usage(&secret_id).expect("usage");
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].id, account.id);
    assert_eq!(usage[0].kind.as_str(), "account");
    assert_eq!(
        fixture.secrets.get(&secret_id).expect("a secret").name,
        "qwen-web:qwen-9"
    );

    store.unlink().expect("unlinks");
    assert_eq!(fixture.find(&account.id), None);
    assert!(
        fixture
            .secrets
            .list(&SecretFilter::default())
            .expect("lists")
            .iter()
            .all(|entry| entry.id != secret_id)
    );
    assert_eq!(
        logger.matching("Unlinked an account")[0].fields["deletedToken"],
        json!(true)
    );
}

#[tokio::test]
async fn an_expired_token_re_probes_identity_exactly_once_and_then_retries() {
    let fixture = Fixture::new();
    let mut fresh = qwen_payload();
    fresh["token"] = json!("qwen-token-2");
    fixture
        .session
        .queue(QWEN_IDENTITY_ENDPOINT, vec![FakeReply::json(200, &fresh)]);
    let account = fixture.account(AccountFamily::QwenWeb, "qwen-9");
    let stale = fixture.stale_secret();
    fixture.update(
        &account.id,
        &AccountPatch {
            token_secret_id: Some(Some(stale.clone())),
            token_expires_at: Some(Some(1_000)),
            ..AccountPatch::default()
        },
    );
    let store = Arc::new(fixture.store(&account.id, 2_000_000, None));

    let current = store.current(&token()).await.expect("a token");
    assert_eq!(current, Some(AccountToken::bearer("qwen-token-2")));
    assert_eq!(fixture.session.requests().len(), 1);
    assert_eq!(
        fixture.secrets.resolve(&stale).expect("resolves").expose(),
        "qwen-token-2"
    );
    assert_eq!(
        fixture.find(&account.id).expect("kept").token_secret_id,
        Some(stale.clone())
    );

    fixture.session.queue(
        QWEN_CHAT,
        vec![
            FakeReply::json(401, &json!({ "detail": "expired" })),
            FakeReply::json(200, &json!({ "ok": true })),
        ],
    );
    let transport = fixture.transport("https://chat.qwen.ai/api/v2", Some(store));
    let response = transport
        .request(
            &RequestSpec::post("chat/completions", json!({ "model": "qwen3" })),
            &token(),
        )
        .await
        .expect("retries");
    assert_eq!(response.status, 200);
    assert_eq!(
        response.json::<Value>().expect("json"),
        json!({ "ok": true })
    );
    let chat = fixture.session.requests_to(QWEN_CHAT);
    assert_eq!(chat.len(), 2);
    assert_eq!(fixture.session.requests_to(QWEN_IDENTITY_ENDPOINT).len(), 2);
    for call in &chat {
        assert_eq!(
            call.request.header("authorization"),
            Some("Bearer qwen-token-2")
        );
    }
    let first = &chat[0].request;
    assert_eq!(
        first.header("user-agent"),
        Some(crate::testing::fake_session::FAKE_USER_AGENT)
    );
    assert_eq!(first.header("origin"), Some("https://chat.qwen.ai"));
    assert_eq!(first.header("referer"), Some("https://chat.qwen.ai/"));
    assert_eq!(first.header("content-type"), Some("application/json"));
    assert_eq!(first.body.as_deref(), Some(r#"{"model":"qwen3"}"#));
    assert_eq!(first.header("cookie"), None);
}

#[tokio::test]
async fn a_session_that_stays_dead_ends_as_session_expired_after_one_re_probe() {
    let fixture = Fixture::new();
    let account = fixture.account(AccountFamily::QwenWeb, "qwen-9");
    let stale = fixture.stale_secret();
    fixture.update(
        &account.id,
        &AccountPatch {
            token_secret_id: Some(Some(stale)),
            ..AccountPatch::default()
        },
    );
    let store = Arc::new(fixture.store(&account.id, 2_000_000, None));
    fixture.session.queue(
        QWEN_CHAT,
        vec![FakeReply::json(401, &json!({ "detail": "expired" }))],
    );
    fixture
        .session
        .queue(QWEN_IDENTITY_ENDPOINT, vec![html(200)]);
    let transport = fixture.transport("https://chat.qwen.ai/api/v2", Some(store));
    let error = transport
        .request(&RequestSpec::get("chat/completions"), &token())
        .await
        .expect_err("expired");
    assert_eq!(error.code, AppErrorCode::ProviderSessionExpired);
    assert_eq!(fixture.session.requests_to(QWEN_IDENTITY_ENDPOINT).len(), 1);
    assert_eq!(fixture.session.requests_to(QWEN_CHAT).len(), 1);
    let row = fixture.find(&account.id).expect("kept");
    assert_eq!(row.status, "needs-relink");
    assert_eq!(row.status_detail.as_deref(), Some(error.message.as_str()));
}

#[tokio::test]
async fn a_store_refuses_an_unlinked_account_and_a_different_browser_identity() {
    let fixture = Fixture::new();
    let account = fixture.account(AccountFamily::QwenWeb, "qwen-9");
    let store = fixture.store(&account.id, 2_000_000, None);
    assert_eq!(store.current(&token()).await.expect("no token yet"), None);

    let mut other = qwen_payload();
    other["id"] = json!("someone-else");
    fixture
        .session
        .queue(QWEN_IDENTITY_ENDPOINT, vec![FakeReply::json(200, &other)]);
    let error = store.refresh(&token()).await.expect_err("mismatch");
    assert_eq!(error.code, AppErrorCode::ProviderSessionExpired);
    assert_eq!(
        error
            .details
            .as_ref()
            .and_then(|details| details.get("reason")),
        Some(&json!("identity-mismatch"))
    );
    let row = fixture.find(&account.id).expect("kept");
    assert_eq!(
        row.status_detail.as_deref(),
        Some(store::IDENTITY_MISMATCH_DETAIL)
    );
    assert_eq!(
        store.current(&token()).await.expect_err("not linked").code,
        AppErrorCode::ProviderSessionExpired
    );
    let missing = fixture.store("missing", 0, None);
    assert_eq!(
        missing.current(&token()).await.expect_err("missing").code,
        AppErrorCode::NotFound
    );
}

#[tokio::test]
async fn a_rejected_bearer_has_the_client_renew_its_session_before_the_request_is_retried() {
    let fixture = Fixture::new();
    let account = fixture.account(AccountFamily::QwenWeb, "qwen-9");
    let stale = fixture.stale_secret();
    fixture.update(
        &account.id,
        &AccountPatch {
            token_secret_id: Some(Some(stale)),
            ..AccountPatch::default()
        },
    );
    let account = fixture.find(&account.id).expect("an account");
    fixture.session.renewable(true);
    fixture.session.queue(
        QWEN_IDENTITY_ENDPOINT,
        vec![FakeReply::json(200, &json!({ "id": "qwen-9" }))],
    );
    fixture.session.queue(
        QWEN_CHAT,
        vec![
            FakeReply::json(401, &json!({ "detail": "expired" })),
            FakeReply::json(200, &json!({ "ok": true })),
        ],
    );
    let credentials = ProbeAccountCredentials::new(
        account.clone(),
        fixture.shared_session(),
        fixture.secrets.clone(),
    )
    .expect("credentials");
    let mut options =
        AccountTransportOptions::new("https://chat.qwen.ai/api/v2", fixture.shared_session());
    options.credentials = Some(Arc::new(credentials));
    let response = AccountTransport::new(options)
        .request(&RequestSpec::get("chat/completions"), &token())
        .await
        .expect("retries");
    assert_eq!(response.status, 200);
    assert_eq!(
        fixture.session.renewals(),
        vec![QWEN_IDENTITY_ENDPOINT.to_owned()]
    );
    assert_eq!(fixture.session.requests_to(QWEN_CHAT).len(), 2);

    fixture.session.queue(
        QWEN_IDENTITY_ENDPOINT,
        vec![
            FakeReply::json(401, &json!({ "detail": "expired" })),
            FakeReply::json(200, &json!({ "id": "qwen-9" })),
        ],
    );
    let renewed = ProbeAccountCredentials::new(
        account.clone(),
        fixture.shared_session(),
        fixture.secrets.clone(),
    )
    .expect("credentials")
    .current(&token())
    .await
    .expect("a token");
    assert_eq!(renewed, Some(AccountToken::bearer("qwen-token-stale")));
    assert_eq!(fixture.session.renewals().len(), 2);
    assert_eq!(fixture.find(&account.id), Some(account));
}

#[tokio::test]
async fn probe_credentials_read_the_stored_token_after_a_restart_without_writing_anything() {
    let fixture = Fixture::new();
    let account = fixture.account(AccountFamily::QwenWeb, "qwen-9");
    let stale = fixture.stale_secret();
    fixture.update(
        &account.id,
        &AccountPatch {
            token_secret_id: Some(Some(stale)),
            ..AccountPatch::default()
        },
    );
    let account = fixture.find(&account.id).expect("an account");
    fixture.session.queue(
        QWEN_IDENTITY_ENDPOINT,
        vec![FakeReply::json(200, &json!({ "id": "qwen-9" }))],
    );
    let credentials = ProbeAccountCredentials::new(
        account.clone(),
        fixture.shared_session(),
        fixture.secrets.clone(),
    )
    .expect("credentials");
    assert_eq!(
        credentials.current(&token()).await.expect("a token"),
        Some(AccountToken::bearer("qwen-token-stale"))
    );
    assert_eq!(
        fixture
            .session
            .last_request()
            .expect("a probe")
            .request
            .header("authorization"),
        Some("Bearer qwen-token-stale")
    );

    fixture.session.observe(
        QWEN_IDENTITY_ENDPOINT,
        AccountToken::bearer("browser-token"),
    );
    assert_eq!(
        credentials.refresh(&token()).await.expect("a token"),
        Some(AccountToken::bearer("browser-token"))
    );
    assert_eq!(fixture.find(&account.id), Some(account.clone()));

    fixture.session.queue(
        QWEN_IDENTITY_ENDPOINT,
        vec![FakeReply::json(200, &json!({ "id": "different" }))],
    );
    assert_eq!(
        credentials
            .current(&token())
            .await
            .expect_err("mismatch")
            .code,
        AppErrorCode::ProviderSessionExpired
    );
    let cancel = token();
    cancel.cancel();
    assert_eq!(
        credentials
            .current(&cancel)
            .await
            .expect_err("cancelled")
            .code,
        AppErrorCode::RunCancelled
    );
    assert_eq!(fixture.find(&account.id), Some(account));
}

#[test]
fn the_stored_token_only_answers_for_the_identity_origin() {
    let fixture = Fixture::new();
    let account = fixture.account(AccountFamily::QwenWeb, "qwen-9");
    let stale = fixture.stale_secret();
    fixture.update(
        &account.id,
        &AccountPatch {
            token_secret_id: Some(Some(stale)),
            ..AccountPatch::default()
        },
    );
    let account = fixture.find(&account.id).expect("an account");
    let session = account_session(&account, fixture.shared_session(), fixture.secrets.as_ref())
        .expect("a session");
    assert_eq!(
        session.account_token("https://chat.qwen.ai/api/v2/models/"),
        Some(AccountToken::bearer("qwen-token-stale"))
    );
    assert_eq!(session.account_token("https://chat.qwen.ai.example/"), None);
    assert!(!format!("{session:?}").contains("qwen-token-stale"));
    fixture
        .session
        .observe("https://chat.qwen.ai/", AccountToken::bearer("newer"));
    assert_eq!(
        session.account_token(QWEN_IDENTITY_ENDPOINT),
        Some(AccountToken::bearer("newer"))
    );
    let unknown = AccountRow {
        adapter: "made-up".to_owned(),
        ..account
    };
    assert_eq!(
        account_session(&unknown, fixture.shared_session(), fixture.secrets.as_ref())
            .expect_err("refuses")
            .code,
        AppErrorCode::ValidationFailed
    );
}

#[tokio::test]
async fn an_account_mode_401_is_session_expired_while_an_api_mode_401_is_an_auth_failure() {
    let session = FakeSessionGateway::shared();
    let url = "https://chat.qwen.ai/api/v2/chat";
    session.queue(url, vec![FakeReply::json(401, &json!({ "code": 40_100 }))]);
    let transport = AccountTransport::new(AccountTransportOptions::new(
        "https://chat.qwen.ai/api/v2",
        session.clone(),
    ));
    assert_eq!(
        transport
            .request(&RequestSpec::get("chat"), &token())
            .await
            .expect_err("expired")
            .code,
        AppErrorCode::ProviderSessionExpired
    );
    assert_eq!(session.requests().len(), 1);
    let api = crate::drivers::ai::errors::http_failure(
        crate::drivers::ai::errors::HttpFailure::status(401),
    );
    assert_eq!(api.code, AppErrorCode::ProviderAuthFailed);
}

#[tokio::test]
async fn the_account_transport_guards_redirects_html_and_server_errors() {
    let session = FakeSessionGateway::shared();
    let base = "https://chat.qwen.ai/api/v2";
    let transport = AccountTransport::new(AccountTransportOptions {
        headers: vec![("X-Client".to_owned(), "studio".to_owned())],
        ..AccountTransportOptions::new(base, session.clone())
    });
    let mut moved = Headers::new();
    moved.insert(
        "location".to_owned(),
        "https://chat.qwen.ai/auth".to_owned(),
    );
    session.queue(
        &format!("{base}/models/"),
        vec![
            FakeReply::chunks(302, moved, Vec::new()),
            FakeReply::json(503, &json!({ "error": { "message": "busy" } })),
            FakeReply::json(200, &json!({ "data": [] })),
        ],
    );
    let spec = RequestSpec::get("/models/").with_header("x-client", "override");
    let redirected = transport
        .request(&spec, &token())
        .await
        .expect_err("signed out");
    assert_eq!(redirected.code, AppErrorCode::ProviderSessionExpired);
    let busy = transport.request(&spec, &token()).await.expect_err("busy");
    assert_eq!(busy.code, AppErrorCode::ProviderUnreachable);
    assert_eq!(
        busy.details
            .as_ref()
            .and_then(|details| details.get("vendorMessage")),
        Some(&json!("busy"))
    );
    let ok = transport.request(&spec, &token()).await.expect("answers");
    assert_eq!(ok.text, r#"{"data":[]}"#);
    let sent = session.last_request().expect("a request");
    assert_eq!(sent.url, "https://chat.qwen.ai/api/v2/models/");
    assert_eq!(sent.request.header("x-client"), Some("override"));
    assert_eq!(sent.request.header("accept"), Some("application/json"));
    assert_eq!(sent.request.header("authorization"), None);
    assert_eq!(
        sent.request
            .headers
            .iter()
            .filter(|(name, _)| name == "x-client")
            .count(),
        1
    );
    assert!(session.scopes().iter().all(CancellationToken::is_cancelled));

    session.queue(
        &format!("{base}/chat"),
        vec![FakeReply::text(
            200,
            "text/html",
            "<title>Checking your browser</title>",
        )],
    );
    let error = transport
        .stream(&RequestSpec::post("chat", json!({})), &token())
        .await
        .err()
        .expect("a verification page");
    assert_eq!(error.code, AppErrorCode::ProviderUnreachable);
    assert_eq!(
        error.message,
        crate::drivers::ai::errors::BROWSER_VERIFICATION_MESSAGE
    );
    assert_eq!(
        error
            .details
            .as_ref()
            .and_then(|details| details.get("reason")),
        Some(&json!("browser-verification"))
    );
    assert_eq!(
        session
            .last_request()
            .expect("a request")
            .request
            .header("accept"),
        Some("text/event-stream")
    );
}

#[tokio::test]
async fn html_failures_do_not_refresh_credentials_or_leak_page_content() {
    struct Credentials;
    #[async_trait]
    impl AccountCredentials for Credentials {
        async fn current(&self, _: &CancellationToken) -> Result<Option<AccountToken>, AppError> {
            Ok(Some(AccountToken::bearer("secret")))
        }
        async fn refresh(&self, _: &CancellationToken) -> Result<Option<AccountToken>, AppError> {
            panic!("an HTML response must not refresh authentication");
        }
    }
    for (status, body, reason) in [
        (
            403,
            "<html><title>Checking your browser</title>secret</html>",
            "browser-verification",
        ),
        (200, "<html>gateway secret</html>", "unexpected-html"),
        (403, "<title>Sign in</title>", "login-required"),
    ] {
        let session = FakeSessionGateway::shared();
        session.queue(QWEN_CHAT, vec![FakeReply::text(status, "text/html", body)]);
        let mut options =
            AccountTransportOptions::new("https://chat.qwen.ai/api/v2", session.clone());
        options.credentials = Some(Arc::new(Credentials));
        let transport = AccountTransport::new(options);
        let error = transport
            .stream(&RequestSpec::get("chat/completions"), &token())
            .await
            .err()
            .expect("rejects page");
        assert_eq!(
            error
                .details
                .as_ref()
                .and_then(|details| details.get("reason")),
            Some(&json!(reason))
        );
        assert!(!format!("{error:?}").contains("secret"));
        assert_eq!(session.requests().len(), 1);
    }
}

#[tokio::test]
async fn the_account_stream_yields_chunks_and_a_refused_stream_carries_the_vendor_message() {
    let session = FakeSessionGateway::shared();
    let base = "https://chat.qwen.ai/api/v2";
    let url = format!("{base}/chat/completions");
    let mut headers = Headers::new();
    headers.insert("content-type".to_owned(), "text/event-stream".to_owned());
    session.queue(
        &url,
        vec![
            FakeReply::chunks(
                200,
                headers,
                vec![b"data: a\n\n".to_vec(), b"data: b\n\n".to_vec()],
            ),
            FakeReply::json(429, &json!({ "error": "slow down" })),
        ],
    );
    let transport = AccountTransport::new(AccountTransportOptions::new(base, session.clone()));
    let bytes = transport
        .stream(&RequestSpec::post("chat/completions", json!({})), &token())
        .await
        .expect("streams")
        .map(|chunk| chunk.expect("a chunk"))
        .collect::<Vec<_>>()
        .await
        .concat();
    assert_eq!(bytes, b"data: a\n\ndata: b\n\n");
    let limited = transport
        .stream(&RequestSpec::post("chat/completions", json!({})), &token())
        .await
        .err()
        .expect("limited");
    assert_eq!(limited.code, AppErrorCode::RateLimited);
}

#[tokio::test(start_paused = true)]
async fn the_account_transport_times_out_and_cancels_the_gateway_request() {
    let session = FakeSessionGateway::shared();
    let base = "https://chat.qwen.ai/api/v2";
    let url = format!("{base}/chat");
    session.queue(&url, vec![FakeReply::Stall]);
    let transport = AccountTransport::new(AccountTransportOptions {
        timeout_seconds: Some(5.0),
        ..AccountTransportOptions::new(base, session.clone())
    });
    assert_eq!(transport.timeout_seconds(), 5.0);
    let error = transport
        .request(&RequestSpec::get("chat"), &token())
        .await
        .expect_err("times out");
    assert_eq!(error.message, TIMED_OUT_MESSAGE);
    assert_eq!(
        error
            .details
            .as_ref()
            .and_then(|details| details.get("timeoutSeconds")),
        Some(&json!(5))
    );
    assert!(session.scopes()[0].is_cancelled());

    session.queue(
        &url,
        vec![FakeReply::chunks(200, Headers::new(), vec![b"first".to_vec()]).holding()],
    );
    let mut stream = transport
        .stream(&RequestSpec::get("chat"), &token())
        .await
        .expect("streams");
    assert_eq!(
        stream.next().await.expect("a chunk").expect("data"),
        b"first"
    );
    let ended = stream
        .next()
        .await
        .expect("an outcome")
        .expect_err("times out");
    assert_eq!(ended.message, TIMED_OUT_MESSAGE);
    assert!(stream.next().await.is_none());
    drop(stream);
    assert!(session.scopes()[1].is_cancelled());

    let cancel = token();
    let mut stream = transport
        .stream(&RequestSpec::get("chat"), &cancel)
        .await
        .expect("streams");
    assert!(stream.next().await.expect("a chunk").is_ok());
    cancel.cancel();
    assert_eq!(
        stream
            .next()
            .await
            .expect("an outcome")
            .expect_err("cancelled")
            .code,
        AppErrorCode::RunCancelled
    );
    let early = token();
    early.cancel();
    assert_eq!(
        transport
            .request(&RequestSpec::get("chat"), &early)
            .await
            .expect_err("cancelled")
            .code,
        AppErrorCode::RunCancelled
    );
    assert_eq!(session.requests().len(), 3);
    assert!(!format!("{transport:?}").contains("token"));
}
