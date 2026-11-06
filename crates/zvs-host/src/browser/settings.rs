use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use serde_json::{Map, Value};
use url::{Host, Url};
use zvs_contract::{AppError, AppErrorCode, BrowserLinkTarget, BrowserSettingsDto, Validate};

use super::capability::BrowserCapability;
use crate::artifact_pages::{PageOpener, SharedPageOpener};
use crate::platform::external::UrlOpener;
use crate::services::SettingService;
use crate::tools::ToolRegistry;

pub const BROWSER_SETTINGS_KEY: &str = "browser.settings";
pub const BROWSER_DISABLED: &str =
    "Встроенный браузер выключен. Включите его в разделе «Параметры» → «Браузер».";
pub const ACCOUNT_NEEDS_BROWSER: &str = "Вход в аккаунт выполняется во встроенном браузере. Включите его в разделе «Параметры» → «Браузер».";

pub type SettingsListener = Arc<dyn Fn(&BrowserSettingsDto) + Send + Sync>;

pub const PROFILE_UNAVAILABLE: &str = "Данные браузера недоступны";

#[async_trait]
pub trait BrowserProfile: Send + Sync {
    async fn clear_data(&self) -> Result<(), AppError>;
}

pub type SharedBrowserProfile = Arc<dyn BrowserProfile>;

struct Inner {
    settings: SettingService,
    listeners: Mutex<Vec<SettingsListener>>,
}

#[derive(Clone)]
pub struct BrowserSettings(Arc<Inner>);

fn defaults() -> Map<String, Value> {
    match serde_json::to_value(BrowserSettingsDto::default()) {
        Ok(Value::Object(fields)) => fields,
        _ => Map::new(),
    }
}

pub fn decode_settings(stored: Option<&Value>) -> BrowserSettingsDto {
    let mut merged = defaults();
    if let Some(Value::Object(stored)) = stored {
        for (key, value) in stored {
            let Some(previous) = merged.get(key).cloned() else {
                continue;
            };
            merged.insert(key.clone(), value.clone());
            if serde_json::from_value::<BrowserSettingsDto>(Value::Object(merged.clone())).is_err()
            {
                merged.insert(key.clone(), previous);
            }
        }
    }
    serde_json::from_value(Value::Object(merged)).unwrap_or_default()
}

pub fn is_local_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    match parsed.host() {
        Some(Host::Domain(domain)) => {
            let domain = domain.to_ascii_lowercase();
            domain == "localhost" || domain.ends_with(".localhost")
        }
        Some(Host::Ipv4(ip)) => ip.is_loopback() || ip == Ipv4Addr::UNSPECIFIED,
        Some(Host::Ipv6(ip)) => ip.is_loopback() || ip == Ipv6Addr::UNSPECIFIED,
        None => false,
    }
}

impl BrowserSettings {
    pub fn new(settings: SettingService) -> Self {
        Self(Arc::new(Inner {
            settings,
            listeners: Mutex::default(),
        }))
    }

    pub fn get(&self) -> BrowserSettingsDto {
        let stored = self
            .0
            .settings
            .get(BROWSER_SETTINGS_KEY)
            .ok()
            .flatten()
            .map(|setting| setting.value);
        decode_settings(stored.as_ref())
    }

    pub fn set(&self, next: &BrowserSettingsDto) -> Result<BrowserSettingsDto, AppError> {
        next.validate()?;
        let value = serde_json::to_value(next)
            .map_err(|error| AppError::new(AppErrorCode::Unknown, error.to_string()))?;
        self.0.settings.set(BROWSER_SETTINGS_KEY, &value)?;
        let listeners = self
            .0
            .listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        for listener in listeners {
            listener(next);
        }
        Ok(next.clone())
    }

    pub fn subscribe(&self, listener: SettingsListener) {
        listener(&self.get());
        self.0
            .listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(listener);
    }

    pub fn enabled(&self) -> bool {
        self.get().enabled
    }

    pub fn require_enabled(&self) -> Result<(), AppError> {
        if self.enabled() {
            Ok(())
        } else {
            Err(AppError::new(AppErrorCode::Conflict, BROWSER_DISABLED))
        }
    }

    pub fn target(&self, url: &str) -> BrowserLinkTarget {
        let settings = self.get();
        if !settings.enabled {
            BrowserLinkTarget::System
        } else if is_local_url(url) {
            settings.local_links
        } else {
            settings.web_links
        }
    }
}

pub fn tool_switch(tools: ToolRegistry) -> SettingsListener {
    Arc::new(move |settings| {
        for capability in BrowserCapability::ALL {
            let _ = tools.set_available(capability.key(), settings.enabled);
        }
    })
}

pub struct RoutedPages {
    pages: SharedPageOpener,
    system: UrlOpener,
    settings: BrowserSettings,
}

impl RoutedPages {
    pub fn new(pages: SharedPageOpener, system: UrlOpener, settings: BrowserSettings) -> Self {
        Self {
            pages,
            system,
            settings,
        }
    }
}

#[async_trait]
impl PageOpener for RoutedPages {
    async fn open(&self, url: &str) -> Result<(), AppError> {
        match self.settings.target(url) {
            BrowserLinkTarget::Studio => self.pages.open(url).await,
            BrowserLinkTarget::System => (self.system)(url),
        }
    }
}

pub fn routed_url_opener(
    studio: UrlOpener,
    system: UrlOpener,
    settings: BrowserSettings,
) -> UrlOpener {
    Arc::new(move |url| {
        if settings.enabled() {
            studio(url)
        } else {
            system(url)
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use serde_json::json;

    use super::*;
    use crate::platform::clock::FakeClock;
    use crate::testing::temporary_database;

    fn settings(database: &crate::testing::TemporaryDatabase) -> BrowserSettings {
        BrowserSettings::new(SettingService::new(
            database.shared(),
            Arc::new(FakeClock::default()),
        ))
    }

    struct CountingPages(Arc<Mutex<Vec<String>>>);

    #[async_trait]
    impl PageOpener for CountingPages {
        async fn open(&self, url: &str) -> Result<(), AppError> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(url.to_owned());
            Ok(())
        }
    }

    #[test]
    fn missing_or_damaged_fields_fall_back_to_the_defaults() {
        assert_eq!(decode_settings(None), BrowserSettingsDto::default());
        assert_eq!(
            decode_settings(Some(
                &json!({ "enabled": false, "webLinks": "edge", "extra": 1 })
            )),
            BrowserSettingsDto {
                enabled: false,
                ..BrowserSettingsDto::default()
            }
        );
        assert_eq!(
            decode_settings(Some(&json!("broken"))),
            BrowserSettingsDto::default()
        );
    }

    #[test]
    fn loopback_addresses_are_local_and_sites_are_not() {
        for url in [
            "http://localhost:5173/",
            "http://app.localhost/",
            "http://127.0.0.1:8080/a",
            "http://[::1]:3000/",
            "http://0.0.0.0:8000/",
        ] {
            assert!(is_local_url(url), "{url}");
        }
        for url in ["https://example.com/", "http://192.168.1.4/", "not a url"] {
            assert!(!is_local_url(url), "{url}");
        }
    }

    #[test]
    fn a_saved_change_reaches_every_listener_and_survives_a_reload() {
        let database = temporary_database();
        let browser = settings(&database);
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        browser.subscribe(Arc::new(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
        }));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let next = BrowserSettingsDto {
            enabled: false,
            full_url: true,
            ..BrowserSettingsDto::default()
        };
        assert_eq!(browser.set(&next).expect("saves"), next);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(settings(&database).get(), next);
        assert_eq!(
            browser.require_enabled().expect_err("disabled").message,
            BROWSER_DISABLED
        );
    }

    #[tokio::test]
    async fn links_follow_the_destination_setting_and_the_switch() {
        let database = temporary_database();
        let browser = settings(&database);
        let studio = Arc::new(Mutex::new(Vec::new()));
        let system = Arc::new(Mutex::new(Vec::new()));
        let outside = Arc::clone(&system);
        let pages = RoutedPages::new(
            Arc::new(CountingPages(Arc::clone(&studio))),
            Arc::new(move |url: &str| {
                outside
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(url.to_owned());
                Ok(())
            }),
            browser.clone(),
        );
        pages.open("https://example.com/").await.expect("opens");
        browser
            .set(&BrowserSettingsDto {
                local_links: BrowserLinkTarget::System,
                ..BrowserSettingsDto::default()
            })
            .expect("saves");
        pages.open("http://localhost:5173/").await.expect("opens");
        pages.open("https://example.org/").await.expect("opens");
        browser
            .set(&BrowserSettingsDto {
                enabled: false,
                ..BrowserSettingsDto::default()
            })
            .expect("saves");
        pages.open("https://example.net/").await.expect("opens");
        assert_eq!(
            *studio.lock().unwrap_or_else(PoisonError::into_inner),
            ["https://example.com/", "https://example.org/"]
        );
        assert_eq!(
            *system.lock().unwrap_or_else(PoisonError::into_inner),
            ["http://localhost:5173/", "https://example.net/"]
        );
    }
}
