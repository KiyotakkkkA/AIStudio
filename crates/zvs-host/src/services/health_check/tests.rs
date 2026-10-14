use serde_json::json;
use zvs_contract::{CreateProviderInput, HostEvent, ProviderId, UpdateProviderInput};

use super::*;
use crate::drivers::ai::ports::DiscoveredModel;
use crate::platform::clock::FakeClock;
use crate::platform::logger::silent_logger;
use crate::services::crypto::CryptoService;
use crate::services::providers::ProviderServiceOptions;
use crate::services::secrets::SecretService;
use crate::testing::{
    FakeCrypto, FakeDriver, FakeDriverSource, FakeScript, MemoryLogger, MemorySink,
    TemporaryDatabase, temporary_database,
};

struct Fixture {
    _database: TemporaryDatabase,
    driver: Arc<FakeDriver>,
    providers: ProviderService,
    settings: SettingService,
    events: EventBus,
    sink: Arc<MemorySink>,
    logger: Arc<MemoryLogger>,
    health: HealthCheckService,
}

impl Fixture {
    fn new() -> Self {
        let database = temporary_database();
        let clock = Arc::new(FakeClock::default());
        let driver = FakeDriver::with_models(vec![DiscoveredModel::named("model-1")]);
        let secrets = Arc::new(SecretService::new(
            database.shared(),
            Arc::new(CryptoService::new(FakeCrypto::shared())),
            clock.clone(),
            silent_logger(),
        ));
        let providers = ProviderService::new(ProviderServiceOptions::new(
            database.shared(),
            FakeDriverSource::shared(driver.clone()),
            secrets,
            silent_logger(),
            clock.clone(),
        ));
        let settings = SettingService::new(database.shared(), clock.clone());
        let events = EventBus::default();
        let sink = MemorySink::shared();
        events.attach(sink.clone());
        let logger = MemoryLogger::shared();
        let health = HealthCheckService::new(HealthCheckOptions {
            providers: providers.clone(),
            settings: settings.clone(),
            logger: logger.clone(),
            clock,
            events: Some(events.clone()),
            runtime: None,
        });
        Self {
            _database: database,
            driver,
            providers,
            settings,
            events,
            sink,
            logger,
            health,
        }
    }

    fn set(&self, key: &str, value: Value) {
        self.settings.set(key, &value).expect("stores the setting");
        if is_health_check_key(key) {
            self.health.stop();
            self.health.start();
        }
    }

    fn create(&self, name: &str) -> ProviderId {
        let input: CreateProviderInput = zvs_contract::parse(json!({
            "kind": "ollama",
            "name": name,
            "baseUrl": "http://localhost:11434",
            "capabilities": ["text"],
        }))
        .expect("a valid input");
        self.providers.create(&input).expect("creates").summary.id
    }

    fn progress(&self) -> Vec<(u64, u64)> {
        self.sink
            .events()
            .into_iter()
            .filter_map(|event: HostEvent| match event.body {
                HostEventDraft::Progress { done, total } => Some((done, total)),
                _ => None,
            })
            .collect()
    }

    fn outcomes(&self) -> Vec<RunOutcomeStatus> {
        self.sink
            .events()
            .into_iter()
            .filter_map(|event| match event.body {
                HostEventDraft::End { outcome } => Some(outcome.status),
                _ => None,
            })
            .collect()
    }
}

#[tokio::test]
async fn health_checking_is_opt_in_configurable_skips_busy_providers_and_stops_pending_work() {
    let fixture = Fixture::new();
    let health = &fixture.health;
    assert!(!health.start());
    assert_eq!(
        fixture.logger.matching("Background health check stays off")[0].fields["setting"],
        json!(HEALTH_CHECK_ENABLED_KEY)
    );
    health.run_once().await;
    assert_eq!(health.interval_minutes(), 15);
    fixture.set(HEALTH_CHECK_INTERVAL_KEY, json!(2));
    fixture.set(HEALTH_CHECK_ENABLED_KEY, json!(true));
    assert!(health.running());
    assert_eq!(health.interval_minutes(), 2);

    let row = fixture.create("Local").to_string();
    let second = fixture.create("Second");
    fixture.driver.script(FakeScript {
        manual: Some(true),
        ..FakeScript::default()
    });
    let manual = fixture
        .providers
        .probe(ProbeInput::Saved(row.clone()), None);
    fixture.driver.when_holding().await;
    let mut disabled = UpdateProviderInput::new(second);
    disabled.enabled = Some(false);
    fixture.providers.update(&disabled).expect("disables");
    health.run_once().await;
    assert!(fixture.providers.is_probing(&row));
    fixture.driver.release();
    manual.await.expect("the manual probe finishes");

    let pending = health.run_once();
    fixture.driver.when_holding().await;
    health.stop();
    pending.await;
    health.dispose().await;
    assert!(!health.running());
    assert_eq!(fixture.events.open(), 0);
    assert!(!fixture.providers.is_probing(&row));
    assert_eq!(fixture.outcomes(), [RunOutcomeStatus::Failed]);
    assert_eq!(
        fixture.logger.matching("Health check sweep stopped early")[0].fields["failed"],
        json!(true)
    );
    assert_eq!(
        fixture
            .logger
            .matching("Stopped the background health check")
            .len(),
        1
    );
}

#[tokio::test(start_paused = true)]
async fn the_interval_drives_sweeps_that_report_progress_until_the_check_is_switched_off() {
    let fixture = Fixture::new();
    let first = fixture.create("First").to_string();
    fixture.create("Second");
    fixture.set(HEALTH_CHECK_INTERVAL_KEY, json!(1.4));
    fixture.set(HEALTH_CHECK_ENABLED_KEY, json!(true));
    assert_eq!(fixture.health.interval_minutes(), 1);
    assert_eq!(
        fixture
            .logger
            .matching("Started the background health check")[0]
            .fields["minutes"],
        json!(1)
    );

    tokio::time::sleep(Duration::from_secs(59)).await;
    assert_eq!(fixture.driver.list_calls(), 0);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(fixture.driver.list_calls(), 2);
    assert_eq!(fixture.progress(), [(1, 2), (2, 2)]);
    assert_eq!(fixture.outcomes(), [RunOutcomeStatus::Ok]);
    assert_eq!(
        fixture
            .providers
            .get(&first, false)
            .expect("reads")
            .summary
            .status,
        zvs_contract::ProviderStatus::Ok
    );

    tokio::time::sleep(Duration::from_secs(60)).await;
    assert_eq!(fixture.driver.list_calls(), 4);

    fixture.set(HEALTH_CHECK_ENABLED_KEY, json!(false));
    assert!(!fixture.health.running());
    tokio::time::sleep(Duration::from_secs(30 * 60)).await;
    assert_eq!(fixture.driver.list_calls(), 4);
}

#[tokio::test]
async fn the_interval_setting_falls_back_on_bad_values_and_never_drops_below_a_minute() {
    let fixture = Fixture::new();
    for (stored, minutes) in [
        (json!("ten"), 15),
        (json!(null), 15),
        (json!(0), 1),
        (json!(-3), 1),
        (json!(2.5), 3),
        (json!(45), 45),
    ] {
        fixture.set(HEALTH_CHECK_INTERVAL_KEY, stored);
        assert_eq!(fixture.health.interval_minutes(), minutes);
    }
    fixture.set(HEALTH_CHECK_ENABLED_KEY, json!("yes"));
    assert!(!fixture.health.enabled());
    assert!(!fixture.health.running());
}

#[tokio::test]
async fn concurrent_sweeps_join_and_a_disabled_check_does_nothing() {
    let fixture = Fixture::new();
    fixture.create("Only");
    fixture.health.run_once().await;
    assert_eq!(fixture.driver.list_calls(), 0);
    fixture
        .settings
        .set(HEALTH_CHECK_ENABLED_KEY, &json!(true))
        .expect("enables");
    let (first, second) = (fixture.health.run_once(), fixture.health.run_once());
    futures::join!(first, second);
    assert_eq!(fixture.driver.list_calls(), 1);
    assert!(!fixture.health.running());
    assert_eq!(
        fixture.logger.matching("Health check sweep finished")[0].fields["done"],
        json!(1)
    );
}
