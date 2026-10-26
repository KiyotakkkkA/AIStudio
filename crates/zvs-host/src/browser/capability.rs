use zvs_contract::ToolTier;

use super::policy::is_web_url;

pub const BROWSER_NAVIGATE: &str = "browser.navigate";
pub const BROWSER_READ_PAGE: &str = "browser.read_page";
pub const BROWSER_SCREENSHOT: &str = "browser.screenshot";
pub const BROWSER_FIND: &str = "browser.find";
pub const BROWSER_CLICK: &str = "browser.click";
pub const BROWSER_TYPE: &str = "browser.type";
pub const BROWSER_SUBMIT: &str = "browser.submit";
pub const BROWSER_DOWNLOAD: &str = "browser.download";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrowserCapability {
    Navigate,
    ReadPage,
    Screenshot,
    Find,
    Click,
    Type,
    Submit,
    Download,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserTier {
    Auto,
    PerSite,
    AlwaysAsk,
}

impl BrowserCapability {
    pub const ALL: [Self; 8] = [
        Self::Navigate,
        Self::ReadPage,
        Self::Screenshot,
        Self::Find,
        Self::Click,
        Self::Type,
        Self::Submit,
        Self::Download,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Navigate => BROWSER_NAVIGATE,
            Self::ReadPage => BROWSER_READ_PAGE,
            Self::Screenshot => BROWSER_SCREENSHOT,
            Self::Find => BROWSER_FIND,
            Self::Click => BROWSER_CLICK,
            Self::Type => BROWSER_TYPE,
            Self::Submit => BROWSER_SUBMIT,
            Self::Download => BROWSER_DOWNLOAD,
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|capability| capability.key() == key)
    }

    pub const fn tier(self) -> BrowserTier {
        match self {
            Self::Navigate | Self::ReadPage | Self::Screenshot | Self::Find => BrowserTier::Auto,
            Self::Click | Self::Type => BrowserTier::PerSite,
            Self::Submit | Self::Download => BrowserTier::AlwaysAsk,
        }
    }

    pub const fn grantable(self) -> bool {
        matches!(self.tier(), BrowserTier::PerSite)
    }

    pub const fn side_effect(self) -> bool {
        !matches!(self, Self::ReadPage | Self::Screenshot | Self::Find)
    }
}

pub fn resolve_tier(
    capability: BrowserCapability,
    origin: Option<&str>,
    granted: impl FnOnce(&str, BrowserCapability) -> bool,
) -> ToolTier {
    match capability.tier() {
        BrowserTier::Auto => ToolTier::Auto,
        BrowserTier::AlwaysAsk => ToolTier::Ask,
        BrowserTier::PerSite => match origin {
            Some(origin) if granted(origin, capability) => ToolTier::Auto,
            _ => ToolTier::Ask,
        },
    }
}

pub fn site_origin(url: &str) -> Option<String> {
    if !is_web_url(url) {
        return None;
    }
    let parsed = url::Url::parse(url).ok()?;
    let origin = parsed.origin();
    origin.is_tuple().then(|| origin.ascii_serialization())
}

pub fn site_scope(origin: &str) -> String {
    format!("site:{origin}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tiers_are_exactly_the_passport_table() {
        let tiers: Vec<(&str, BrowserTier)> = BrowserCapability::ALL
            .into_iter()
            .map(|capability| (capability.key(), capability.tier()))
            .collect();
        assert_eq!(
            tiers,
            [
                ("browser.navigate", BrowserTier::Auto),
                ("browser.read_page", BrowserTier::Auto),
                ("browser.screenshot", BrowserTier::Auto),
                ("browser.find", BrowserTier::Auto),
                ("browser.click", BrowserTier::PerSite),
                ("browser.type", BrowserTier::PerSite),
                ("browser.submit", BrowserTier::AlwaysAsk),
                ("browser.download", BrowserTier::AlwaysAsk),
            ]
        );
        for capability in BrowserCapability::ALL {
            assert_eq!(
                BrowserCapability::from_key(capability.key()),
                Some(capability)
            );
        }
        assert_eq!(BrowserCapability::from_key("browser.evaluate"), None);
    }

    #[test]
    fn always_ask_ignores_every_grant() {
        for capability in [BrowserCapability::Submit, BrowserCapability::Download] {
            assert_eq!(
                resolve_tier(capability, Some("https://shop.example"), |_, _| true),
                ToolTier::Ask
            );
            assert!(!capability.grantable());
        }
    }

    #[test]
    fn per_site_tiers_need_a_grant_for_that_origin() {
        let granted = |origin: &str, capability: BrowserCapability| {
            origin == "https://a.example" && capability == BrowserCapability::Click
        };
        assert_eq!(
            resolve_tier(BrowserCapability::Click, Some("https://a.example"), granted),
            ToolTier::Auto
        );
        assert_eq!(
            resolve_tier(BrowserCapability::Click, Some("https://b.example"), granted),
            ToolTier::Ask
        );
        assert_eq!(
            resolve_tier(BrowserCapability::Type, Some("https://a.example"), granted),
            ToolTier::Ask
        );
        assert_eq!(
            resolve_tier(BrowserCapability::Click, None, |_, _| true),
            ToolTier::Ask
        );
        assert_eq!(
            resolve_tier(BrowserCapability::ReadPage, None, |_, _| false),
            ToolTier::Auto
        );
    }

    #[test]
    fn origins_are_scheme_host_and_port_of_web_pages_only() {
        assert_eq!(
            site_origin("https://Shop.Example/cart?x=1#y").as_deref(),
            Some("https://shop.example")
        );
        assert_eq!(
            site_origin("http://127.0.0.1:8123/form").as_deref(),
            Some("http://127.0.0.1:8123")
        );
        assert_eq!(
            site_origin("https://shop.example:443/").as_deref(),
            Some("https://shop.example")
        );
        assert_ne!(
            site_origin("https://shop.example.evil.test/"),
            site_origin("https://shop.example/")
        );
        for url in [
            "about:blank",
            "file:///C:/a.html",
            "data:text/html,x",
            "javascript:alert(1)",
            "",
        ] {
            assert_eq!(site_origin(url), None, "{url}");
        }
        assert_eq!(site_scope("https://a.example"), "site:https://a.example");
    }
}
