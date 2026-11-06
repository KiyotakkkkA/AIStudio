use std::sync::Arc;

use zvs_contract::{BrowserSiteDefaultInput, BrowserSitePutInput, BrowserSiteRef};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "browserSites.get",
            |services: Arc<Services>, _input: ()| async move { services.browser_sites.sites() },
        )
        .handle(
            "browserSites.setDefault",
            |services: Arc<Services>, input: BrowserSiteDefaultInput| async move {
                services
                    .browser_sites
                    .set_default(input.action, input.decision)
            },
        )
        .handle(
            "browserSites.put",
            |services: Arc<Services>, input: BrowserSitePutInput| async move {
                services.browser_sites.put(&input.site, &input.rules)
            },
        )
        .handle(
            "browserSites.remove",
            |services: Arc<Services>, input: BrowserSiteRef| async move {
                services.browser_sites.remove_site(&input.origin)
            },
        );
}
