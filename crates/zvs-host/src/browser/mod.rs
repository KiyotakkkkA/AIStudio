pub mod agent;
pub mod automation;
pub mod capability;
pub mod credentials;
pub mod downloads;
pub mod grants;
pub mod import;
pub mod library;
pub mod lifecycle;
pub mod network;
pub mod nodes;
pub mod page;
pub mod policy;
pub mod settings;

#[cfg(test)]
mod tests;

pub use agent::{AgentChange, AgentControls, AgentRuns, BrowserAgent, setting_limit};
pub use automation::{AutomationPort, ElementTarget, SharedAutomation};
pub use capability::{BrowserCapability, BrowserTier, resolve_tier, site_origin, site_scope};
pub use credentials::{FieldFacts, SecretField, secret_field};
pub use downloads::{BrowserDownloads, DownloadRecord, move_file, unique_path};
pub use grants::{
    BrowserGrant, BrowserGrants, DEFAULT_ORIGIN, SITE_BLOCKED, SiteDecision, canonical_site,
    site_action,
};
pub use import::{BrowserImporter, ImportRoots};
pub use library::{BrowserLibrary, ImportedBookmark, ImportedVisit};
pub use lifecycle::{BrowserLifecycle, BrowserLifecycleEvent, LifecycleSubscription};
pub use network::{NetworkCapture, NetworkEvent, NetworkLog, SharedNetworkCapture, record_for};
pub use nodes::{Browser, register_browser_nodes};
pub use page::{AxNode, ElementRef, Handles, PageElement};
pub use policy::{
    AGENT_PANEL_WIDTH, APPROVAL_HEIGHT, BROWSER_PARTITION, CHROME_HEIGHT, DEVTOOLS_SHARE,
    NAVIGATION_BLOCKED, ONLY_WEB_ADDRESSES, PROFILE_DIR, STATUS_HEIGHT, TAB_LIMIT, devtools_height,
    is_web_url, navigation_url, profile_dir,
};
pub use settings::{
    ACCOUNT_NEEDS_BROWSER, BROWSER_DISABLED, BrowserProfile, BrowserSettings, PROFILE_UNAVAILABLE,
    RoutedPages, SettingsListener, SharedBrowserProfile, is_local_url, routed_url_opener,
    tool_switch,
};
