use std::sync::Mutex;

use super::*;

#[derive(Default)]
struct FakeRunner {
    calls: Mutex<Vec<String>>,
    delay: Duration,
    fail_fetch: bool,
}

#[async_trait]
impl DashboardRunner for FakeRunner {
    async fn fetch(&self, script: &Path, output: &Path, cwd: &Path) -> Result<(), AppError> {
        assert_eq!(script.parent(), Some(cwd));
        self.calls.lock().expect("a lock").push(format!(
            "fetch {} -> {}",
            script.file_name().expect("a name").to_string_lossy(),
            output.file_name().expect("a name").to_string_lossy()
        ));
        if self.fail_fetch {
            return Err(AppError::new(AppErrorCode::Unknown, FETCH_FAILED));
        }
        Ok(())
    }

    async fn build(&self, _library: &Path, stem: &str) -> Result<(), AppError> {
        tokio::time::sleep(self.delay).await;
        self.calls
            .lock()
            .expect("a lock")
            .push(format!("build {stem}"));
        Ok(())
    }
}

struct Fixture {
    _root: tempfile::TempDir,
    library: PathBuf,
    runner: Arc<FakeRunner>,
    service: DashboardService,
}

const ID: &str = "dashboards/Продажи.html";
const NAME: &str = "Продажи.html";

fn fixture(runner: FakeRunner, meta: &str) -> Fixture {
    let root = tempfile::tempdir().expect("a folder");
    let library = root.path().join("dashboards");
    let folder = library.join(LIVE_DIR).join("Продажи");
    std::fs::create_dir_all(folder.join("fetch")).expect("creates");
    std::fs::write(library.join(NAME), "<p>page</p>").expect("writes");
    std::fs::write(folder.join("meta.json"), meta).expect("writes");
    std::fs::write(folder.join("data.json"), r#"{"title":"Продажи"}"#).expect("writes");
    std::fs::write(folder.join("fetch").join("1-crm.py"), "print(1)\n").expect("writes");
    std::fs::write(folder.join("fetch").join("1-crm.json"), "[]").expect("writes");
    let runner = Arc::new(runner);
    let service = DashboardService::new(library.clone(), runner.clone());
    Fixture {
        _root: root,
        library,
        runner,
        service,
    }
}

const WITH_SCRIPT: &str =
    r#"{"interval":60,"fetch":[{"script":"fetch/1-crm.py","output":"fetch/1-crm.json"}]}"#;
const WITHOUT_SCRIPT: &str = r#"{"interval":0,"fetch":[]}"#;

fn calls(fixture: &Fixture) -> Vec<String> {
    fixture.runner.calls.lock().expect("a lock").clone()
}

fn hashes(status: &DashboardStatusDto) -> Vec<String> {
    status
        .scripts
        .iter()
        .map(|script| script.sha256.clone())
        .collect()
}

#[test]
fn a_page_without_a_saved_spec_is_not_live() {
    let fixture = fixture(FakeRunner::default(), WITHOUT_SCRIPT);
    std::fs::write(fixture.library.join("old.html"), "x").expect("writes");
    let status = fixture
        .service
        .status("dashboards/old.html", "old.html")
        .expect("a status");
    assert!(!status.live);
    assert!(!fixture.service.is_live("old.html"));
    assert!(fixture.service.is_live(NAME));
    assert!(fixture.service.status("dashboards/x", "x").is_err());
}

#[tokio::test]
async fn a_fetch_script_runs_only_after_its_exact_hash_is_approved() {
    let fixture = fixture(FakeRunner::default(), WITH_SCRIPT);
    let status = fixture.service.status(ID, NAME).expect("a status");
    assert!(status.live);
    assert_eq!(status.interval, 60);
    assert_eq!(status.scripts.len(), 1);
    assert_eq!(status.scripts[0].name, "1-crm.py");
    assert_eq!(status.scripts[0].source, "print(1)\n");
    assert!(!status.scripts[0].approved);
    assert!(
        fixture
            .service
            .page_state(ID, NAME)
            .expect("a state")
            .needs_approval
    );

    let refused = fixture
        .service
        .refresh(ID, NAME, Duration::ZERO)
        .await
        .expect_err("not approved");
    assert_eq!(refused.code, AppErrorCode::PermissionDenied);
    assert_eq!(refused.message, NEEDS_APPROVAL);
    assert!(calls(&fixture).is_empty());
    assert_eq!(
        fixture
            .service
            .status(ID, NAME)
            .expect("a status")
            .error
            .as_deref(),
        Some(NEEDS_APPROVAL)
    );

    let stale = fixture
        .service
        .approve(ID, NAME, &["0".repeat(64)])
        .expect_err("another hash");
    assert_eq!(stale.code, AppErrorCode::Conflict);
    let approved = fixture
        .service
        .approve(ID, NAME, &hashes(&status))
        .expect("approves");
    assert!(approved.scripts[0].approved);
    assert_eq!(approved.error, None);

    let refreshed = fixture
        .service
        .refresh(ID, NAME, Duration::ZERO)
        .await
        .expect("refreshes");
    assert_eq!(refreshed.error, None);
    assert!(refreshed.refreshed_at.is_some());
    assert_eq!(
        calls(&fixture),
        ["fetch 1-crm.py -> 1-crm.json", "build Продажи"]
    );

    let script = fixture
        .library
        .join(LIVE_DIR)
        .join("Продажи")
        .join("fetch")
        .join("1-crm.py");
    std::fs::write(&script, "import os\n").expect("writes");
    let changed = fixture
        .service
        .refresh(ID, NAME, Duration::ZERO)
        .await
        .expect_err("the script changed");
    assert_eq!(changed.message, NEEDS_APPROVAL);
    assert_eq!(calls(&fixture).len(), 2);
}

#[tokio::test]
async fn a_failed_fetch_is_kept_as_the_status_error() {
    let fixture = fixture(
        FakeRunner {
            fail_fetch: true,
            ..FakeRunner::default()
        },
        WITH_SCRIPT,
    );
    let status = fixture.service.status(ID, NAME).expect("a status");
    fixture
        .service
        .approve(ID, NAME, &hashes(&status))
        .expect("approves");
    let error = fixture
        .service
        .refresh(ID, NAME, Duration::ZERO)
        .await
        .expect_err("fails");
    assert_eq!(error.message, FETCH_FAILED);
    let status = fixture.service.status(ID, NAME).expect("a status");
    assert_eq!(status.error.as_deref(), Some(FETCH_FAILED));
    assert!(!status.refreshing);
    assert_eq!(calls(&fixture), ["fetch 1-crm.py -> 1-crm.json"]);
}

#[tokio::test]
async fn overlapping_and_recent_refreshes_build_once() {
    let fixture = fixture(
        FakeRunner {
            delay: Duration::from_millis(80),
            ..FakeRunner::default()
        },
        WITHOUT_SCRIPT,
    );
    let first = fixture.service.refresh(ID, NAME, Duration::ZERO);
    let second = fixture.service.refresh(ID, NAME, Duration::ZERO);
    let (first, second) = tokio::join!(first, second);
    first.expect("refreshes");
    second.expect("joins");
    assert_eq!(calls(&fixture), ["build Продажи"]);
    fixture
        .service
        .refresh(ID, NAME, Duration::from_secs(60))
        .await
        .expect("fresh enough");
    assert_eq!(calls(&fixture).len(), 1);
    fixture
        .service
        .refresh(ID, NAME, Duration::ZERO)
        .await
        .expect("refreshes");
    assert_eq!(calls(&fixture).len(), 2);
}

#[test]
fn the_interval_override_wins_until_it_is_reset() {
    let fixture = fixture(FakeRunner::default(), WITH_SCRIPT);
    let status = fixture
        .service
        .set_interval(ID, NAME, Some(300))
        .expect("sets");
    assert_eq!(
        (status.interval, status.spec_interval, status.overridden),
        (300, 60, true)
    );
    let status = fixture
        .service
        .set_interval(ID, NAME, Some(0))
        .expect("sets");
    assert_eq!((status.interval, status.overridden), (0, true));
    assert!(fixture.service.set_interval(ID, NAME, Some(5)).is_err());
    let status = fixture
        .service
        .set_interval(ID, NAME, None)
        .expect("resets");
    assert_eq!((status.interval, status.overridden), (60, false));
    assert!(
        fixture
            .service
            .set_interval("dashboards/old.html", "old.html", Some(60))
            .is_err()
    );
}

#[tokio::test]
async fn a_saved_spec_cannot_point_a_script_outside_its_folder() {
    let fixture = fixture(
        FakeRunner::default(),
        r#"{"interval":0,"fetch":[{"script":"../../../evil.py","output":"fetch/1-crm.json"}]}"#,
    );
    assert!(fixture.service.status(ID, NAME).is_err());
    assert!(
        fixture
            .service
            .refresh(ID, NAME, Duration::ZERO)
            .await
            .is_err()
    );
    assert!(calls(&fixture).is_empty());
}

#[test]
fn forgetting_a_dashboard_removes_its_saved_spec_and_settings() {
    let fixture = fixture(FakeRunner::default(), WITH_SCRIPT);
    let status = fixture.service.status(ID, NAME).expect("a status");
    fixture
        .service
        .approve(ID, NAME, &hashes(&status))
        .expect("approves");
    fixture.service.forget(NAME).expect("forgets");
    assert!(!fixture.library.join(LIVE_DIR).join("Продажи").exists());
    let settings =
        std::fs::read_to_string(fixture.library.join(LIVE_DIR).join(SETTINGS_FILE)).expect("reads");
    assert!(!settings.contains("Продажи"), "{settings}");
    assert!(!fixture.service.is_live(NAME));
}

#[test]
fn page_state_is_what_a_page_needs_and_nothing_more() {
    let fixture = fixture(FakeRunner::default(), WITHOUT_SCRIPT);
    let state = serde_json::to_value(fixture.service.page_state(ID, NAME).expect("a state"))
        .expect("serialises");
    let mut keys: Vec<&str> = state
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["interval", "needsApproval", "refreshedAt", "refreshing"]
    );
    assert_eq!(
        fixture.service.data(NAME).expect("data"),
        r#"{"title":"Продажи"}"#.as_bytes()
    );
}
