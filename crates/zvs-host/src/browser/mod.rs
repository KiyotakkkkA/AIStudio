pub mod lifecycle;
pub mod network;
pub mod policy;

pub use lifecycle::{BrowserLifecycle, BrowserLifecycleEvent, LifecycleSubscription};
pub use network::{NetworkCapture, NetworkEvent, NetworkLog, SharedNetworkCapture, record_for};
pub use policy::{
    BROWSER_PARTITION, CHROME_HEIGHT, DEVTOOLS_SHARE, NAVIGATION_BLOCKED, ONLY_WEB_ADDRESSES,
    PROFILE_DIR, STATUS_HEIGHT, TAB_LIMIT, devtools_height, is_web_url, navigation_url,
    profile_dir,
};
