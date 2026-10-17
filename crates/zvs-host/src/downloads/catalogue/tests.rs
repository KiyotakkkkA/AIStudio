use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::drivers::ai::mock::{MockServer, Reply, direct_http};
use crate::platform::clock::FakeClock;
use crate::platform::logger::silent_logger;
use crate::testing::MemoryLogger;

struct Counting {
    live: bool,
    calls: AtomicUsize,
    items: Mutex<Vec<CatalogueItem>>,
    fail: bool,
}

impl Counting {
    fn new(live: bool, refs: &[&str]) -> Self {
        Self {
            live,
            calls: AtomicUsize::new(0),
            items: Mutex::new(
                refs.iter()
                    .map(|item_ref| {
                        CatalogueItem::new(
                            *item_ref,
                            DownloadItemKind::Model,
                            CatalogueSource::Curated,
                            *item_ref,
                        )
                    })
                    .collect(),
            ),
            fail: false,
        }
    }

    fn shared(live: bool, refs: &[&str]) -> Arc<Self> {
        Arc::new(Self::new(live, refs))
    }
}

#[async_trait]
impl CatalogueProvider for Counting {
    fn id(&self) -> CatalogueSource {
        if self.live {
            CatalogueSource::Ollama
        } else {
            CatalogueSource::Curated
        }
    }

    fn live(&self) -> bool {
        self.live
    }

    async fn list(&self) -> Result<Vec<CatalogueItem>, AppError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(AppError::new(AppErrorCode::ProviderUnreachable, "offline"));
        }
        Ok(lock(&self.items).clone())
    }
}

fn service(providers: Vec<SharedCatalogueProvider>, clock: Arc<FakeClock>) -> CatalogueService {
    CatalogueService::new(CatalogueServiceOptions {
        providers,
        ..CatalogueServiceOptions::new(clock, silent_logger())
    })
}

fn refs(items: &[CatalogueItem]) -> Vec<String> {
    items.iter().map(|item| item.item_ref.clone()).collect()
}

#[tokio::test]
async fn opening_the_page_answers_from_the_shipped_catalogue_and_its_cache() {
    let clock = Arc::new(FakeClock::default());
    let shipped = Counting::shared(false, &["curated:model:a"]);
    let live = Counting::shared(true, &["ollama:model:b"]);
    let catalogue = service(vec![shipped.clone(), live.clone()], clock.clone());

    assert_eq!(refs(&catalogue.list(false).await), ["curated:model:a"]);
    catalogue.list(false).await;
    assert_eq!(shipped.calls.load(Ordering::SeqCst), 1);
    assert_eq!(live.calls.load(Ordering::SeqCst), 0);

    clock.advance(60_000);
    catalogue.list(false).await;
    assert_eq!(shipped.calls.load(Ordering::SeqCst), 2);

    assert_eq!(
        refs(&catalogue.list(true).await),
        ["curated:model:a", "ollama:model:b"]
    );
    assert_eq!(live.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_refresh_that_reaches_nothing_keeps_what_the_page_already_showed() {
    let clock = Arc::new(FakeClock::default());
    let shipped = Counting::shared(false, &["curated:model:a"]);
    let catalogue = service(vec![shipped.clone()], clock);
    catalogue.list(false).await;
    lock(&shipped.items).clear();

    assert_eq!(refs(&catalogue.list(true).await), ["curated:model:a"]);
}

#[tokio::test]
async fn a_failing_source_costs_only_its_own_rows_and_a_later_ref_replaces_an_earlier_one() {
    let logger = MemoryLogger::shared();
    let clock = Arc::new(FakeClock::default());
    let broken = Arc::new(Counting {
        fail: true,
        ..Counting::new(false, &[])
    });
    let first = Counting::shared(false, &["curated:model:a", "curated:model:b"]);
    let second = Counting::shared(false, &["curated:model:a"]);
    "replaced".clone_into(&mut lock(&second.items)[0].display_name);
    let catalogue = CatalogueService::new(CatalogueServiceOptions {
        providers: vec![broken, first, second],
        ..CatalogueServiceOptions::new(clock, logger.clone())
    });

    let items = catalogue.list(false).await;
    assert_eq!(refs(&items), ["curated:model:a", "curated:model:b"]);
    assert_eq!(items[0].display_name, "replaced");
    assert_eq!(logger.matching("A catalogue source failed").len(), 1);
}

#[tokio::test]
async fn an_offered_archive_is_found_without_being_listed() {
    let clock = Arc::new(FakeClock::default());
    let catalogue = service(vec![Arc::new(CuratedProvider)], clock);
    let archive = CatalogueItem::new(
        "runtime:llama-cpp-cpu:b1:a.zip",
        DownloadItemKind::Runtime,
        CatalogueSource::Github,
        "llama-cpp-cpu/a.zip",
    );
    catalogue.offer(archive.clone());

    assert_eq!(
        catalogue
            .find("runtime:llama-cpp-cpu:b1:a.zip")
            .await
            .expect("found"),
        archive
    );
    assert!(
        !refs(&catalogue.list(false).await).contains(&"runtime:llama-cpp-cpu:b1:a.zip".to_owned())
    );
    assert!(catalogue.find("curated:embedding:bge-m3-f16").await.is_ok());
    assert_eq!(
        catalogue
            .find("curated:model:nope")
            .await
            .expect_err("missing")
            .code,
        AppErrorCode::NotFound
    );
}

fn ollama(base_url: Option<String>, logger: SharedLogger) -> OllamaProvider {
    OllamaProvider::new(Arc::new(move || base_url.clone()), direct_http(), logger)
}

#[tokio::test]
async fn ollama_reports_what_it_has_with_digests_and_quantisation() {
    let server = MockServer::start().await;
    server.reply(
        "/api/tags",
        Reply::json(json!({
            "models": [
                {
                    "name": "qwen3-coder:480b",
                    "size": 12345,
                    "digest": "sha256:abc123",
                    "details": { "quantization_level": "Q4_K_M" },
                },
                { "name": "nomic-EMBED-text", "size": 10 },
                { "size": 1 },
            ],
        })),
    );
    let provider = ollama(Some(format!("{}//", server.origin())), silent_logger());

    let installed = provider.installed().await.expect("lists");

    assert_eq!(
        installed,
        [
            InstalledItem {
                kind: DownloadItemKind::Model,
                name: "qwen3-coder:480b".to_owned(),
                version: Some("Q4_K_M".to_owned()),
                size_bytes: 12345,
                digest: Some("abc123".to_owned()),
            },
            InstalledItem {
                kind: DownloadItemKind::Embedding,
                name: "nomic-EMBED-text".to_owned(),
                version: None,
                size_bytes: 10,
                digest: None,
            },
        ]
    );
    assert_eq!(server.paths(), ["/api/tags"]);
    assert!(provider.live());
    assert!(provider.list().await.expect("lists").is_empty());
}

#[tokio::test]
async fn an_unreachable_or_unconfigured_ollama_reports_nothing() {
    let logger = MemoryLogger::shared();
    let server = MockServer::start().await;
    server.reply("/api/tags", Reply::json(json!({})).with_status(500));

    assert!(
        ollama(Some(server.origin()), logger.clone())
            .installed()
            .await
            .expect("answers")
            .is_empty()
    );
    assert_eq!(
        logger
            .matching("Could not list the installed Ollama models")
            .len(),
        1
    );
    assert!(
        ollama(None, logger.clone())
            .installed()
            .await
            .expect("answers")
            .is_empty()
    );
    server.reply("/api/tags", Reply::json(json!({ "models": "nope" })));
    assert!(
        ollama(Some(server.origin()), logger)
            .installed()
            .await
            .expect("answers")
            .is_empty()
    );
}

#[tokio::test]
async fn the_installed_snapshot_is_keyed_by_kind_and_name_and_dropped_on_invalidate() {
    let server = MockServer::start().await;
    server.reply(
        "/api/tags",
        Reply::json(json!({ "models": [{ "name": "bge-m3", "size": 1 }] })),
    );
    let clock = Arc::new(FakeClock::default());
    let catalogue = service(
        vec![Arc::new(ollama(Some(server.origin()), silent_logger()))],
        clock,
    );

    let installed = catalogue.installed(false).await;
    assert!(installed.contains_key(&installed_key(DownloadItemKind::Model, "bge-m3")));
    catalogue.installed(false).await;
    assert_eq!(server.calls().len(), 1);
    catalogue.invalidate();
    catalogue.installed(false).await;
    assert_eq!(server.calls().len(), 2);
}
