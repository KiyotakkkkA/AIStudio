use serde_json::json;

use super::*;
use crate::drivers::ai::mock::{MockServer, Reply, direct_http};
use crate::platform::clock::FakeClock;
use crate::platform::logger::silent_logger;
use crate::runtimes::definitions::definition_of;
use crate::testing::MemoryLogger;

const TAG: &str = "b10970";
const RECORDED: &str = include_str!("../fixtures/llama-releases-b11696.json");

fn release_payload() -> Value {
    let asset = |name: &str, size: u64| {
        json!({
            "name": name,
            "size": size,
            "state": "uploaded",
            "browser_download_url": format!("https://github.test/{name}"),
            "digest": format!("sha256:{}", "a".repeat(64)),
        })
    };
    json!([
        { "tag_name": "v0.4.1", "draft": false, "prerelease": false, "assets": [asset("nightly-tag.txt", 7)] },
        {
            "tag_name": TAG,
            "draft": false,
            "prerelease": true,
            "assets": [
                asset(&format!("llama-{TAG}-bin-win-cpu-x64.zip"), 18_428_751),
                asset(&format!("llama-{TAG}-bin-win-vulkan-x64.zip"), 31_675_940),
                asset(&format!("llama-{TAG}-bin-win-cuda-13.3-x64.zip"), 149_722_826),
                asset("cudart-llama-bin-win-cuda-13.3-x64.zip", 390_970_417),
                asset(&format!("llama-{TAG}-bin-ubuntu-vulkan-x64.tar.gz"), 30_188_817),
                asset(&format!("llama-{TAG}-bin-macos-arm64.tar.gz"), 11_149_707),
            ],
        },
    ])
}

struct Setup {
    server: MockServer,
    resolver: ReleaseResolver,
    clock: Arc<FakeClock>,
}

async fn resolver_over(payload: Value, platform: Platform, logger: SharedLogger) -> Setup {
    let server = MockServer::start().await;
    server.reply("/releases", Reply::json(payload));
    let clock = Arc::new(FakeClock::default());
    let resolver = ReleaseResolver::new(ReleaseResolverOptions {
        url: server.url("/releases"),
        platform,
        ..ReleaseResolverOptions::new(direct_http(), clock.clone(), logger)
    });
    Setup {
        server,
        resolver,
        clock,
    }
}

const WINDOWS: Platform = Platform::new("win32", "x64");

fn names(build: &ResolvedBuild) -> Vec<&str> {
    build
        .assets
        .iter()
        .map(|asset| asset.name.as_str())
        .collect()
}

async fn resolve(setup: &Setup, id: &str) -> Option<ResolvedBuild> {
    setup
        .resolver
        .resolve(definition_of(id).expect("a definition"), false)
        .await
        .expect("resolves")
}

#[tokio::test]
async fn the_resolver_picks_the_newest_build_tag_and_skips_the_nightly_pointer_release() {
    let setup = resolver_over(release_payload(), WINDOWS, silent_logger()).await;

    let build = resolve(&setup, "llama-cpp-vulkan").await.expect("a build");

    assert_eq!(build.tag, TAG);
    assert_eq!(build.runtime_id, "llama-cpp-vulkan");
    assert_eq!(
        names(&build),
        [format!("llama-{TAG}-bin-win-vulkan-x64.zip")]
    );
    let checksum = build.assets[0].checksum.as_ref().expect("a checksum");
    assert_eq!(checksum.algorithm, ChecksumAlgorithm::Sha256);
    assert_eq!(build.assets[0].size_bytes, 31_675_940);
}

#[tokio::test]
async fn a_cuda_build_brings_its_runtime_libraries_with_it() {
    let setup = resolver_over(release_payload(), WINDOWS, silent_logger()).await;

    let build = resolve(&setup, "llama-cpp-cuda").await.expect("a build");

    assert_eq!(
        names(&build),
        [
            format!("llama-{TAG}-bin-win-cuda-13.3-x64.zip"),
            "cudart-llama-bin-win-cuda-13.3-x64.zip".to_owned(),
        ]
    );
}

#[tokio::test]
async fn a_cuda_build_whose_runtime_libraries_are_missing_is_not_offered_at_all() {
    let mut payload = release_payload();
    payload[1]["assets"]
        .as_array_mut()
        .expect("assets")
        .retain(|asset| {
            !asset["name"]
                .as_str()
                .unwrap_or_default()
                .starts_with("cudart-")
        });
    let setup = resolver_over(payload, WINDOWS, silent_logger()).await;

    assert_eq!(resolve(&setup, "llama-cpp-cuda").await, None);
}

#[tokio::test]
async fn a_build_that_does_not_exist_for_this_platform_resolves_to_nothing_without_a_request() {
    let setup = resolver_over(
        release_payload(),
        Platform::new("darwin", "arm64"),
        silent_logger(),
    )
    .await;

    assert_eq!(resolve(&setup, "llama-cpp-cuda").await, None);
    assert!(setup.server.calls().is_empty());
    assert!(!setup.resolver.cached_releases());
}

#[tokio::test]
async fn the_release_list_is_read_once_and_reused_until_it_goes_stale() {
    let setup = resolver_over(release_payload(), WINDOWS, silent_logger()).await;

    resolve(&setup, "llama-cpp-cpu").await;
    resolve(&setup, "llama-cpp-vulkan").await;
    assert_eq!(setup.server.calls().len(), 1);
    assert_eq!(
        setup.server.last().header("accept").as_deref(),
        Some("application/vnd.github+json")
    );

    setup.clock.advance(6 * 60 * 60 * 1000);
    resolve(&setup, "llama-cpp-cpu").await;
    assert_eq!(setup.server.calls().len(), 2);

    setup
        .resolver
        .resolve(definition_of("llama-cpp-cpu").expect("a definition"), true)
        .await
        .expect("resolves");
    assert_eq!(setup.server.calls().len(), 3);
}

#[tokio::test]
async fn a_rate_limited_github_is_reported_as_such_rather_than_as_a_missing_build() {
    let setup = resolver_over(json!({}), WINDOWS, silent_logger()).await;
    setup
        .server
        .reply("/releases", Reply::json(json!({})).with_status(403));

    let error = setup
        .resolver
        .resolve(definition_of("llama-cpp-cpu").expect("a definition"), false)
        .await
        .expect_err("refuses");
    assert_eq!(error.code, AppErrorCode::RateLimited);
    assert_eq!(error.message, RATE_LIMITED_MESSAGE);

    setup
        .server
        .reply("/releases", Reply::json(json!({})).with_status(502));
    let error = setup
        .resolver
        .resolve(definition_of("llama-cpp-cpu").expect("a definition"), false)
        .await
        .expect_err("refuses");
    assert_eq!(error.code, AppErrorCode::ProviderUnreachable);
    assert!(error.message.contains("502"));

    setup.server.reply("/releases", Reply::json(json!({})));
    let error = setup
        .resolver
        .resolve(definition_of("llama-cpp-cpu").expect("a definition"), false)
        .await
        .expect_err("refuses");
    assert_eq!(error.code, AppErrorCode::ValidationFailed);
}

#[tokio::test]
async fn a_refresh_that_cannot_reach_github_keeps_what_was_already_known() {
    let logger = MemoryLogger::shared();
    let setup = resolver_over(release_payload(), WINDOWS, logger.clone()).await;
    assert!(resolve(&setup, "llama-cpp-cpu").await.is_some());
    setup
        .server
        .reply("/releases", Reply::json(json!({})).with_status(500));

    let build = setup
        .resolver
        .resolve(definition_of("llama-cpp-cpu").expect("a definition"), true)
        .await
        .expect("falls back");

    assert_eq!(build.map(|build| build.tag), Some(TAG.to_owned()));
    assert_eq!(logger.matching("Kept the cached release list").len(), 1);
}

#[tokio::test]
async fn a_draft_release_and_an_asset_still_uploading_are_skipped() {
    let mut payload = release_payload();
    let mut draft = payload[1].clone();
    draft["tag_name"] = json!("b20000");
    draft["draft"] = json!(true);
    payload.as_array_mut().expect("releases").insert(0, draft);
    let mut uploading = payload[2].clone();
    uploading["tag_name"] = json!("b19999");
    for asset in uploading["assets"].as_array_mut().expect("assets") {
        asset["state"] = json!("starter");
    }
    payload
        .as_array_mut()
        .expect("releases")
        .insert(1, uploading);
    payload[3]["assets"][0]["digest"] = json!("md5:abc");
    let setup = resolver_over(payload, WINDOWS, silent_logger()).await;

    let build = resolve(&setup, "llama-cpp-cpu").await.expect("a build");

    assert_eq!(build.tag, TAG);
    assert_eq!(build.assets[0].checksum, None);
}

#[tokio::test]
async fn the_recorded_github_answer_resolves_every_build_this_app_offers() {
    let recorded: Value = serde_json::from_str(RECORDED).expect("the fixture parses");
    let windows = resolver_over(recorded.clone(), WINDOWS, silent_logger()).await;
    let linux = resolver_over(
        recorded.clone(),
        Platform::new("linux", "x64"),
        silent_logger(),
    )
    .await;
    let mac = resolver_over(
        recorded.clone(),
        Platform::new("darwin", "arm64"),
        silent_logger(),
    )
    .await;
    let arm = resolver_over(recorded, Platform::new("win32", "arm64"), silent_logger()).await;

    let vulkan = resolve(&windows, "llama-cpp-vulkan")
        .await
        .expect("a build");
    assert_eq!(vulkan.tag, "b11696");
    assert_eq!(names(&vulkan), ["llama-b11696-bin-win-vulkan-x64.zip"]);
    assert_eq!(
        vulkan.assets[0].url,
        "https://github.com/ggml-org/llama.cpp/releases/download/b11696/llama-b11696-bin-win-vulkan-x64.zip"
    );
    assert!(vulkan.assets[0].checksum.is_some());
    assert!(vulkan.assets[0].size_bytes > 0);
    assert_eq!(
        names(&resolve(&windows, "llama-cpp-cpu").await.expect("a build")),
        ["llama-b11696-bin-win-cpu-x64.zip"]
    );
    assert_eq!(
        names(&resolve(&windows, "llama-cpp-cuda").await.expect("a build")),
        [
            "llama-b11696-bin-win-cuda-12.4-x64.zip",
            "cudart-llama-bin-win-cuda-12.4-x64.zip",
        ]
    );
    assert_eq!(resolve(&windows, "llama-cpp-metal").await, None);

    assert_eq!(
        names(&resolve(&linux, "llama-cpp-cuda").await.expect("a build")),
        [
            "llama-b11696-bin-ubuntu-cuda-12.8-x64.tar.gz",
            "cudart-llama-b11696-bin-ubuntu-cuda-12.8-x64.tar.gz",
        ]
    );
    assert_eq!(
        names(&resolve(&linux, "llama-cpp-cpu").await.expect("a build")),
        ["llama-b11696-bin-ubuntu-x64.tar.gz"]
    );
    assert_eq!(
        names(&resolve(&mac, "llama-cpp-metal").await.expect("a build")),
        ["llama-b11696-bin-macos-arm64.tar.gz"]
    );
    assert_eq!(
        names(&resolve(&arm, "llama-cpp-cuda").await.expect("a build")),
        [
            "llama-b11696-bin-win-cuda-13.4-arm64.zip",
            "cudart-llama-bin-win-cuda-13.4-arm64.zip",
        ]
    );
}
