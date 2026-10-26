pub mod agent;
pub mod automation;
pub mod capability;
pub mod credentials;
pub mod grants;
pub mod lifecycle;
pub mod network;
pub mod nodes;
pub mod page;
pub mod policy;

#[cfg(test)]
mod tests;

pub use agent::{AgentChange, AgentControls, AgentRuns, BrowserAgent, setting_limit};
pub use automation::{AutomationPort, ElementTarget, SharedAutomation};
pub use capability::{BrowserCapability, BrowserTier, resolve_tier, site_origin, site_scope};
pub use credentials::{FieldFacts, SecretField, secret_field};
pub use grants::{BrowserGrant, BrowserGrants};
pub use lifecycle::{BrowserLifecycle, BrowserLifecycleEvent, LifecycleSubscription};
pub use network::{NetworkCapture, NetworkEvent, NetworkLog, SharedNetworkCapture, record_for};
pub use nodes::{Browser, register_browser_nodes};
pub use page::{AxNode, ElementRef, Handles, PageElement};
pub use policy::{
    AGENT_PANEL_WIDTH, APPROVAL_HEIGHT, BROWSER_PARTITION, CHROME_HEIGHT, DEVTOOLS_SHARE,
    NAVIGATION_BLOCKED, ONLY_WEB_ADDRESSES, PROFILE_DIR, STATUS_HEIGHT, TAB_LIMIT, devtools_height,
    is_web_url, navigation_url, profile_dir,
};
