use std::sync::Arc;

use zvs_contract::{OpenSiteInput, SiteIconInput};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "sites.open",
            |services: Arc<Services>, input: OpenSiteInput| async move {
                services.sites.open(&input).await
            },
        )
        .handle(
            "sites.icon",
            |services: Arc<Services>, input: SiteIconInput| async move {
                services.sites.icon(&input).await
            },
        );
}
