use std::sync::Arc;

use zvs_contract::{AppError, BrowserGrantRef};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "browserGrants.list",
            |services: Arc<Services>, _input: ()| async move { services.browser.site_grants() },
        )
        .handle(
            "browserGrants.revoke",
            |services: Arc<Services>, input: BrowserGrantRef| async move {
                services.browser.revoke(&input.id)?;
                Ok::<(), AppError>(())
            },
        );
}
