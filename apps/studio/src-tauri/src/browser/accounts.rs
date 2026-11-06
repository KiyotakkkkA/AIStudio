use std::sync::Arc;

use async_trait::async_trait;
use zvs_contract::{AppError, AppErrorCode};
use zvs_host::artifact_pages::{PAGES_UNAVAILABLE, PageOpener};
use zvs_host::browser::{ACCOUNT_NEEDS_BROWSER, BrowserSettings, routed_url_opener};
use zvs_host::drivers::ai::{SessionFactory, SharedSessionGateway};
use zvs_host::platform::external::{OPEN_FAILED, UrlOpener, system_url_opener};
use zvs_host::services::AccountBrowser;

use super::{AccountGateway, BrowserWorkspace};

pub const BROWSER_UNAVAILABLE: &str = "Browser is unavailable";

/// Opens OAuth and sign-in pages of plugin accounts in the integrated browser,
/// next to the account linking tabs, instead of the system browser.
pub fn workspace_url_opener(workspace: &BrowserWorkspace, settings: BrowserSettings) -> UrlOpener {
    let workspace = workspace.clone();
    routed_url_opener(
        Arc::new(move |url| {
            workspace.open_page_tab(url).map_err(|reason| {
                AppError::new(AppErrorCode::Unknown, OPEN_FAILED).with_detail("reason", reason)
            })
        }),
        system_url_opener(),
        settings,
    )
}

pub struct WorkspaceOpener(pub BrowserWorkspace, pub BrowserSettings);

fn unavailable(reason: &str) -> AppError {
    AppError::new(AppErrorCode::Unknown, BROWSER_UNAVAILABLE).with_detail("reason", reason)
}

#[async_trait]
impl AccountBrowser for WorkspaceOpener {
    async fn open_external(&self, url: &str) -> Result<(), AppError> {
        zvs_host::platform::external::open_in_system_browser(url)
    }

    async fn open_tab(&self, url: &str) -> Result<(), AppError> {
        if !self.1.enabled() {
            return Err(AppError::new(AppErrorCode::Conflict, ACCOUNT_NEEDS_BROWSER));
        }
        let workspace = self.0.clone();
        let url = url.to_owned();
        tauri::async_runtime::spawn_blocking(move || workspace.open_link_tab(&url))
            .await
            .map_err(|error| unavailable(&error.to_string()))?
            .map_err(|error| unavailable(&error))
    }
}

pub struct WorkspacePages(pub BrowserWorkspace);

fn pages_unavailable(reason: &str) -> AppError {
    AppError::new(AppErrorCode::Unknown, PAGES_UNAVAILABLE).with_detail("reason", reason)
}

#[async_trait]
impl PageOpener for WorkspacePages {
    async fn open(&self, url: &str) -> Result<(), AppError> {
        let workspace = self.0.clone();
        let url = url.to_owned();
        tauri::async_runtime::spawn_blocking(move || workspace.open_page_tab(&url))
            .await
            .map_err(|error| pages_unavailable(&error.to_string()))?
            .map_err(|error| pages_unavailable(&error))
    }
}

pub fn gateway_sessions(gateway: Arc<AccountGateway>) -> SessionFactory {
    Arc::new(move |_partition: &str| -> SharedSessionGateway { gateway.clone() })
}
