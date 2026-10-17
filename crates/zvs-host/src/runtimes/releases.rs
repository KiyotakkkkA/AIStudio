use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde_json::{Value, json};
use zvs_contract::{AppError, AppErrorCode, ChecksumAlgorithm, ChecksumDto, Timestamp};

use super::definitions::{Platform, RuntimeDefinition};
use crate::drivers::ai::HttpClient;
use crate::platform::clock::SharedClock;
use crate::platform::logger::{LogLevel, SharedLogger};

pub const LLAMA_RELEASES_URL: &str =
    "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=8";
pub const LLAMA_RELEASES_PAGE: &str = "https://github.com/ggml-org/llama.cpp/releases";
pub const DEFAULT_RELEASE_TTL: Duration = Duration::from_secs(6 * 60 * 60);
pub const RATE_LIMITED_MESSAGE: &str =
    "GitHub временно ограничил число запросов. Попробуйте через несколько минут.";
pub const UNEXPECTED_ANSWER: &str = "GitHub вернул неожиданный ответ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAsset {
    pub name: String,
    pub url: String,
    pub size_bytes: u64,
    pub checksum: Option<ChecksumDto>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBuild {
    pub runtime_id: String,
    pub tag: String,
    pub assets: Vec<ResolvedAsset>,
}

#[derive(Clone)]
pub struct ReleaseResolverOptions {
    pub http: HttpClient,
    pub url: String,
    pub platform: Platform,
    pub ttl: Duration,
    pub clock: SharedClock,
    pub logger: SharedLogger,
}

impl ReleaseResolverOptions {
    pub fn new(http: HttpClient, clock: SharedClock, logger: SharedLogger) -> Self {
        Self {
            http,
            url: LLAMA_RELEASES_URL.to_owned(),
            platform: Platform::current(),
            ttl: DEFAULT_RELEASE_TTL,
            clock,
            logger,
        }
    }
}

struct Cached {
    releases: Arc<Vec<Value>>,
    at: Timestamp,
}

struct Inner {
    options: ReleaseResolverOptions,
    cached: Mutex<Option<Cached>>,
    loading: tokio::sync::Mutex<()>,
    generation: AtomicU64,
}

#[derive(Clone)]
pub struct ReleaseResolver {
    inner: Arc<Inner>,
}

fn is_build_tag(tag: &str) -> bool {
    tag.strip_prefix('b').is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn sha256_digest(digest: &str) -> Option<String> {
    let value = digest.strip_prefix("sha256:")?;
    let valid = value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    valid.then(|| value.to_owned())
}

fn read_assets(release: &Value) -> Vec<ResolvedAsset> {
    let Some(assets) = release.get("assets").and_then(Value::as_array) else {
        return Vec::new();
    };
    assets
        .iter()
        .filter_map(|raw| {
            let name = raw.get("name")?.as_str()?;
            let url = raw.get("browser_download_url")?.as_str()?;
            if raw
                .get("state")
                .is_some_and(|state| state.as_str() != Some("uploaded"))
            {
                return None;
            }
            Some(ResolvedAsset {
                name: name.to_owned(),
                url: url.to_owned(),
                size_bytes: raw
                    .get("size")
                    .and_then(Value::as_f64)
                    .filter(|size| size.is_finite() && *size >= 0.0)
                    .map_or(0, |size| size as u64),
                checksum: raw
                    .get("digest")
                    .and_then(Value::as_str)
                    .and_then(sha256_digest)
                    .map(|value| ChecksumDto {
                        algorithm: ChecksumAlgorithm::Sha256,
                        value,
                    }),
            })
        })
        .collect()
}

impl ReleaseResolver {
    pub fn new(options: ReleaseResolverOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                cached: Mutex::new(None),
                loading: tokio::sync::Mutex::new(()),
                generation: AtomicU64::new(0),
            }),
        }
    }

    pub fn platform(&self) -> Platform {
        self.inner.options.platform
    }

    fn cached(&self) -> MutexGuard<'_, Option<Cached>> {
        self.inner
            .cached
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub fn cached_releases(&self) -> bool {
        self.cached().is_some()
    }

    pub async fn resolve(
        &self,
        definition: &RuntimeDefinition,
        refresh: bool,
    ) -> Result<Option<ResolvedBuild>, AppError> {
        let Some(plan) = definition.assets(self.inner.options.platform) else {
            return Ok(None);
        };
        for release in self.releases(refresh).await?.iter() {
            let tag = release
                .get("tag_name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !is_build_tag(tag) || release.get("draft") == Some(&Value::Bool(true)) {
                continue;
            }
            let assets = read_assets(release);
            let Some(primary) = assets
                .iter()
                .find(|asset| plan.primary.is_match(&asset.name))
            else {
                continue;
            };
            let mut collected = vec![primary.clone()];
            let variant = plan
                .primary
                .captures(&primary.name)
                .and_then(|captures| captures.get(1))
                .map_or("", |variant| variant.as_str());
            if let Some(pattern) = plan.companion(variant) {
                let Some(companion) = assets.iter().find(|asset| pattern.is_match(&asset.name))
                else {
                    continue;
                };
                collected.push(companion.clone());
            }
            return Ok(Some(ResolvedBuild {
                runtime_id: definition.id.to_owned(),
                tag: tag.to_owned(),
                assets: collected,
            }));
        }
        Ok(None)
    }

    fn fresh(&self) -> Option<Arc<Vec<Value>>> {
        let ttl = i64::try_from(self.inner.options.ttl.as_millis()).unwrap_or(i64::MAX);
        let now = self.inner.options.clock.now();
        self.cached()
            .as_ref()
            .filter(|cached| now - cached.at < ttl)
            .map(|cached| Arc::clone(&cached.releases))
    }

    async fn releases(&self, refresh: bool) -> Result<Arc<Vec<Value>>, AppError> {
        if !refresh && let Some(releases) = self.fresh() {
            return Ok(releases);
        }
        let seen = self.inner.generation.load(Ordering::SeqCst);
        let _loading = self.inner.loading.lock().await;
        if self.inner.generation.load(Ordering::SeqCst) != seen
            && let Some(cached) = self.cached().as_ref()
        {
            return Ok(Arc::clone(&cached.releases));
        }
        match self.load().await {
            Ok(releases) => Ok(releases),
            Err(error) => {
                let kept = self
                    .cached()
                    .as_ref()
                    .map(|cached| Arc::clone(&cached.releases));
                let Some(kept) = kept else {
                    return Err(error);
                };
                self.inner.options.logger.log(
                    LogLevel::Warn,
                    "runtimes",
                    "Kept the cached release list",
                    &[("error", json!(error.message))],
                );
                Ok(kept)
            }
        }
    }

    async fn load(&self) -> Result<Arc<Vec<Value>>, AppError> {
        let options = &self.inner.options;
        let response = options
            .http
            .client()
            .get(&options.url)
            .header("accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|error| AppError::new(AppErrorCode::ProviderUnreachable, error.to_string()))?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(if status == 403 {
                AppError::new(AppErrorCode::RateLimited, RATE_LIMITED_MESSAGE)
            } else {
                AppError::new(
                    AppErrorCode::ProviderUnreachable,
                    format!("Не удалось получить список сборок движка ({status})"),
                )
            });
        }
        let body = response
            .bytes()
            .await
            .map_err(|error| AppError::new(AppErrorCode::ProviderUnreachable, error.to_string()))?;
        let payload: Value = serde_json::from_slice(&body)
            .map_err(|_| AppError::new(AppErrorCode::ValidationFailed, UNEXPECTED_ANSWER))?;
        let Value::Array(releases) = payload else {
            return Err(AppError::new(
                AppErrorCode::ValidationFailed,
                UNEXPECTED_ANSWER,
            ));
        };
        options.logger.log(
            LogLevel::Info,
            "runtimes",
            "Read the llama.cpp release list",
            &[("releases", json!(releases.len()))],
        );
        let releases = Arc::new(releases);
        *self.cached() = Some(Cached {
            releases: Arc::clone(&releases),
            at: options.clock.now(),
        });
        self.inner.generation.fetch_add(1, Ordering::SeqCst);
        Ok(releases)
    }
}

#[cfg(test)]
mod tests;
