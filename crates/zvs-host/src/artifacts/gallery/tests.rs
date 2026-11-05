use std::sync::Mutex;

use super::*;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("creates");
    std::fs::write(path, text).expect("writes");
}

struct Recorded {
    urls: Mutex<Vec<String>>,
    fail: bool,
}

#[async_trait]
impl PageCapture for Recorded {
    async fn capture(&self, url: &str, target: &Path) -> Result<(), AppError> {
        self.urls.lock().expect("a lock").push(url.to_owned());
        if self.fail {
            return Err(AppError::new(AppErrorCode::Unknown, CAPTURE_FAILED));
        }
        std::fs::write(target, b"\x89PNG").expect("writes");
        Ok(())
    }
}

struct NoRunner;

#[async_trait]
impl crate::dashboards::DashboardRunner for NoRunner {
    async fn fetch(&self, _script: &Path, _output: &Path, _cwd: &Path) -> Result<(), AppError> {
        Ok(())
    }

    async fn build(&self, _library: &Path, _stem: &str) -> Result<(), AppError> {
        Ok(())
    }
}

struct Setup {
    root: tempfile::TempDir,
    store: ArtifactStore,
    capture: Arc<Recorded>,
}

fn setup(fail: bool) -> Setup {
    let root = tempfile::tempdir().expect("a folder");
    let dashboards = root.path().join(DASHBOARDS);
    write(&dashboards.join("Продажи.html"), "<p>sales</p>");
    write(
        &dashboards.join(".zvs").join("Продажи").join("meta.json"),
        r#"{"interval":0,"fetch":[]}"#,
    );
    write(
        &dashboards.join(".zvs").join("Продажи").join("data.json"),
        "{}",
    );
    let capture = Arc::new(Recorded {
        urls: Mutex::new(Vec::new()),
        fail,
    });
    let store = ArtifactStore::new(root.path().join("conversations"))
        .with_library(DASHBOARDS, dashboards)
        .with_dashboards(Arc::new(NoRunner))
        .with_capture(root.path().join("cache"), capture.clone());
    Setup {
        root,
        store,
        capture,
    }
}

fn kept(setup: &Setup, owner: &str, name: &str, text: &str) {
    let file = setup
        .root
        .path()
        .join("conversations")
        .join(owner)
        .join("work")
        .join(name);
    write(&file, text);
    setup.store.keep(owner, "r1", &[file]);
}

#[test]
fn files_are_grouped_by_what_they_are() {
    assert_eq!(kind_of(DASHBOARDS, "html"), GalleryKind::Dashboard);
    assert_eq!(kind_of("chat-1", "html"), GalleryKind::Document);
    assert_eq!(kind_of("chat-1", "docx"), GalleryKind::Document);
    assert_eq!(kind_of("chat-1", "csv"), GalleryKind::Table);
    assert_eq!(kind_of("chat-1", "svg"), GalleryKind::Image);
    assert_eq!(kind_of("chat-1", "zip"), GalleryKind::Other);
}

#[test]
fn the_gallery_lists_the_dashboards_library_and_every_chat_output() {
    let setup = setup(false);
    kept(&setup, "chat-1", "Отчёт.pdf", "%PDF");
    kept(&setup, "chat-2", "chart.png", "png");
    kept(&setup, "chat-2", "gone.csv", "a,b");
    std::fs::remove_file(
        setup
            .root
            .path()
            .join("conversations")
            .join("chat-2")
            .join("work")
            .join("gone.csv"),
    )
    .expect("removes");
    write(&setup.root.path().join(DASHBOARDS).join("notes.json"), "{}");
    let listed = setup.store.gallery().items;
    let mut ids: Vec<&str> = listed.iter().map(|item| item.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        [
            "chat-1/Отчёт.pdf",
            "chat-2/chart.png",
            "dashboards/Продажи.html"
        ]
    );
    let dashboard = listed
        .iter()
        .find(|item| item.owner == DASHBOARDS)
        .expect("the dashboard");
    assert_eq!(dashboard.kind, GalleryKind::Dashboard);
    assert!(dashboard.live);
    let image = listed
        .iter()
        .find(|item| item.format == "png")
        .expect("the image");
    assert_eq!(
        (image.kind, image.owner.as_str(), image.live),
        (GalleryKind::Image, "chat-2", false)
    );
    zvs_contract::Validate::validate(&GalleryListDto { items: listed }).expect("a valid list");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_page_preview_is_captured_once_per_version_through_the_loopback_server() {
    let setup = setup(false);
    let first = setup
        .store
        .thumbnail("dashboards/Продажи.html")
        .await
        .expect("a preview");
    assert_eq!(first.media_type.as_deref(), Some("image/png"));
    let again = setup
        .store
        .thumbnail("dashboards/Продажи.html")
        .await
        .expect("a preview");
    assert_eq!(again, first);
    assert_eq!(setup.capture.urls.lock().expect("a lock").len(), 1);
    let url = setup.capture.urls.lock().expect("a lock")[0].clone();
    assert!(url.starts_with("http://127.0.0.1:"), "{url}");
    assert!(url.ends_with("?preview"), "{url}");

    let page = setup.root.path().join(DASHBOARDS).join("Продажи.html");
    std::thread::sleep(Duration::from_millis(20));
    write(&page, "<p>refreshed sales</p>");
    setup
        .store
        .thumbnail("dashboards/Продажи.html")
        .await
        .expect("a preview");
    assert_eq!(setup.capture.urls.lock().expect("a lock").len(), 2);
    let cached: Vec<_> = std::fs::read_dir(setup.root.path().join("cache"))
        .expect("a cache")
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".png"))
        .collect();
    assert_eq!(cached.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn files_preview_as_themselves_and_failures_preview_as_nothing() {
    let setup = setup(true);
    kept(&setup, "chat-1", "chart.png", "png");
    kept(&setup, "chat-1", "data.csv", "a,b");
    let image = setup
        .store
        .thumbnail("chat-1/chart.png")
        .await
        .expect("a preview");
    assert_eq!(image.media_type.as_deref(), Some("image/png"));
    assert_eq!(image.data.as_deref(), Some(STANDARD.encode("png").as_str()));
    assert_eq!(
        setup
            .store
            .thumbnail("chat-1/data.csv")
            .await
            .expect("no preview"),
        GalleryThumbnailDto::default()
    );
    assert_eq!(
        setup
            .store
            .thumbnail("dashboards/Продажи.html")
            .await
            .expect("no preview"),
        GalleryThumbnailDto::default()
    );
    assert!(setup.store.thumbnail("../escape.png").await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_a_dashboard_drops_its_page_saved_spec_and_preview() {
    let setup = setup(false);
    setup
        .store
        .thumbnail("dashboards/Продажи.html")
        .await
        .expect("a preview");
    setup
        .store
        .remove_artifact("dashboards/Продажи.html")
        .expect("removes");
    let dashboards = setup.root.path().join(DASHBOARDS);
    assert!(!dashboards.join("Продажи.html").exists());
    assert!(!dashboards.join(".zvs").join("Продажи").exists());
    let previews = std::fs::read_dir(setup.root.path().join("cache"))
        .expect("a cache")
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".png"))
        .count();
    assert_eq!(previews, 0);
    let missing = setup
        .store
        .remove_artifact("dashboards/Продажи.html")
        .expect_err("already gone");
    assert_eq!(missing.code, AppErrorCode::NotFound);
    assert_eq!(missing.message, crate::artifacts::ARTIFACT_NOT_FOUND);
}

#[test]
fn removing_a_chat_output_keeps_its_record_so_the_chat_shows_it_was_removed() {
    let setup = setup(false);
    kept(&setup, "chat-1", "Отчёт.pdf", "%PDF");
    setup
        .store
        .remove_artifact("chat-1/Отчёт.pdf")
        .expect("removes");
    assert!(
        setup
            .store
            .gallery()
            .items
            .iter()
            .all(|item| item.owner != "chat-1")
    );
    let error = setup.store.resolve("chat-1/Отчёт.pdf").expect_err("gone");
    assert_eq!(error.message, crate::artifacts::ARTIFACT_NOT_FOUND);
}
