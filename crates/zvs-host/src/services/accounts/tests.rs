use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use zvs_contract::{
    AdapterFamily, AuthMode, HostEventDraft, ProviderCapability, ProviderKind, ProviderStatus,
    RunOutcomeStatus,
};

use super::*;
use crate::data::repositories::{ModelDraft, ProviderDraft, SecretFilter};
use crate::data::schema::{ProviderRow, ProviderSettings};
use crate::drivers::ai::identity::AccountIdentity;
use crate::drivers::ai::ports::DiscoveredModel;
use crate::drivers::ai::transport::{AccountToken, SharedSessionGateway};
use crate::platform::clock::{CLOCK_ORIGIN, Clock};
use crate::platform::events::EventBusOptions;
use crate::services::providers::{ProbeInput, ProviderService, ProviderServiceOptions};
use crate::testing::fake_accounts::{FAKE_IDENTITY_ENDPOINT, FAKE_QWEN_LOGIN};
use crate::testing::{
    FakeCrypto, FakeDriver, FakeDriverSource, FakeIdentityProbes, FakeSessionGateway, MemorySink,
    ProbeStep, RecordingBrowser, RecordingNavigator, TemporaryDatabase, TokioClock, fake_identity,
    signed_out, temporary_database,
};

const QWEN: AccountFamily = AccountFamily::QwenWeb;

fn openai_credentials(client: &str, subject: &str) -> zvs_openai::Credentials {
    zvs_openai::Credentials {
        client_id: client.to_owned(),
        host_id: "urn:uuid:host".to_owned(),
        subject: subject.to_owned(),
        email: Some("person@example.com".to_owned()),
        name: Some("Person".to_owned()),
        access_token: "private-openai-access".to_owned(),
        refresh_token: "private-openai-refresh".to_owned(),
        id_token: "private-openai-id-token".to_owned(),
        scope: zvs_openai::SCOPES.to_owned(),
        expires_at: 2_000_000_000,
        earliest_refresh_at: None,
    }
}

#[tokio::test]
async fn openai_registrations_are_separate_and_cookies_do_not_revoke_oauth() {
    let harness = harness();
    harness
        .service
        .inner
        .save_openai(openai_credentials("client-a", "subject"))
        .expect("first");
    harness
        .service
        .inner
        .save_openai(openai_credentials("client-b", "subject"))
        .expect("second");
    let accounts = harness.list();
    assert_eq!(accounts.len(), 2);
    assert_eq!(harness.secret_count(), 2);
    let printed = serde_json::to_string(&accounts).expect("DTOs");
    assert!(!printed.contains("private-openai"));
    assert!(!printed.contains("person@example.com"));
    harness
        .service
        .refresh(&accounts[0].id.to_string())
        .await
        .expect("saved OAuth session");
    harness
        .lifecycle
        .emit(&BrowserLifecycleEvent::CookiesCleared {
            domain: "openai.com".to_owned(),
        });
    assert!(
        harness
            .list()
            .iter()
            .all(|account| account.status == AccountStatus::Linked)
    );
    harness
        .service
        .inner
        .save_openai(openai_credentials("client-a", "subject"))
        .expect("same registration");
    assert_eq!(harness.list().len(), 2);
    assert_eq!(harness.secret_count(), 2);
}

#[tokio::test]
async fn openai_without_plan_permission_is_saved_without_enabling_inference() {
    let harness = harness();
    let mut credentials = openai_credentials("client", "subject");
    credentials.scope = "openid profile email".to_owned();
    harness
        .service
        .inner
        .save_openai(credentials)
        .expect("identity saved");
    assert_eq!(harness.first().status, AccountStatus::NeedsRelink);
    assert_eq!(harness.secret_count(), 1);
}

#[tokio::test]
async fn account_usage_opens_only_the_fixed_openai_page() {
    let harness = harness();
    harness
        .service
        .inner
        .save_openai(openai_credentials("client", "subject"))
        .expect("saved");
    let id = harness.first().id.to_string();
    harness.service.open_usage(&id).await.expect("opened");
    assert_eq!(
        harness.browser.opened(),
        ["https://chatgpt.com/#settings/Usage"]
    );
    assert!(harness.service.open_usage("missing").await.is_err());
}

#[tokio::test]
async fn an_openai_provider_without_an_account_never_adopts_a_new_registration() {
    let harness = harness();
    harness
        .service
        .inner
        .save_openai(openai_credentials("old-client", "subject"))
        .expect("original registration");
    let original_id = harness.first().id.to_string();
    let orphan = harness
        .database
        .with_connection(|connection| {
            Repositories::new(connection)
                .providers
                .create(&ProviderDraft {
                    adapter: AdapterFamily::Openai,
                    auth_mode: AuthMode::Account,
                    account_id: Some(original_id.clone()),
                    ..ProviderDraft::new(
                        ProviderKind::OpenaiCompatible,
                        "OpenAI orphan",
                        zvs_openai::API_BASE,
                        vec![ProviderCapability::Text],
                        harness.clock.now(),
                    )
                })
        })
        .expect("orphan provider");
    harness
        .database
        .with_connection(|connection| {
            Ok(Repositories::new(connection)
                .accounts
                .remove(&original_id, harness.clock.now())?)
        })
        .expect("detach original registration");
    let orphan = harness.provider(&orphan.id);
    harness
        .service
        .inner
        .save_openai(openai_credentials("new-client", "subject"))
        .expect("new registration");
    assert!(harness.provider(&orphan.id).account_id.is_none());
    let accounts = crate::drivers::ai::registry::DatabaseAccounts::new(
        harness.database.shared(),
        None,
        harness.secrets.clone(),
        silent_logger(),
    );
    let error = crate::drivers::ai::registry::AccountPort::transport(&accounts, &orphan)
        .err()
        .expect("explicit binding required");
    assert_eq!(error.code, AppErrorCode::ProviderSessionExpired);
    assert!(harness.provider(&orphan.id).account_id.is_none());
}

#[tokio::test]
async fn openai_link_uses_a_stable_host_and_reuses_the_selected_registration() {
    let harness = harness();
    let first = harness.service.link(AccountFamily::Openai);
    settle().await;
    let opened = harness.browser.opened();
    assert_eq!(opened.len(), 1);
    let url = Url::parse(&opened[0]).expect("authorization URL");
    let first_query = url.query_pairs().into_owned().collect::<HashMap<_, _>>();
    assert_eq!(first_query["client_id"], "dynamic_agent_client");
    assert_eq!(first_query["code_challenge_method"], "S256");
    let host = first_query["ext_agent_host_id"].clone();
    let id =
        uuid::Uuid::parse_str(host.strip_prefix("urn:uuid:").expect("UUID URI")).expect("UUID");
    assert_eq!(id.get_version_num(), 4);
    assert!(harness.service.cancel_link(AccountFamily::Openai).cancelled);
    assert!(matches!(
        first.await.expect("cancelled"),
        AccountLinkResult::Cancelled { .. }
    ));
    assert!(matches!(&harness.sink.events().last().expect("end").body,
        HostEventDraft::End { outcome } if outcome.status == RunOutcomeStatus::Cancelled));

    let mut saved_registration = openai_credentials("saved-client", "subject");
    saved_registration.scope = "openid profile email".to_owned();
    harness
        .service
        .inner
        .save_openai(saved_registration)
        .expect("registration");
    let account = harness.first();
    let second = harness
        .service
        .link_account(AccountFamily::Openai, Some(account.id.to_string()));
    settle().await;
    let opened = harness.browser.opened();
    let url = Url::parse(&opened[1]).expect("reauthorization URL");
    let second_query = url.query_pairs().into_owned().collect::<HashMap<_, _>>();
    assert_eq!(second_query["client_id"], "saved-client");
    assert_eq!(second_query["prompt"], "consent");
    assert_eq!(second_query["ext_agent_host_id"], host);
    assert_ne!(second_query["state"], first_query["state"]);
    assert!(!second_query.contains_key("agent_name_hint"));
    harness.service.cancel_link(AccountFamily::Openai);
    assert!(matches!(
        second.await.expect("cancelled"),
        AccountLinkResult::Cancelled { .. }
    ));
    assert_eq!(harness.list().len(), 1);
}

struct Harness {
    database: TemporaryDatabase,
    secrets: Arc<SecretService>,
    service: AccountService,
    probes: Arc<FakeIdentityProbes>,
    browser: Arc<RecordingBrowser>,
    navigator: Arc<RecordingNavigator>,
    lifecycle: BrowserLifecycle,
    sink: Arc<MemorySink>,
    clock: Arc<TokioClock>,
    session: Arc<FakeSessionGateway>,
}

fn harness() -> Harness {
    let database = temporary_database();
    let clock = Arc::new(TokioClock::new(CLOCK_ORIGIN));
    let secrets = Arc::new(SecretService::new(
        database.shared(),
        Arc::new(crate::services::CryptoService::new(FakeCrypto::shared())),
        clock.clone(),
        silent_logger(),
    ));
    let probes = FakeIdentityProbes::shared();
    let browser = RecordingBrowser::shared();
    let navigator = RecordingNavigator::shared();
    let lifecycle = BrowserLifecycle::new();
    let sink = MemorySink::shared();
    let events = EventBus::new(EventBusOptions::default());
    events.attach(sink.clone());
    let fake_session = FakeSessionGateway::shared();
    let session: SharedSessionGateway = fake_session.clone();
    let sessions: SessionFactory = Arc::new(move |_partition: &str| session.clone());
    let service = AccountService::new(AccountServiceOptions {
        probes: probes.factory(),
        clock: clock.clone(),
        random: Arc::new(|| 0.5),
        timeout: Duration::from_millis(6000),
        interval: Duration::from_millis(2000),
        ..AccountServiceOptions::new(
            database.shared(),
            Arc::clone(&secrets),
            sessions,
            browser.clone(),
            lifecycle.clone(),
            events,
            navigator.clone(),
        )
    });
    Harness {
        database,
        secrets,
        service,
        probes,
        browser,
        navigator,
        lifecycle,
        sink,
        clock,
        session: fake_session,
    }
}

async fn settle() {
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }
}

async fn advance(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
    settle().await;
}

fn without_expiry(token: &str) -> ProbeResult {
    ProbeResult {
        identity: AccountIdentity::new("user-1"),
        credential: Some(AccountToken::bearer(token)),
    }
}

fn unreachable(message: &str) -> AppError {
    AppError::new(AppErrorCode::ProviderUnreachable, message)
}

impl Harness {
    fn list(&self) -> Vec<AccountDto> {
        self.service.list().expect("lists")
    }

    fn first(&self) -> AccountDto {
        self.list().remove(0)
    }

    fn row(&self, id: &str) -> Option<AccountRow> {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).accounts.find_by_id(id)?)
            })
            .expect("reads")
    }

    fn token_of(&self, id: &str) -> String {
        let secret_id = self
            .row(id)
            .and_then(|row| row.token_secret_id)
            .expect("a token secret");
        self.secrets
            .resolve(&secret_id)
            .expect("resolves")
            .expose()
            .to_owned()
    }

    fn secret_count(&self) -> usize {
        self.secrets
            .list(&SecretFilter::default())
            .expect("lists")
            .len()
    }

    fn provider(&self, id: &str) -> ProviderRow {
        self.database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).providers.find_by_id(id)?)
            })
            .expect("reads")
            .expect("a provider")
    }

    fn attach(&self, account_id: &str) -> ProviderRow {
        let now = self.clock.now();
        let mut settings = ProviderSettings::default();
        settings.0.insert("temperature".to_owned(), json!(0.7));
        self.database
            .transaction(|transaction| {
                Repositories::new(transaction)
                    .providers
                    .create(&ProviderDraft {
                        adapter: AdapterFamily::QwenWeb,
                        auth_mode: AuthMode::Account,
                        account_id: Some(account_id.to_owned()),
                        settings,
                        ..ProviderDraft::new(
                            ProviderKind::OpenaiCompatible,
                            "Qwen",
                            "https://chat.qwen.ai",
                            vec![ProviderCapability::Text],
                            now,
                        )
                    })
            })
            .expect("a provider")
    }

    async fn link(&self, family: AccountFamily) -> AccountLinkResult {
        self.service.link(family).await.expect("links")
    }
}

fn linked_account(result: &AccountLinkResult) -> &AccountDto {
    match result {
        AccountLinkResult::Linked { account, .. } => account,
        other => panic!("not linked: {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn polling_stores_a_masked_identity_and_encrypted_token_with_progress_and_done_events() {
    let harness = harness();
    harness.probes.once(ProbeStep::Fail(signed_out()));
    let pending = harness.service.link(QWEN);
    settle().await;
    assert_eq!(harness.browser.opened(), [FAKE_QWEN_LOGIN]);
    assert!(harness.list().is_empty());
    let result = pending.await.expect("links");
    let dto = harness.first();
    assert_eq!(linked_account(&result), &dto);
    assert_eq!(dto.email_masked.as_deref(), Some("p***@example.com"));
    assert_eq!(dto.expires_at, Some(2_000_000_000_000));
    assert_eq!(
        harness.token_of(&dto.id.to_string()),
        "private-bearer-token"
    );
    let events = harness.sink.events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event.body, HostEventDraft::Progress { total: 6000, .. }))
    );
    assert!(events.iter().any(|event| matches!(
        &event.body,
        HostEventDraft::Step { step } if step.get("phase") == Some(&json!("done"))
    )));
    assert!(matches!(
        &events.last().expect("an end").body,
        HostEventDraft::End { outcome } if outcome.status == RunOutcomeStatus::Ok
    ));
    let printed = format!(
        "{}{}{}",
        serde_json::to_string(&result).expect("serialises"),
        serde_json::to_string(&events).expect("serialises"),
        serde_json::to_string(&harness.list()).expect("serialises")
    );
    for leak in [
        "private-bearer",
        "tokenSecret",
        "partition",
        "person@example",
    ] {
        assert!(!printed.contains(leak), "{leak}");
    }
    assert_eq!(harness.navigator.routes(), [WorkspaceRoute::Providers]);
    assert!(!harness.service.is_linking(QWEN));
}

#[tokio::test(start_paused = true)]
async fn a_timeout_writes_nothing_and_makes_no_further_requests() {
    let harness = harness();
    harness.probes.always(ProbeStep::Fail(signed_out()));
    let result = harness.service.link(QWEN).await.expect("settles");
    assert_eq!(
        result,
        AccountLinkResult::Timeout {
            detail: NO_SIGN_IN.to_owned()
        }
    );
    assert!(harness.list().is_empty());
    assert_eq!(harness.secret_count(), 0);
    let count = harness.probes.calls();
    advance(60_000).await;
    assert_eq!(harness.probes.calls(), count);
    assert!(
        harness
            .probes
            .tokens()
            .iter()
            .all(CancellationToken::is_cancelled)
    );
    assert!(matches!(
        &harness.sink.events().last().expect("an end").body,
        HostEventDraft::End { outcome } if outcome.status == RunOutcomeStatus::Cancelled
    ));
    assert!(harness.navigator.routes().is_empty());
}

#[tokio::test(start_paused = true)]
async fn temporary_vendor_failures_do_not_end_sign_in_before_the_browser_authenticates() {
    let harness = harness();
    harness
        .probes
        .once(ProbeStep::Fail(unreachable("Connection reset")));
    let pending = harness.service.link(QWEN);
    settle().await;
    assert!(harness.list().is_empty());
    assert!(matches!(
        pending.await.expect("links"),
        AccountLinkResult::Linked { .. }
    ));
    assert_eq!(harness.probes.calls(), 2);
}

#[tokio::test(start_paused = true)]
async fn an_unexpected_vendor_failure_ends_linking_as_unreachable() {
    let harness = harness();
    harness.probes.once(ProbeStep::Fail(AppError::new(
        AppErrorCode::ValidationFailed,
        "Вендор вернул неизвестный формат профиля",
    )));
    let error = harness.service.link(QWEN).await.expect_err("fails");
    assert_eq!(error.code, AppErrorCode::ProviderUnreachable);
    assert_eq!(error.message, VENDOR_UNCHECKED);
    assert!(matches!(
        &harness.sink.events().last().expect("an end").body,
        HostEventDraft::End { outcome }
            if outcome.status == RunOutcomeStatus::Failed
                && outcome.message.as_deref() == Some(LINKING_FAILED)
    ));
    harness.browser.fail_with(AppError::new(
        AppErrorCode::Unknown,
        "Browser is unavailable",
    ));
    assert_eq!(
        harness.service.link(QWEN).await.expect_err("fails").message,
        "Browser is unavailable"
    );
}

#[tokio::test(start_paused = true)]
async fn automatic_refresh_saves_rotated_credentials_without_opening_a_login_tab() {
    let harness = harness();
    let soon = harness.clock.now() / 1000 + 120;
    harness.probes.once(ProbeStep::Identity(ProbeResult {
        identity: AccountIdentity {
            expires_at: Some(soon),
            ..AccountIdentity::new("user-1")
        },
        credential: Some(AccountToken::bearer("private-bearer-token")),
    }));
    harness.link(QWEN).await;
    let id = harness.first().id.to_string();
    harness.probes.always(ProbeStep::Identity(fake_identity(
        "user-1",
        "rotated-token",
    )));
    harness.service.start_auto_refresh().expect("starts");
    harness.service.start_auto_refresh().expect("starts once");
    advance(60_000).await;
    assert_eq!(harness.probes.calls(), 2);
    assert_eq!(harness.browser.opened().len(), 1);
    assert_eq!(harness.token_of(&id), "rotated-token");
    harness.service.dispose().await;
    advance(120_000).await;
    assert_eq!(harness.probes.calls(), 2);
    assert_eq!(
        harness
            .service
            .start_auto_refresh()
            .expect_err("closed")
            .message,
        SERVICE_CLOSED
    );
}

#[tokio::test(start_paused = true)]
async fn automatic_refresh_checks_unknown_expiry_every_fifteen_minutes_and_stops_on_sign_out() {
    let harness = harness();
    harness
        .probes
        .once(ProbeStep::Identity(without_expiry("private-bearer-token")));
    harness.link(QWEN).await;
    harness.probes.always(ProbeStep::Fail(signed_out()));
    harness.service.start_auto_refresh().expect("starts");
    advance(14 * 60_000).await;
    assert_eq!(harness.probes.calls(), 1);
    advance(60_000).await;
    assert_eq!(harness.first().status, AccountStatus::NeedsRelink);
    assert_eq!(
        harness.first().detail.as_deref(),
        Some(SESSION_EXPIRED_DETAIL)
    );
    harness
        .probes
        .always(ProbeStep::Identity(without_expiry("rotated-token")));
    advance(15 * 60_000).await;
    assert_eq!(harness.probes.calls(), 3);
    assert_eq!(harness.first().status, AccountStatus::Linked);
    assert_eq!(harness.browser.opened().len(), 1);
    harness.service.dispose().await;
}

#[tokio::test(start_paused = true)]
async fn a_lapsed_session_is_rechecked_as_soon_as_the_service_starts_again() {
    let harness = harness();
    harness.link(QWEN).await;
    let id = harness.first().id.to_string();
    harness.probes.once(ProbeStep::Fail(signed_out()));
    harness.service.refresh(&id).await.expect("refreshes");
    assert_eq!(harness.first().status, AccountStatus::NeedsRelink);
    harness.service.start_auto_refresh().expect("starts");
    settle().await;
    assert_eq!(harness.probes.calls(), 3);
    assert_eq!(harness.first().status, AccountStatus::Linked);
    harness.service.dispose().await;
}

#[tokio::test(start_paused = true)]
async fn a_rejected_session_is_renewed_by_its_client_before_the_account_needs_a_relink() {
    let harness = harness();
    harness.link(QWEN).await;
    let id = harness.first().id.to_string();
    harness.session.renewable(true);
    harness.probes.once(ProbeStep::Fail(signed_out()));
    let renewed = harness.service.refresh(&id).await.expect("refreshes");
    assert_eq!(renewed.status, AccountStatus::Linked);
    assert_eq!(harness.probes.calls(), 3);
    assert_eq!(
        harness.session.renewals(),
        vec![FAKE_IDENTITY_ENDPOINT.to_owned(); 3]
    );

    harness.probes.once(ProbeStep::Fail(signed_out()));
    harness.probes.once(ProbeStep::Fail(signed_out()));
    let lapsed = harness.service.refresh(&id).await.expect("refreshes");
    assert_eq!(lapsed.status, AccountStatus::NeedsRelink);
    assert_eq!(lapsed.detail.as_deref(), Some(SESSION_EXPIRED_DETAIL));
    assert_eq!(harness.session.renewals().len(), 4);

    harness.session.renewable(false);
    harness.probes.once(ProbeStep::Fail(unreachable("Offline")));
    harness.service.refresh(&id).await.expect_err("offline");
    assert_eq!(harness.session.renewals().len(), 4);
}

#[tokio::test(start_paused = true)]
async fn automatic_refresh_backs_off_after_transient_failures_and_recovers() {
    let harness = harness();
    harness
        .probes
        .once(ProbeStep::Identity(without_expiry("private-bearer-token")));
    harness.link(QWEN).await;
    harness.probes.once(ProbeStep::Fail(unreachable("Offline")));
    harness.service.start_auto_refresh().expect("starts");
    advance(15 * 60_000).await;
    assert_eq!(harness.first().status, AccountStatus::Linked);
    advance(14 * 60_000).await;
    assert_eq!(harness.probes.calls(), 2);
    advance(60_000).await;
    assert_eq!(harness.probes.calls(), 3);
    harness.service.dispose().await;
}

#[tokio::test(start_paused = true)]
async fn persistent_vendor_failures_still_stop_at_the_sign_in_deadline() {
    let harness = harness();
    harness
        .probes
        .always(ProbeStep::Fail(unreachable("Unavailable")));
    assert!(matches!(
        harness.service.link(QWEN).await.expect("settles"),
        AccountLinkResult::Timeout { .. }
    ));
    assert!(harness.list().is_empty());
}

#[derive(Clone, Copy, Debug)]
enum Cancel {
    Explicit,
    Tab,
    Workspace,
    Dispose,
}

#[tokio::test(start_paused = true)]
async fn every_cancellation_aborts_an_in_flight_probe_even_if_the_probe_ignores_abort() {
    for mode in [
        Cancel::Explicit,
        Cancel::Tab,
        Cancel::Workspace,
        Cancel::Dispose,
    ] {
        let harness = harness();
        harness.probes.always(ProbeStep::Hang);
        let pending = harness.service.link(QWEN);
        settle().await;
        match mode {
            Cancel::Explicit => {
                assert!(harness.service.cancel_link(QWEN).cancelled);
            }
            Cancel::Tab => harness
                .lifecycle
                .emit(&BrowserLifecycleEvent::LinkTabClosed {
                    url: FAKE_QWEN_LOGIN.to_owned(),
                }),
            Cancel::Workspace => harness
                .lifecycle
                .emit(&BrowserLifecycleEvent::WorkspaceClosed),
            Cancel::Dispose => harness.service.dispose().await,
        }
        assert_eq!(
            pending.await.expect("settles"),
            AccountLinkResult::Cancelled {
                detail: NO_SIGN_IN.to_owned()
            },
            "{mode:?}"
        );
        assert!(harness.probes.tokens()[0].is_cancelled(), "{mode:?}");
        harness
            .probes
            .release(fake_identity("user-1", "private-bearer-token"));
        advance(60_000).await;
        assert!(harness.list().is_empty(), "{mode:?}");
        assert_eq!(harness.probes.calls(), 1, "{mode:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn cancelling_during_the_polling_delay_stops_further_probes() {
    let harness = harness();
    harness.probes.always(ProbeStep::Fail(signed_out()));
    let pending = harness.service.link(QWEN);
    settle().await;
    harness.service.cancel_link(QWEN);
    assert!(matches!(
        pending.await.expect("settles"),
        AccountLinkResult::Cancelled { .. }
    ));
    advance(60_000).await;
    assert_eq!(harness.probes.calls(), 1);
    assert!(!harness.service.cancel_link(QWEN).cancelled);
}

#[tokio::test(start_paused = true)]
async fn duplicate_linking_joins_one_loop_and_cancelling_before_it_starts_opens_nothing() {
    let harness = harness();
    let events = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&events);
    let _subscription = harness
        .lifecycle
        .subscribe(move |event| recorded.lock().expect("events").push(event.clone()));
    let first = harness.service.link(QWEN);
    let second = harness.service.link(QWEN);
    harness.service.cancel_link(QWEN);
    let (first, second) = futures::join!(first, second);
    assert_eq!(first.expect("settles"), second.expect("joins"));
    assert!(harness.browser.opened().is_empty());
    assert_eq!(harness.probes.calls(), 0);
    assert_eq!(
        *events.lock().expect("events"),
        [BrowserLifecycleEvent::LinkFinished {
            url: FAKE_QWEN_LOGIN.to_owned()
        }]
    );
}

#[tokio::test(start_paused = true)]
async fn the_same_identity_retains_its_row_provider_settings_and_default_model() {
    let harness = harness();
    harness.link(QWEN).await;
    let row = harness.first();
    let provider = harness.attach(&row.id.to_string());
    let now = harness.clock.now();
    harness
        .database
        .transaction(|transaction| {
            let repositories = Repositories::new(transaction);
            let models = repositories.models.replace_for_provider(
                &provider.id,
                &[ModelDraft::discovered(&DiscoveredModel::named("qwen"), now)],
                now,
            )?;
            repositories
                .providers
                .set_default_model(&provider.id, Some(&models[0].id), now)?;
            Ok(())
        })
        .expect("models");
    let before = harness.provider(&provider.id);
    harness.probes.always(ProbeStep::Identity(fake_identity(
        "user-1",
        "replacement-token",
    )));
    let result = harness.link(QWEN).await;
    let AccountLinkResult::Linked {
        account,
        replaced_previous_identity,
        detail,
    } = result
    else {
        panic!("not linked");
    };
    assert!(!replaced_previous_identity);
    assert_eq!(detail, None);
    assert_eq!(account.id, row.id);
    assert_eq!(account.linked_providers_count, 1);
    assert_eq!(harness.provider(&provider.id), before);
    assert_eq!(harness.secret_count(), 1);
    assert_eq!(harness.token_of(&row.id.to_string()), "replacement-token");
}

#[tokio::test(start_paused = true)]
async fn a_different_identity_replaces_the_previous_account_and_deletes_its_token() {
    let harness = harness();
    harness.link(QWEN).await;
    let old = harness.first();
    let provider = harness.attach(&old.id.to_string());
    harness.probes.always(ProbeStep::Identity(fake_identity(
        "user-2",
        "new-identity-token",
    )));
    let result = harness.link(QWEN).await;
    assert!(matches!(
        &result,
        AccountLinkResult::Linked {
            replaced_previous_identity: true,
            detail: Some(detail),
            ..
        } if detail == IDENTITY_REPLACED
    ));
    assert_eq!(harness.list().len(), 1);
    assert_eq!(harness.row(&old.id.to_string()), None);
    // The provider follows the new identity instead of waiting to be reattached.
    let current = harness.first();
    let attached = harness.provider(&provider.id);
    assert_eq!(
        attached.account_id.as_deref(),
        Some(current.id.to_string().as_str())
    );
    assert_eq!(attached.status, ProviderStatus::Unknown);
    assert_eq!(attached.status_detail, None);
    assert_eq!(harness.secret_count(), 1);
}

#[tokio::test(start_paused = true)]
async fn unlink_preserves_the_browser_session_and_providers_while_deleting_the_token() {
    let harness = harness();
    harness.link(QWEN).await;
    let row = harness.first();
    let provider = harness.attach(&row.id.to_string());
    assert_eq!(
        harness
            .service
            .unlink(&row.id.to_string())
            .expect("unlinks"),
        AccountsUnlinkOutput {
            id: row.id,
            removed: true,
            browser_session_preserved: true,
        }
    );
    assert!(harness.list().is_empty());
    let detached = harness.provider(&provider.id);
    assert_eq!(detached.account_id, None);
    assert_eq!(detached.status, ProviderStatus::Failed);
    assert_eq!(harness.secret_count(), 0);
    assert_eq!(harness.browser.opened().len(), 1);
    assert_eq!(
        harness
            .service
            .unlink(&row.id.to_string())
            .expect_err("gone")
            .code,
        AppErrorCode::NotFound
    );
}

#[tokio::test(start_paused = true)]
async fn cookie_clearing_marks_only_the_matching_vendor_and_derives_session_expired_without_outbound_calls()
 {
    let harness = harness();
    harness.link(QWEN).await;
    let row = harness
        .list()
        .into_iter()
        .find(|account| account.adapter == QWEN)
        .expect("qwen");
    let provider = harness.attach(&row.id.to_string());
    let status_of = |family: AccountFamily| {
        harness
            .list()
            .into_iter()
            .find(|account| account.adapter == family)
            .expect("an account")
            .status
    };
    harness
        .lifecycle
        .emit(&BrowserLifecycleEvent::CookiesCleared {
            domain: "evilqwen.ai".to_owned(),
        });
    assert_eq!(status_of(QWEN), AccountStatus::Linked);
    harness
        .lifecycle
        .emit(&BrowserLifecycleEvent::CookiesCleared {
            domain: ".qwen.ai".to_owned(),
        });
    assert_eq!(status_of(QWEN), AccountStatus::NeedsRelink);
    assert_eq!(
        harness
            .row(&row.id.to_string())
            .and_then(|row| row.status_detail)
            .as_deref(),
        Some(COOKIES_CLEARED_DETAIL)
    );

    let providers = ProviderService::new(ProviderServiceOptions::new(
        harness.database.shared(),
        FakeDriverSource::shared(FakeDriver::with_models(Vec::new())),
        Arc::clone(&harness.secrets),
        silent_logger(),
        harness.clock.clone(),
    ));
    let dto = providers.get(&provider.id, false).expect("a provider");
    assert_eq!(dto.summary.status, ProviderStatus::Failed);
    assert_eq!(
        dto.summary.status_detail.as_deref(),
        Some("Сессия истекла — войдите заново")
    );
    let probed = providers
        .probe(ProbeInput::Saved(provider.id.clone()), None)
        .await
        .expect("probes");
    assert_eq!(probed.outcome.kind(), "session-expired");
    assert_eq!(harness.probes.calls(), 1);
    providers.dispose().await;
}

#[tokio::test(start_paused = true)]
async fn refresh_updates_health_expiry_and_token_and_rejects_mismatched_identities() {
    let harness = harness();
    harness.link(QWEN).await;
    let id = harness.first().id.to_string();
    harness.probes.once(ProbeStep::Fail(signed_out()));
    assert_eq!(
        harness
            .service
            .refresh(&id)
            .await
            .expect("refreshes")
            .status,
        AccountStatus::NeedsRelink
    );
    harness.probes.once(ProbeStep::Identity(fake_identity(
        "other-user",
        "private-bearer-token",
    )));
    let mismatch = harness.service.refresh(&id).await.expect("refreshes");
    assert_eq!(mismatch.status, AccountStatus::NeedsRelink);
    assert_eq!(mismatch.detail.as_deref(), Some(DIFFERENT_ACCOUNT));
    harness.probes.once(ProbeStep::Identity(ProbeResult {
        identity: AccountIdentity {
            expires_at: Some(3_000_000_000),
            ..AccountIdentity::new("user-1")
        },
        credential: None,
    }));
    let refreshed = harness.service.refresh(&id).await.expect("refreshes");
    assert_eq!(refreshed.status, AccountStatus::Linked);
    assert_eq!(refreshed.expires_at, Some(3_000_000_000_000));
    assert_eq!(harness.secret_count(), 0);

    harness.probes.once(ProbeStep::Fail(unreachable("Offline")));
    let offline = harness.service.refresh(&id).await.expect_err("offline");
    assert_eq!(offline.message, VENDOR_UNCHECKED);
    assert_eq!(harness.first().status, AccountStatus::Linked);
    assert_eq!(
        harness
            .service
            .refresh("missing")
            .await
            .expect_err("missing")
            .message,
        ACCOUNT_NOT_FOUND
    );
}

#[tokio::test(start_paused = true)]
async fn a_refresh_is_refused_while_signing_in_and_joins_a_running_refresh() {
    let harness = harness();
    harness.link(QWEN).await;
    let id = harness.first().id.to_string();
    harness.probes.always(ProbeStep::Hang);
    let first = harness.service.refresh(&id);
    let second = harness.service.refresh(&id);
    harness
        .probes
        .release(fake_identity("user-1", "private-bearer-token"));
    let (first, second) = futures::join!(first, second);
    assert_eq!(first.expect("refreshes"), second.expect("joins"));
    assert_eq!(harness.probes.calls(), 2);

    harness.probes.always(ProbeStep::Fail(signed_out()));
    let linking = harness.service.link(QWEN);
    settle().await;
    assert_eq!(
        harness
            .service
            .refresh(&id)
            .await
            .expect_err("busy")
            .message,
        SIGN_IN_IN_PROGRESS
    );
    harness.service.cancel_link(QWEN);
    linking.await.expect("settles");
}

#[tokio::test(start_paused = true)]
async fn cookie_clearing_aborts_a_refresh_so_a_late_response_cannot_restore_linked_status() {
    let harness = harness();
    harness.link(QWEN).await;
    let id = harness.first().id.to_string();
    harness.probes.always(ProbeStep::Hang);
    let pending = harness.service.refresh(&id);
    settle().await;
    harness
        .lifecycle
        .emit(&BrowserLifecycleEvent::CookiesCleared {
            domain: "chat.qwen.ai".to_owned(),
        });
    assert_eq!(
        pending.await.expect_err("aborted").message,
        OPERATION_CANCELLED
    );
    harness
        .probes
        .release(fake_identity("user-1", "private-bearer-token"));
    settle().await;
    assert_eq!(harness.first().status, AccountStatus::NeedsRelink);
}

#[tokio::test(start_paused = true)]
async fn a_disposed_service_refuses_new_work() {
    let harness = harness();
    harness.service.dispose().await;
    assert_eq!(
        harness
            .service
            .link(QWEN)
            .await
            .expect_err("closed")
            .message,
        SERVICE_CLOSED
    );
    assert!(harness.list().is_empty());
}

#[test]
fn emails_are_masked_to_their_first_character_and_domain() {
    assert_eq!(
        mask_email(Some("person@example.com")).as_deref(),
        Some("p***@example.com")
    );
    assert_eq!(
        mask_email(Some("кирилл@mail.ru")).as_deref(),
        Some("к***@mail.ru")
    );
    assert_eq!(mask_email(Some("@example.com")).as_deref(), Some("***"));
    assert_eq!(mask_email(Some("no-at")).as_deref(), Some("***"));
    assert_eq!(mask_email(Some("")), None);
    assert_eq!(mask_email(None), None);
}

#[tokio::test(start_paused = true)]
async fn linking_waits_for_a_real_sign_in_while_the_browser_session_cannot_be_renewed() {
    let harness = harness();
    harness.session.revoked(true);
    let pending = tokio::spawn({
        let service = harness.service.clone();
        async move { service.link(QWEN).await }
    });
    advance(2500).await;
    assert!(harness.list().is_empty());
    assert!(!harness.session.renewals().is_empty());
    harness.session.revoked(false);
    let result = pending.await.expect("joins").expect("links");
    assert_eq!(linked_account(&result).status, AccountStatus::Linked);
}

#[tokio::test(start_paused = true)]
async fn a_check_marks_an_account_whose_session_cannot_be_renewed_for_a_relink() {
    let harness = harness();
    harness.link(QWEN).await;
    let id = harness.first().id.to_string();
    harness.session.revoked(true);
    let checked = harness.service.refresh(&id).await.expect("refreshes");
    assert_eq!(checked.status, AccountStatus::NeedsRelink);
    assert_eq!(checked.detail.as_deref(), Some(SESSION_EXPIRED_DETAIL));
}
