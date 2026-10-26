pub mod accounts;
pub mod automation;
pub mod bearer;
pub mod cdp;
pub mod cookies;
pub mod gateway;
pub mod model;
pub mod network;
mod workspace;

pub use accounts::{WorkspaceOpener, WorkspacePages, gateway_sessions};
pub use bearer::BearerTokens;
pub use gateway::{AccountGateway, WebviewPages};
pub use workspace::{BROWSER_UI, BrowserWorkspace};
