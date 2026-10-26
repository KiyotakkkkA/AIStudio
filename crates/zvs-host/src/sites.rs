use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqwest::redirect::Policy;
use url::{Host, Url};
use zvs_contract::validate::js_trim;
use zvs_contract::{
    AppError, AppErrorCode, OpenSiteInput, SITE_ICON_BYTES_MAX, SiteIconDto, SiteIconInput,
    Validate,
};

use crate::artifact_pages::{PAGES_UNAVAILABLE, SharedPageOpener};
use crate::browser::policy::is_web_url;

pub const ICON_TIMEOUT: Duration = Duration::from_secs(4);
pub const ICON_REDIRECTS: usize = 3;
pub const ICON_CACHE_MAX: usize = 512;
pub const NOT_A_SITE: &str = "Этот адрес нельзя открыть во встроенном браузере";

#[async_trait]
pub trait Resolve: Send + Sync {
    async fn lookup(&self, host: &str, port: u16) -> Vec<SocketAddr>;
}

pub struct SystemResolve;

#[async_trait]
impl Resolve for SystemResolve {
    async fn lookup(&self, host: &str, port: u16) -> Vec<SocketAddr> {
        tokio::net::lookup_host((host, port))
            .await
            .map(Iterator::collect)
            .unwrap_or_default()
    }
}

pub type Permit = fn(IpAddr) -> bool;

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || a == 0
        || (a == 100 && (64..128).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (18..20).contains(&b))
        || a >= 240)
}

fn public_v6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return public_v4(mapped);
    }
    let first = ip.segments()[0];
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
        || (first == 0x2001 && ip.segments()[1] == 0x0db8))
}

pub fn public_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_v4(ip),
        IpAddr::V6(ip) => public_v6(ip),
    }
}

pub fn sniff_icon(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(&[0, 0, 1, 0]) {
        Some("image/x-icon")
    } else {
        None
    }
}

pub fn icon_candidates(input: &SiteIconInput) -> Vec<Url> {
    let mut candidates: Vec<Url> = input
        .icon
        .as_deref()
        .and_then(|icon| Url::parse(js_trim(icon)).ok())
        .into_iter()
        .collect();
    if let Ok(page) = Url::parse(js_trim(&input.url))
        && let Ok(favicon) = page.join("/favicon.ico")
    {
        candidates.push(favicon);
    }
    candidates.retain(|url| url.scheme() == "https" || url.scheme() == "http");
    candidates
}

fn cache_key(input: &SiteIconInput) -> String {
    let origin = Url::parse(js_trim(&input.url))
        .map(|url| url.origin().ascii_serialization())
        .unwrap_or_default();
    format!("{origin} {}", input.icon.as_deref().unwrap_or_default())
}

pub struct SiteServiceOptions {
    pub pages: Option<SharedPageOpener>,
    pub resolver: Arc<dyn Resolve>,
    pub permit: Permit,
}

impl Default for SiteServiceOptions {
    fn default() -> Self {
        Self {
            pages: None,
            resolver: Arc::new(SystemResolve),
            permit: public_address,
        }
    }
}

struct Inner {
    options: SiteServiceOptions,
    icons: Mutex<HashMap<String, Option<SiteIconDto>>>,
}

#[derive(Clone)]
pub struct SiteService {
    inner: Arc<Inner>,
}

impl SiteService {
    pub fn new(options: SiteServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                options,
                icons: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub async fn open(&self, input: &OpenSiteInput) -> Result<(), AppError> {
        input.validate()?;
        let url = js_trim(&input.url);
        if !is_web_url(url) {
            return Err(AppError::new(AppErrorCode::ValidationFailed, NOT_A_SITE));
        }
        let pages = self
            .inner
            .options
            .pages
            .clone()
            .ok_or_else(|| AppError::new(AppErrorCode::NotImplemented, PAGES_UNAVAILABLE))?;
        pages.open(url).await
    }

    pub async fn icon(&self, input: &SiteIconInput) -> Result<Option<SiteIconDto>, AppError> {
        input.validate()?;
        let key = cache_key(input);
        if let Some(known) = self.icons().get(&key) {
            return Ok(known.clone());
        }
        let mut found = None;
        for candidate in icon_candidates(input) {
            found = self.fetch(candidate).await;
            if found.is_some() {
                break;
            }
        }
        let mut icons = self.icons();
        if icons.len() >= ICON_CACHE_MAX {
            icons.clear();
        }
        icons.insert(key, found.clone());
        Ok(found)
    }

    fn icons(&self) -> std::sync::MutexGuard<'_, HashMap<String, Option<SiteIconDto>>> {
        self.inner
            .icons
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    async fn address(&self, url: &Url) -> Option<SocketAddr> {
        let port = url.port_or_known_default()?;
        let permit = self.inner.options.permit;
        match url.host()? {
            Host::Ipv4(ip) => permit(IpAddr::V4(ip)).then(|| SocketAddr::new(IpAddr::V4(ip), port)),
            Host::Ipv6(ip) => permit(IpAddr::V6(ip)).then(|| SocketAddr::new(IpAddr::V6(ip), port)),
            Host::Domain(domain) => {
                let domain = domain.to_ascii_lowercase();
                if domain == "localhost" || domain.ends_with(".localhost") {
                    return None;
                }
                let addresses = self.inner.options.resolver.lookup(&domain, port).await;
                if addresses.is_empty() || !addresses.iter().all(|address| permit(address.ip())) {
                    return None;
                }
                addresses.first().copied()
            }
        }
    }

    async fn fetch(&self, start: Url) -> Option<SiteIconDto> {
        let mut url = start;
        for _ in 0..=ICON_REDIRECTS {
            if url.scheme() != "https" && url.scheme() != "http" {
                return None;
            }
            let address = self.address(&url).await?;
            let mut builder = reqwest::Client::builder()
                .no_proxy()
                .redirect(Policy::none())
                .timeout(ICON_TIMEOUT);
            if let Some(domain) = url.domain() {
                builder = builder.resolve(domain, address);
            }
            let client = builder.build().ok()?;
            let mut response = client
                .get(url.clone())
                .header(reqwest::header::ACCEPT, "image/*")
                .send()
                .await
                .ok()?;
            if response.status().is_redirection() {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)?
                    .to_str()
                    .ok()?;
                url = url.join(location).ok()?;
                continue;
            }
            if !response.status().is_success() {
                return None;
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.ok()? {
                bytes.extend_from_slice(&chunk);
                if bytes.len() > SITE_ICON_BYTES_MAX {
                    return None;
                }
            }
            let mime = sniff_icon(&bytes)?;
            return Some(SiteIconDto {
                mime: mime.to_owned(),
                data: STANDARD.encode(&bytes),
            });
        }
        None
    }
}

#[cfg(test)]
mod tests;
