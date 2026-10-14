use std::sync::Arc;

use async_trait::async_trait;
use zvs_contract::{AppError, AppErrorCode};
use zvs_host::drivers::ai::{SessionFactory, SharedSessionGateway};
use zvs_host::services::AccountBrowser;

use super::{AccountGateway, BrowserWorkspace};

pub const BROWSER_UNAVAILABLE: &str = "Browser is unavailable";

pub struct WorkspaceOpener(pub BrowserWorkspace);

fn unavailable(reason: &str) -> AppError {
    AppError::new(AppErrorCode::Unknown, BROWSER_UNAVAILABLE).with_detail("reason", reason)
}

#[async_trait]
impl AccountBrowser for WorkspaceOpener {
    async fn open_tab(&self, url: &str) -> Result<(), AppError> {
        let workspace = self.0.clone();
        let url = url.to_owned();
        tauri::async_runtime::spawn_blocking(move || workspace.open_link_tab(&url))
            .await
            .map_err(|error| unavailable(&error.to_string()))?
            .map_err(|error| unavailable(&error))
    }
}

pub fn gateway_sessions(gateway: Arc<AccountGateway>) -> SessionFactory {
    Arc::new(move |_partition: &str| -> SharedSessionGateway { gateway.clone() })
}
