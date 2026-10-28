use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use zvs_contract::{
    AppError, AppErrorCode, CreateIntegrationInput, CreateScenarioInput, FieldDescriptor,
    FieldKind, FieldOption, IntegrationAccountDto, IntegrationActivityInput, IntegrationCheckDto,
    IntegrationConfig, IntegrationKind, IntegrationStatus, IntegrationToolDto,
    IntegrationTriggerDto, IntegrationValue, RunListFilter, ScenarioEdgeDto, ScenarioGraphDto,
    ScenarioNodeDto, ScenarioPositionDto, SecretId, SecretScope, ToolSource, ToolTier,
    UpdateIntegrationInput,
};

use super::*;
use crate::data::schema::SecretConsumerKind;
use crate::platform::clock::FakeClock;
use crate::platform::events::EventBus;
use crate::platform::logger::silent_logger;
use crate::services::{SecretCreateInput, SystemService};
use crate::testing::{TestServices, test_services};
use crate::workers::{Backoff, WorkerFault, WorkerSupervisor};

const EVENT: &str = "fake.event";
const READ: &str = "fake.read";
const SEND: &str = "fake.send";
const OFFLINE: &str = "Сервер недоступен";

fn field(key: &str, kind: FieldKind, required: bool) -> FieldDescriptor {
    FieldDescriptor {
        key: key.to_owned(),
        label: key.to_owned(),
        kind,
        required,
        placeholder: None,
        help: None,
        options: None,
        default: None,
    }
}

struct FakeProvider {
    fields: Vec<FieldDescriptor>,
    starts: AtomicUsize,
    stops: AtomicUsize,
    calls: Mutex<Vec<(String, String)>>,
}

impl FakeProvider {
    fn new() -> Self {
        let mut mode = field("mode", FieldKind::Select, false);
        mode.options = Some(
            ["steady", "fail"]
                .map(|value| FieldOption {
                    value: value.to_owned(),
                    label: value.to_owned(),
                })
                .to_vec(),
        );
        Self::with_fields(vec![
            field("token", FieldKind::Secret, true),
            mode,
            field("chat", FieldKind::Text, false),
        ])
    }

    fn with_fields(fields: Vec<FieldDescriptor>) -> Self {
        Self {
            fields,
            starts: AtomicUsize::new(0),
            stops: AtomicUsize::new(0),
            calls: Mutex::new(Vec::new()),
        }
    }
}

fn tool(name: &str, side_effect: bool) -> IntegrationToolSpec {
    IntegrationToolSpec {
        tool: IntegrationToolDto {
            name: name.to_owned(),
            label: name.to_owned(),
            description: String::new(),
            side_effect,
        },
        input_schema: json!({ "type": "object" }),
    }
}

#[async_trait]
impl IntegrationProvider for FakeProvider {
    fn kind(&self) -> IntegrationKind {
        IntegrationKind::Telegram
    }

    fn label(&self) -> String {
        "Тестовая интеграция".to_owned()
    }

    fn describe(&self) -> Vec<FieldDescriptor> {
        self.fields.clone()
    }

    fn triggers(&self) -> Vec<IntegrationTriggerDto> {
        vec![IntegrationTriggerDto {
            name: EVENT.to_owned(),
            label: "Событие".to_owned(),
            description: String::new(),
        }]
    }

    fn tools(&self) -> Vec<IntegrationToolSpec> {
        vec![tool(READ, false), tool(SEND, true)]
    }

    async fn test(
        &self,
        config: &IntegrationConfigView,
    ) -> Result<Vec<IntegrationCheckDto>, AppError> {
        let token = config.secret("token")?;
        Ok(vec![IntegrationCheckDto {
            label: "API".to_owned(),
            ok: token.expose() == "valid",
            detail: None,
        }])
    }

    async fn start(&self, session: IntegrationSession) -> Result<(), WorkerFault> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        if session.config.text("mode") == Some("fail") {
            return Err(WorkerFault::retry(AppError::new(
                AppErrorCode::ProviderUnreachable,
                OFFLINE,
            )));
        }
        session.worker.ready();
        session.cancellation().cancelled().await;
        Ok(())
    }

    async fn stop(&self, _account: &IntegrationAccountDto) {
        self.stops.fetch_add(1, Ordering::SeqCst);
    }

    async fn call_tool(&self, call: IntegrationToolCall) -> Result<Value, AppError> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((call.account.name.clone(), call.tool.clone()));
        Ok(json!({ "tool": call.tool }))
    }
}

struct Rig {
    host: TestServices,
    provider: Arc<FakeProvider>,
    service: IntegrationService,
    workers: WorkerSupervisor,
}

fn rig() -> Rig {
    let host = test_services(SystemService::new(
        Arc::new(FakeClock::default()),
        EventBus::default(),
        Duration::ZERO,
    ));
    let services = host.shared();
    let provider = Arc::new(FakeProvider::new());
    let workers = WorkerSupervisor::new(host.clock.clone(), silent_logger());
    let service = IntegrationService::new(IntegrationServiceOptions {
        providers: vec![provider.clone()],
        clock: host.clock.clone(),
        backoff: Backoff {
            initial: Duration::from_millis(50),
            max: Duration::from_millis(200),
            stable: Duration::from_secs(30),
        },
        ..IntegrationServiceOptions::new(
            host.database.shared(),
            services.tools.clone(),
            Arc::clone(&host.nodes),
            Arc::clone(&services.secrets),
            workers.clone(),
        )
    })
    .expect("an integration service");
    service.attach_sink(services.scenario_runs.integration_sink());
    Rig {
        host,
        provider,
        service,
        workers,
    }
}

static SECRETS: AtomicUsize = AtomicUsize::new(0);

impl Rig {
    fn secret(&self, value: &str) -> SecretId {
        let name = format!("Токен {}", SECRETS.fetch_add(1, Ordering::SeqCst));
        let id = self
            .host
            .shared()
            .secrets
            .create(SecretCreateInput {
                value: Some(crate::services::crypto::Plaintext::new(value)),
                ..SecretCreateInput::new("custom", name, SecretScope::Personal)
            })
            .expect("a secret")
            .id;
        SecretId::parse(&id).expect("a uuid")
    }

    fn config(&self, mode: &str) -> IntegrationConfig {
        IntegrationConfig::from([
            (
                "token".to_owned(),
                IntegrationValue::secret(self.secret("valid")),
            ),
            ("mode".to_owned(), IntegrationValue::text(mode)),
        ])
    }

    async fn account(&self, name: &str, mode: &str) -> IntegrationAccountDto {
        self.service
            .create(&CreateIntegrationInput {
                config: Some(self.config(mode)),
                ..CreateIntegrationInput::new(IntegrationKind::Telegram, name)
            })
            .await
            .expect("creates")
    }

    fn bind(&self, account: &IntegrationAccountDto) -> zvs_contract::ScenarioDetailDto {
        let services = self.host.shared();
        let node = |id: &str, node_type: &str, params: Value| ScenarioNodeDto {
            id: id.to_owned(),
            node_type: node_type.to_owned(),
            name: None,
            position: ScenarioPositionDto { x: 0.5, y: 0.5 },
            params: params.as_object().cloned().unwrap_or_default(),
            disabled: None,
            settings: None,
            notes: None,
            pinned_input: None,
        };
        let created = services
            .scenarios
            .create(&CreateScenarioInput {
                name: "Входящие".to_owned(),
                tags: None,
                graph: Some(ScenarioGraphDto {
                    nodes: vec![
                        node(
                            "trigger",
                            "trigger.integration",
                            json!({ "accountId": account.id.to_string(), "trigger": EVENT }),
                        ),
                        node("notify", "output.notify", json!({ "title": "Событие" })),
                    ],
                    edges: vec![ScenarioEdgeDto {
                        from: "trigger".to_owned(),
                        to: "notify".to_owned(),
                        from_port: "main".to_owned(),
                        to_port: "main".to_owned(),
                    }],
                    attachments: Vec::new(),
                }),
                note: None,
            })
            .expect("creates the scenario");
        assert_eq!(created.issues, [], "the graph is valid");
        services
            .scenarios
            .set_active(&created.scenario.id, true)
            .expect("activates");
        created
    }

    fn reload(&self, account: &IntegrationAccountDto) -> IntegrationAccountDto {
        self.service
            .account(&account.id.to_string())
            .expect("the account")
    }
}

async fn settle() {
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
}

#[test]
fn a_password_kind_is_not_representable_in_a_field_schema() {
    let parsed = serde_json::from_value::<FieldDescriptor>(json!({
        "key": "password",
        "label": "Пароль",
        "kind": "password",
        "required": true,
    }));
    assert!(parsed.is_err());
}

#[test]
fn a_provider_that_asks_for_a_credential_as_text_is_refused() {
    for key in ["password", "apiToken", "clientSecret", "privateKey"] {
        let provider = FakeProvider::with_fields(vec![field(key, FieldKind::Text, true)]);
        let error = check_provider(&provider).expect_err("a typed credential is refused");
        assert_eq!(error.message, CREDENTIAL_NOT_A_SECRET, "{key}");
    }
    let provider = FakeProvider::with_fields(vec![field("password", FieldKind::Secret, true)]);
    assert!(check_provider(&provider).is_ok());
}

#[tokio::test]
async fn a_credential_value_in_the_config_is_rejected() {
    let rig = rig();
    let typed = rig
        .service
        .create(&CreateIntegrationInput {
            config: Some(IntegrationConfig::from([(
                "token".to_owned(),
                IntegrationValue::text("123456:ABC"),
            )])),
            ..CreateIntegrationInput::new(IntegrationKind::Telegram, "Бот")
        })
        .await
        .expect_err("a raw token is refused");
    assert_eq!(typed.code, AppErrorCode::ValidationFailed);
    assert_eq!(typed.message, CREDENTIAL_NOT_A_SECRET);

    let missing = rig
        .service
        .create(&CreateIntegrationInput {
            config: Some(IntegrationConfig::from([(
                "token".to_owned(),
                IntegrationValue::secret(
                    SecretId::parse("0199aa11-1111-7111-8111-111111111111").expect("uuid"),
                ),
            )])),
            ..CreateIntegrationInput::new(IntegrationKind::Telegram, "Бот")
        })
        .await
        .expect_err("an unknown secret is refused");
    assert_eq!(missing.code, AppErrorCode::NotFound);

    let unknown = rig
        .service
        .create(&CreateIntegrationInput {
            config: Some(IntegrationConfig::from([
                (
                    "token".to_owned(),
                    IntegrationValue::secret(rig.secret("valid")),
                ),
                ("password".to_owned(), IntegrationValue::text("hunter2")),
            ])),
            ..CreateIntegrationInput::new(IntegrationKind::Telegram, "Бот")
        })
        .await
        .expect_err("an undeclared field is refused");
    assert_eq!(unknown.code, AppErrorCode::ValidationFailed);

    let required = rig
        .service
        .create(&CreateIntegrationInput::new(
            IntegrationKind::Telegram,
            "Бот",
        ))
        .await
        .expect_err("the credential is required");
    assert_eq!(required.message, FIELD_REQUIRED);

    let unavailable = rig
        .service
        .create(&CreateIntegrationInput::new(IntegrationKind::Mail, "Почта"))
        .await
        .expect_err("no mail provider is registered");
    assert_eq!(unavailable.message, KIND_UNAVAILABLE);
    assert!(rig.service.list().expect("lists").is_empty());
}

#[tokio::test]
async fn a_secret_reference_is_recorded_as_secret_usage() {
    let rig = rig();
    let account = rig.account("Бот", "steady").await;
    let IntegrationValue::Secret(reference) = &account.config["token"] else {
        panic!("the token is a reference");
    };
    let secret = reference.secret_id.to_string();
    let secrets = &rig.host.shared().secrets;
    let usage = secrets.usage(&secret).expect("usage");
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].kind, SecretConsumerKind::Integration);
    assert_eq!(usage[0].id, account.id.to_string());
    rig.service
        .remove(&account.id.to_string())
        .await
        .expect("removes");
    assert!(secrets.usage(&secret).expect("usage").is_empty());
}

#[tokio::test]
async fn a_trigger_event_starts_a_bound_run_once_per_event_id() {
    let rig = rig();
    let account = rig.account("Бот", "steady").await;
    let scenario = rig.bind(&account);
    let id = account.id.to_string();
    let event = IntegrationEvent::new(EVENT, "update-1", json!({ "text": "Привет" }));

    let first = rig.service.dispatch(&id, &event).expect("dispatches");
    assert_eq!(first, Dispatched::Started(vec!["Входящие".to_owned()]));
    let again = rig.service.dispatch(&id, &event).expect("dispatches");
    assert_eq!(again, Dispatched::Duplicate);

    let services = rig.host.shared();
    let runs = services
        .runs
        .list(&RunListFilter {
            subject_id: Some(scenario.scenario.id.to_string()),
            ..RunListFilter::default()
        })
        .expect("lists runs")
        .items;
    assert_eq!(runs.len(), 1);
    let run = services.runs.get(&runs[0].id).expect("the run");
    assert_eq!(run.input["payload"], json!({ "text": "Привет" }));
    assert_eq!(run.input["eventId"], json!("update-1"));
    assert_eq!(run.input["accountId"], json!(id));

    let counters = rig.reload(&account).counters;
    assert_eq!(counters.events, 1);
    assert_eq!(counters.duplicates, 1);
    assert_eq!(counters.runs, 1);
    let activity = rig
        .service
        .activity(&IntegrationActivityInput {
            id: account.id,
            limit: None,
        })
        .expect("activity");
    assert!(
        activity
            .iter()
            .any(|entry| entry.message == "Запущен сценарий «Входящие»")
    );
}

#[tokio::test]
async fn an_event_for_another_account_or_trigger_starts_nothing() {
    let rig = rig();
    let bound = rig.account("Бот", "steady").await;
    let other = rig.account("Второй бот", "steady").await;
    rig.bind(&bound);
    let started = rig
        .service
        .dispatch(
            &other.id.to_string(),
            &IntegrationEvent::new(EVENT, "update-1", json!({})),
        )
        .expect("dispatches");
    assert_eq!(started, Dispatched::Started(Vec::new()));
    let undeclared = rig
        .service
        .dispatch(
            &bound.id.to_string(),
            &IntegrationEvent::new("fake.unknown", "update-1", json!({})),
        )
        .expect_err("an undeclared trigger is refused");
    assert_eq!(undeclared.message, UNKNOWN_TRIGGER);
}

#[tokio::test]
async fn tools_register_per_account_and_leave_with_it() {
    let rig = rig();
    let first = rig.account("Рабочий", "steady").await;
    let second = rig.account("Личный", "steady").await;
    let tools = &rig.host.shared().tools;
    let detail = rig.service.get(&first.id.to_string()).expect("detail");
    let keys: Vec<String> = detail.tools.iter().map(|tool| tool.key.clone()).collect();
    assert_eq!(keys.len(), 2);
    let second_keys: Vec<String> = rig
        .service
        .get(&second.id.to_string())
        .expect("detail")
        .tools
        .iter()
        .map(|tool| tool.key.clone())
        .collect();
    assert!(keys.iter().all(|key| !second_keys.contains(key)));

    let send_key = account_tool_key(SEND, &first.id.to_string());
    let send = tools
        .find_by_key(&send_key)
        .expect("reads")
        .expect("registered");
    assert_eq!(send.source, ToolSource::Integration);
    assert_eq!(send.source_ref, Some(first.id.to_string()));
    assert_eq!(send.tier, ToolTier::Ask);
    assert!(send.available);
    let read = tools
        .find_by_key(&account_tool_key(READ, &first.id.to_string()))
        .expect("reads")
        .expect("registered");
    assert!(read.read_only);
    let node = rig.host.nodes.resolve(&send_key).expect("a kernel node");
    assert!(node.side_effect());

    rig.service
        .disable(&first.id.to_string())
        .await
        .expect("disables");
    assert!(
        !tools
            .find_by_key(&send_key)
            .expect("reads")
            .expect("kept")
            .available
    );
    assert!(rig.host.nodes.resolve(&send_key).is_err());
    assert!(
        tools
            .find_by_key(&account_tool_key(SEND, &second.id.to_string()))
            .expect("reads")
            .expect("kept")
            .available
    );
}

#[tokio::test]
async fn a_tool_call_reaches_the_provider_with_its_account() {
    let rig = rig();
    let account = rig.account("Рабочий", "steady").await;
    let caller = rig.service.caller();
    let output = caller(
        IntegrationCallTarget {
            account_id: account.id.to_string(),
            tool: SEND.to_owned(),
        },
        serde_json::Map::new(),
        tokio_util::sync::CancellationToken::new(),
    )
    .await
    .expect("calls");
    assert_eq!(output, json!({ "tool": SEND }));
    assert_eq!(
        rig.provider
            .calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone(),
        [("Рабочий".to_owned(), SEND.to_owned())]
    );
    assert_eq!(rig.reload(&account).counters.tool_calls, 1);
}

#[tokio::test(start_paused = true)]
async fn a_failing_account_restarts_without_affecting_the_others() {
    let rig = rig();
    let failing = rig.account("Сбойный", "fail").await;
    let steady = rig.account("Рабочий", "steady").await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    settle().await;

    let failing = rig.reload(&failing);
    assert_ne!(failing.status, IntegrationStatus::Running);
    assert_eq!(failing.status_detail.as_deref(), Some(OFFLINE));
    assert!(failing.counters.errors >= 2, "{:?}", failing.counters);
    assert!(rig.provider.starts.load(Ordering::SeqCst) >= 3);
    assert_eq!(rig.reload(&steady).status, IntegrationStatus::Running);

    let fixed = rig
        .service
        .update(&UpdateIntegrationInput {
            config: Some(rig.config("steady")),
            ..UpdateIntegrationInput::new(failing.id)
        })
        .await
        .expect("updates");
    assert_eq!(fixed.status_detail, None);
    settle().await;
    let recovered = rig.reload(&failing);
    assert_eq!(recovered.status, IntegrationStatus::Running);
    assert_eq!(recovered.status_detail, None);
}

#[tokio::test]
async fn start_enabled_launches_enabled_accounts_and_dispose_stops_them() {
    let rig = rig();
    let enabled = rig.account("Рабочий", "steady").await;
    let disabled = rig.account("Отключённый", "steady").await;
    rig.service
        .disable(&disabled.id.to_string())
        .await
        .expect("disables");
    rig.service.dispose().await;
    assert!(rig.workers.list().is_empty());
    let starts = rig.provider.starts.load(Ordering::SeqCst);

    rig.service.start_enabled().await.expect("starts");
    settle().await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(rig.reload(&enabled).status, IntegrationStatus::Running);
    assert_eq!(rig.reload(&disabled).status, IntegrationStatus::Disabled);
    assert_eq!(rig.provider.starts.load(Ordering::SeqCst), starts + 1);
    assert_eq!(rig.workers.list().len(), 1);

    let stops = rig.provider.stops.load(Ordering::SeqCst);
    rig.service.dispose().await;
    assert!(rig.workers.list().is_empty());
    assert_eq!(rig.provider.stops.load(Ordering::SeqCst), stops + 1);
}

#[tokio::test]
async fn test_reports_the_provider_checks() {
    let rig = rig();
    let account = rig.account("Рабочий", "steady").await;
    let result = rig
        .service
        .test(&account.id.to_string())
        .await
        .expect("tests");
    assert!(result.ok);
    assert_eq!(result.checks.len(), 1);
    assert_eq!(result.error, None);
}
