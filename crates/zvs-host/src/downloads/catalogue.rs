use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use zvs_contract::{
    AppError, AppErrorCode, CatalogueItemState, CatalogueSource, ChecksumDto, DownloadItemKind,
    Timestamp,
};

use super::curated::curated_catalogue;
use crate::drivers::ai::HttpClient;
use crate::platform::clock::SharedClock;
use crate::platform::logger::{LogLevel, SharedLogger};

pub const DEFAULT_CATALOGUE_TTL: Duration = Duration::from_secs(60);
pub const ITEM_NOT_FOUND: &str = "Элемент каталога не найден";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueItem {
    pub item_ref: String,
    pub kind: DownloadItemKind,
    pub source: CatalogueSource,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub version: Option<String>,
    pub size_bytes: u64,
    pub url: String,
    pub file_name: String,
    pub checksum: Option<ChecksumDto>,
    pub digest: Option<String>,
    pub tags: Vec<String>,
    pub dimension: Option<u64>,
    pub query_prefix: Option<String>,
    pub document_prefix: Option<String>,
    pub state: Option<CatalogueItemState>,
    pub blocked_reason: Option<String>,
}

impl CatalogueItem {
    pub fn new(
        item_ref: impl Into<String>,
        kind: DownloadItemKind,
        source: CatalogueSource,
        name: impl Into<String>,
    ) -> Self {
        let name = name.into();
        Self {
            item_ref: item_ref.into(),
            kind,
            source,
            display_name: name.clone(),
            name,
            description: String::new(),
            version: None,
            size_bytes: 0,
            url: String::new(),
            file_name: String::new(),
            checksum: None,
            digest: None,
            tags: Vec::new(),
            dimension: None,
            query_prefix: None,
            document_prefix: None,
            state: None,
            blocked_reason: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledItem {
    pub kind: DownloadItemKind,
    pub name: String,
    pub version: Option<String>,
    pub size_bytes: u64,
    pub digest: Option<String>,
}

#[async_trait]
pub trait CatalogueProvider: Send + Sync {
    fn id(&self) -> CatalogueSource;

    fn live(&self) -> bool;

    async fn list(&self) -> Result<Vec<CatalogueItem>, AppError>;

    async fn installed(&self) -> Result<Vec<InstalledItem>, AppError> {
        Ok(Vec::new())
    }
}

pub type SharedCatalogueProvider = Arc<dyn CatalogueProvider>;

pub fn installed_key(kind: DownloadItemKind, name: &str) -> String {
    format!("{kind}:{name}")
}

pub struct CuratedProvider;

#[async_trait]
impl CatalogueProvider for CuratedProvider {
    fn id(&self) -> CatalogueSource {
        CatalogueSource::Curated
    }

    fn live(&self) -> bool {
        false
    }

    async fn list(&self) -> Result<Vec<CatalogueItem>, AppError> {
        Ok(curated_catalogue())
    }
}

pub type BaseUrlSource = Arc<dyn Fn() -> Option<String> + Send + Sync>;

pub struct OllamaProvider {
    base_url: BaseUrlSource,
    http: HttpClient,
    logger: SharedLogger,
}

impl OllamaProvider {
    pub fn new(base_url: BaseUrlSource, http: HttpClient, logger: SharedLogger) -> Self {
        Self {
            base_url,
            http,
            logger,
        }
    }

    async fn tags(&self, base_url: &str) -> Result<Value, AppError> {
        let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
        let response =
            self.http.client().get(url).send().await.map_err(|error| {
                AppError::new(AppErrorCode::ProviderUnreachable, error.to_string())
            })?;
        if !response.status().is_success() {
            return Err(AppError::new(
                AppErrorCode::ProviderUnreachable,
                format!("Ollama ответил {}", response.status().as_u16()),
            ));
        }
        let body = response
            .bytes()
            .await
            .map_err(|error| AppError::new(AppErrorCode::ProviderUnreachable, error.to_string()))?;
        serde_json::from_slice(&body)
            .map_err(|error| AppError::new(AppErrorCode::ProviderUnreachable, error.to_string()))
    }
}

fn ollama_item(raw: &Value) -> Option<InstalledItem> {
    let name = raw.get("name")?.as_str()?;
    let kind = if name.to_lowercase().contains("embed") {
        DownloadItemKind::Embedding
    } else {
        DownloadItemKind::Model
    };
    Some(InstalledItem {
        kind,
        name: name.to_owned(),
        version: raw
            .get("details")
            .and_then(|details| details.get("quantization_level"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        size_bytes: raw
            .get("size")
            .and_then(Value::as_f64)
            .filter(|size| size.is_finite() && *size >= 0.0)
            .map_or(0, |size| size as u64),
        digest: raw
            .get("digest")
            .and_then(Value::as_str)
            .map(|digest| digest.strip_prefix("sha256:").unwrap_or(digest).to_owned()),
    })
}

#[async_trait]
impl CatalogueProvider for OllamaProvider {
    fn id(&self) -> CatalogueSource {
        CatalogueSource::Ollama
    }

    fn live(&self) -> bool {
        true
    }

    async fn list(&self) -> Result<Vec<CatalogueItem>, AppError> {
        Ok(Vec::new())
    }

    async fn installed(&self) -> Result<Vec<InstalledItem>, AppError> {
        let Some(base_url) = (self.base_url)() else {
            return Ok(Vec::new());
        };
        let payload = match self.tags(&base_url).await {
            Ok(payload) => payload,
            Err(error) => {
                self.logger.log(
                    LogLevel::Warn,
                    "downloads",
                    "Could not list the installed Ollama models",
                    &[
                        ("baseUrl", json!(base_url)),
                        ("error", json!(error.message)),
                    ],
                );
                return Ok(Vec::new());
            }
        };
        let Some(models) = payload.get("models").and_then(Value::as_array) else {
            return Ok(Vec::new());
        };
        Ok(models.iter().filter_map(ollama_item).collect())
    }
}

struct Snapshot<T> {
    value: T,
    at: Timestamp,
}

#[derive(Clone)]
pub struct CatalogueServiceOptions {
    pub providers: Vec<SharedCatalogueProvider>,
    pub ttl: Duration,
    pub clock: SharedClock,
    pub logger: SharedLogger,
}

impl CatalogueServiceOptions {
    pub fn new(clock: SharedClock, logger: SharedLogger) -> Self {
        Self {
            providers: vec![Arc::new(CuratedProvider)],
            ttl: DEFAULT_CATALOGUE_TTL,
            clock,
            logger,
        }
    }
}

type InstalledMap = HashMap<String, InstalledItem>;

struct Inner {
    providers: RwLock<Vec<SharedCatalogueProvider>>,
    offered: Mutex<HashMap<String, CatalogueItem>>,
    items: Mutex<Option<Snapshot<Vec<CatalogueItem>>>>,
    present: Mutex<Option<Snapshot<InstalledMap>>>,
    ttl: Duration,
    clock: SharedClock,
    logger: SharedLogger,
}

#[derive(Clone)]
pub struct CatalogueService {
    inner: Arc<Inner>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl CatalogueService {
    pub fn new(options: CatalogueServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                providers: RwLock::new(options.providers),
                offered: Mutex::new(HashMap::new()),
                items: Mutex::new(None),
                present: Mutex::new(None),
                ttl: options.ttl,
                clock: options.clock,
                logger: options.logger,
            }),
        }
    }

    pub fn add_provider(&self, provider: SharedCatalogueProvider) {
        self.inner
            .providers
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .push(provider);
        self.invalidate();
    }

    pub fn offer(&self, item: CatalogueItem) -> CatalogueItem {
        lock(&self.inner.offered).insert(item.item_ref.clone(), item.clone());
        item
    }

    fn providers(&self) -> Vec<SharedCatalogueProvider> {
        self.inner
            .providers
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn fresh(&self, at: Timestamp) -> bool {
        let ttl = i64::try_from(self.inner.ttl.as_millis()).unwrap_or(i64::MAX);
        self.inner.clock.now() - at < ttl
    }

    fn warn(&self, provider: &SharedCatalogueProvider, error: &AppError) {
        self.inner.logger.log(
            LogLevel::Warn,
            "downloads",
            "A catalogue source failed",
            &[
                ("source", json!(provider.id().as_str())),
                ("error", json!(error.message)),
            ],
        );
    }

    pub async fn list(&self, refresh: bool) -> Vec<CatalogueItem> {
        if !refresh
            && let Some(snapshot) = lock(&self.inner.items).as_ref()
            && self.fresh(snapshot.at)
        {
            return snapshot.value.clone();
        }
        let mut collected: Vec<CatalogueItem> = Vec::new();
        let mut positions: HashMap<String, usize> = HashMap::new();
        for provider in self.providers() {
            if !refresh && provider.live() {
                continue;
            }
            let items = provider.list().await.unwrap_or_else(|error| {
                self.warn(&provider, &error);
                Vec::new()
            });
            for item in items {
                match positions.get(&item.item_ref) {
                    Some(index) => collected[*index] = item,
                    None => {
                        positions.insert(item.item_ref.clone(), collected.len());
                        collected.push(item);
                    }
                }
            }
        }
        let mut items = lock(&self.inner.items);
        let merged = match (collected.is_empty(), items.as_ref()) {
            (true, Some(previous)) => previous.value.clone(),
            _ => collected,
        };
        *items = Some(Snapshot {
            value: merged.clone(),
            at: self.inner.clock.now(),
        });
        merged
    }

    pub async fn installed(&self, refresh: bool) -> InstalledMap {
        if !refresh
            && let Some(snapshot) = lock(&self.inner.present).as_ref()
            && self.fresh(snapshot.at)
        {
            return snapshot.value.clone();
        }
        let mut collected = InstalledMap::new();
        for provider in self.providers() {
            let items = provider.installed().await.unwrap_or_else(|error| {
                self.warn(&provider, &error);
                Vec::new()
            });
            for item in items {
                collected.insert(installed_key(item.kind, &item.name), item);
            }
        }
        *lock(&self.inner.present) = Some(Snapshot {
            value: collected.clone(),
            at: self.inner.clock.now(),
        });
        collected
    }

    pub async fn find(&self, item_ref: &str) -> Result<CatalogueItem, AppError> {
        if let Some(offered) = lock(&self.inner.offered).get(item_ref) {
            return Ok(offered.clone());
        }
        self.list(false)
            .await
            .into_iter()
            .find(|candidate| candidate.item_ref == item_ref)
            .ok_or_else(|| AppError::new(AppErrorCode::NotFound, ITEM_NOT_FOUND))
    }

    pub fn invalidate(&self) {
        *lock(&self.inner.items) = None;
        *lock(&self.inner.present) = None;
    }
}

#[cfg(test)]
mod tests;
