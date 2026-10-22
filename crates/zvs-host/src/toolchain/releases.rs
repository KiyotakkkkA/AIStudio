use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde_json::{Value, json};
use zvs_contract::{
    AppError, AppErrorCode, ChecksumAlgorithm, ChecksumDto, Timestamp, ToolchainRuntime,
};

use super::definitions::{Version, node_asset_name, node_file_key, uv_asset_name};
use crate::drivers::ai::HttpClient;
use crate::platform::clock::SharedClock;
use crate::platform::logger::{LogLevel, SharedLogger};
use crate::runtimes::Platform;
use crate::runtimes::releases::RATE_LIMITED_MESSAGE;

pub const NODE_INDEX_URL: &str = "https://nodejs.org/dist/index.json";
pub const NODE_DIST_URL: &str = "https://nodejs.org/dist";
pub const UV_RELEASES_URL: &str = "https://api.github.com/repos/astral-sh/uv/releases?per_page=10";
pub const DEFAULT_LATEST_TTL: Duration = Duration::from_secs(6 * 60 * 60);
pub const NO_BUILD: &str = "Для этой системы нет подходящей сборки";
pub const UNEXPECTED_INDEX: &str = "Сервер выпусков вернул неожиданный ответ";
pub const NO_CHECKSUM: &str = "Для сборки не найдена контрольная сумма";
pub const CHECKSUM_MISMATCH: &str = "Контрольные суммы сборки не совпадают";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainBuild {
    pub runtime: ToolchainRuntime,
    pub version: Version,
    pub asset_name: String,
    pub url: String,
    pub size_bytes: u64,
    pub checksum: ChecksumDto,
}

#[derive(Clone)]
pub struct ToolchainResolverOptions {
    pub http: HttpClient,
    pub node_index_url: String,
    pub node_dist_url: String,
    pub uv_releases_url: String,
    pub platform: Platform,
    pub ttl: Duration,
    pub clock: SharedClock,
    pub logger: SharedLogger,
}

impl ToolchainResolverOptions {
    pub fn new(http: HttpClient, clock: SharedClock, logger: SharedLogger) -> Self {
        Self {
            http,
            node_index_url: NODE_INDEX_URL.to_owned(),
            node_dist_url: NODE_DIST_URL.to_owned(),
            uv_releases_url: UV_RELEASES_URL.to_owned(),
            platform: Platform::current(),
            ttl: DEFAULT_LATEST_TTL,
            clock,
            logger,
        }
    }
}

struct Cached {
    build: ToolchainBuild,
    at: Timestamp,
}

struct Inner {
    options: ToolchainResolverOptions,
    cached: Mutex<HashMap<ToolchainRuntime, Cached>>,
}

#[derive(Clone)]
pub struct ToolchainResolver {
    inner: Arc<Inner>,
}

fn unreachable(error: &reqwest::Error) -> AppError {
    AppError::new(AppErrorCode::ProviderUnreachable, error.to_string())
}

fn unexpected() -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, UNEXPECTED_INDEX)
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256(value: &str) -> ChecksumDto {
    ChecksumDto {
        algorithm: ChecksumAlgorithm::Sha256,
        value: value.to_ascii_lowercase(),
    }
}

pub fn checksum_line(listing: &str, asset_name: &str) -> Option<String> {
    listing.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == asset_name && is_sha256(hash)).then(|| hash.to_ascii_lowercase())
    })
}

pub fn checksum_file(text: &str) -> Option<String> {
    text.split_whitespace()
        .next()
        .filter(|hash| is_sha256(hash))
        .map(str::to_ascii_lowercase)
}

pub fn newest_lts(index: &Value, file_key: &str) -> Option<String> {
    let mut candidates: Vec<(Version, String)> = index
        .as_array()?
        .iter()
        .filter(|release| release.get("lts").is_some_and(Value::is_string))
        .filter(|release| {
            release
                .get("files")
                .and_then(Value::as_array)
                .is_some_and(|files| files.iter().any(|file| file.as_str() == Some(file_key)))
        })
        .filter_map(|release| {
            let tag = release.get("version")?.as_str()?;
            Some((Version::parse(tag)?, tag.to_owned()))
        })
        .collect();
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.0));
    candidates.into_iter().next().map(|(_, tag)| tag)
}

struct GithubAsset {
    url: String,
    size_bytes: u64,
    digest: Option<String>,
}

fn github_assets(release: &Value) -> HashMap<String, GithubAsset> {
    release
        .get("assets")
        .and_then(Value::as_array)
        .map(|assets| {
            assets
                .iter()
                .filter_map(|asset| {
                    let name = asset.get("name")?.as_str()?.to_owned();
                    let url = asset.get("browser_download_url")?.as_str()?.to_owned();
                    let size_bytes = asset
                        .get("size")
                        .and_then(Value::as_u64)
                        .unwrap_or_default();
                    let digest = asset
                        .get("digest")
                        .and_then(Value::as_str)
                        .and_then(|digest| digest.strip_prefix("sha256:"))
                        .filter(|digest| is_sha256(digest))
                        .map(str::to_ascii_lowercase);
                    Some((
                        name,
                        GithubAsset {
                            url,
                            size_bytes,
                            digest,
                        },
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

impl ToolchainResolver {
    pub fn new(options: ToolchainResolverOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                cached: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn platform(&self) -> Platform {
        self.inner.options.platform
    }

    fn cached(&self) -> MutexGuard<'_, HashMap<ToolchainRuntime, Cached>> {
        self.inner
            .cached
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn fresh(&self, runtime: ToolchainRuntime) -> Option<ToolchainBuild> {
        let ttl = i64::try_from(self.inner.options.ttl.as_millis()).unwrap_or(i64::MAX);
        let now = self.inner.options.clock.now();
        self.cached()
            .get(&runtime)
            .filter(|cached| now - cached.at < ttl)
            .map(|cached| cached.build.clone())
    }

    pub fn known_latest(&self, runtime: ToolchainRuntime) -> Option<Version> {
        self.fresh(runtime).map(|build| build.version)
    }

    pub async fn latest(
        &self,
        runtime: ToolchainRuntime,
        refresh: bool,
    ) -> Result<ToolchainBuild, AppError> {
        if !refresh && let Some(build) = self.fresh(runtime) {
            return Ok(build);
        }
        let build = match runtime {
            ToolchainRuntime::Node => self.node().await?,
            ToolchainRuntime::Uv => self.uv().await?,
        };
        self.inner.options.logger.log(
            LogLevel::Info,
            "toolchain",
            "Resolved the latest runtime build",
            &[
                ("runtime", json!(runtime.as_str())),
                ("version", json!(build.version.to_string())),
                ("asset", json!(build.asset_name)),
            ],
        );
        self.cached().insert(
            runtime,
            Cached {
                build: build.clone(),
                at: self.inner.options.clock.now(),
            },
        );
        Ok(build)
    }

    async fn get(&self, url: &str, github: bool) -> Result<reqwest::Response, AppError> {
        let mut request = self.inner.options.http.client().get(url);
        if github {
            request = request.header("accept", "application/vnd.github+json");
        }
        let response = request.send().await.map_err(|error| unreachable(&error))?;
        let status = response.status().as_u16();
        if response.status().is_success() {
            return Ok(response);
        }
        Err(if github && (status == 403 || status == 429) {
            AppError::new(AppErrorCode::RateLimited, RATE_LIMITED_MESSAGE)
        } else {
            AppError::new(
                AppErrorCode::ProviderUnreachable,
                format!("Сервер выпусков ответил {status}"),
            )
            .with_detail("url", url)
        })
    }

    async fn json(&self, url: &str, github: bool) -> Result<Value, AppError> {
        let body = self
            .get(url, github)
            .await?
            .bytes()
            .await
            .map_err(|error| unreachable(&error))?;
        serde_json::from_slice(&body).map_err(|_| unexpected())
    }

    async fn text(&self, url: &str) -> Result<String, AppError> {
        self.get(url, false)
            .await?
            .text()
            .await
            .map_err(|error| unreachable(&error))
    }

    fn no_build(&self, runtime: ToolchainRuntime) -> AppError {
        let platform = self.platform();
        AppError::new(AppErrorCode::UnsupportedFormat, NO_BUILD)
            .with_detail("runtime", runtime.as_str())
            .with_detail("platform", platform.os)
            .with_detail("arch", platform.arch)
    }

    async fn node(&self) -> Result<ToolchainBuild, AppError> {
        let options = &self.inner.options;
        let platform = options.platform;
        let file_key =
            node_file_key(platform).ok_or_else(|| self.no_build(ToolchainRuntime::Node))?;
        let index = self.json(&options.node_index_url, false).await?;
        if !index.is_array() {
            return Err(unexpected());
        }
        let tag =
            newest_lts(&index, &file_key).ok_or_else(|| self.no_build(ToolchainRuntime::Node))?;
        let version = Version::parse(&tag).ok_or_else(unexpected)?;
        let asset_name =
            node_asset_name(&tag, platform).ok_or_else(|| self.no_build(ToolchainRuntime::Node))?;
        let base = options.node_dist_url.trim_end_matches('/');
        let listing = self.text(&format!("{base}/{tag}/SHASUMS256.txt")).await?;
        let checksum = checksum_line(&listing, &asset_name).ok_or_else(|| {
            AppError::new(AppErrorCode::NotFound, NO_CHECKSUM)
                .with_detail("asset", asset_name.clone())
        })?;
        Ok(ToolchainBuild {
            runtime: ToolchainRuntime::Node,
            version,
            url: format!("{base}/{tag}/{asset_name}"),
            asset_name,
            size_bytes: 0,
            checksum: sha256(&checksum),
        })
    }

    async fn uv(&self) -> Result<ToolchainBuild, AppError> {
        let options = &self.inner.options;
        let asset_name =
            uv_asset_name(options.platform).ok_or_else(|| self.no_build(ToolchainRuntime::Uv))?;
        let releases = self.json(&options.uv_releases_url, true).await?;
        let releases = releases.as_array().ok_or_else(unexpected)?;
        let mut newest: Option<(Version, HashMap<String, GithubAsset>)> = None;
        for release in releases {
            if release.get("draft") == Some(&Value::Bool(true))
                || release.get("prerelease") == Some(&Value::Bool(true))
            {
                continue;
            }
            let Some(version) = release
                .get("tag_name")
                .and_then(Value::as_str)
                .and_then(Version::parse)
            else {
                continue;
            };
            let assets = github_assets(release);
            if !assets.contains_key(asset_name) {
                continue;
            }
            if newest.as_ref().is_none_or(|(best, _)| version > *best) {
                newest = Some((version, assets));
            }
        }
        let (version, assets) = newest.ok_or_else(|| self.no_build(ToolchainRuntime::Uv))?;
        let archive = assets
            .get(asset_name)
            .ok_or_else(|| self.no_build(ToolchainRuntime::Uv))?;
        let sidecar = assets.get(&format!("{asset_name}.sha256")).ok_or_else(|| {
            AppError::new(AppErrorCode::NotFound, NO_CHECKSUM).with_detail("asset", asset_name)
        })?;
        let checksum = checksum_file(&self.text(&sidecar.url).await?).ok_or_else(|| {
            AppError::new(AppErrorCode::NotFound, NO_CHECKSUM).with_detail("asset", asset_name)
        })?;
        if archive
            .digest
            .as_ref()
            .is_some_and(|digest| *digest != checksum)
        {
            return Err(
                AppError::new(AppErrorCode::ValidationFailed, CHECKSUM_MISMATCH)
                    .with_detail("asset", asset_name),
            );
        }
        Ok(ToolchainBuild {
            runtime: ToolchainRuntime::Uv,
            version,
            asset_name: asset_name.to_owned(),
            url: archive.url.clone(),
            size_bytes: archive.size_bytes,
            checksum: sha256(&checksum),
        })
    }
}
