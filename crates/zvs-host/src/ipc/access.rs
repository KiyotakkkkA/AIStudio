use std::sync::Arc;

use zvs_contract::{AccessListDto, AppError, UserRulesDto};

use super::IpcServer;
use crate::services::Services;

fn access_list(services: &Services) -> Result<AccessListDto, AppError> {
    Ok(AccessListDto {
        policy: services.exec_policy.get()?,
        folders: services.folders.list()?,
        sites: services.browser_sites.sites()?,
        apps: services.computer.app_grants()?,
        tools: services.runs.grants().tool_grants()?,
        rules: services
            .exec_policy
            .user_rules()
            .unwrap_or_else(|_| UserRulesDto {
                rules: Vec::new(),
                unconfirmed: Vec::new(),
            }),
    })
}

pub fn register(server: &mut IpcServer) {
    server.handle(
        "access.list",
        |services: Arc<Services>, (): ()| async move { access_list(&services) },
    );
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::Value;
    use zvs_contract::{FolderGrantScope, ToolTier};

    use super::*;
    use crate::ipc::Caller;
    use crate::platform::clock::FakeClock;
    use crate::platform::events::EventBus;
    use crate::platform::logger::silent_logger;
    use crate::services::SystemService;
    use crate::testing::{temporary_directory, test_services};

    #[tokio::test]
    async fn the_list_groups_every_stored_grant_by_kind() {
        let host = test_services(SystemService::new(
            Arc::new(FakeClock::default()),
            EventBus::default(),
            Duration::ZERO,
        ));
        let folder = temporary_directory("access-list-");
        host.folders
            .grant(folder.path(), FolderGrantScope::Always, None)
            .expect("grants a folder");
        host.runs
            .permissions()
            .grant("mail.send", "skill:notes", ToolTier::Auto, "user")
            .expect("grants a tool");
        let mut server = IpcServer::new(host.shared(), silent_logger());
        register(&mut server);
        let reply = server
            .dispatch(Caller::Studio, "access.list", Value::Null)
            .await;
        assert_eq!(reply["ok"], true, "{reply}");
        let list: AccessListDto =
            serde_json::from_value(reply["data"].clone()).expect("an access list");
        assert_eq!(list.folders.len(), 1);
        assert!(list.apps.is_empty());
        let tool = list
            .tools
            .iter()
            .find(|grant| grant.tool == "mail.send")
            .expect("the tool grant is listed");
        assert_eq!(
            (tool.scope.as_str(), tool.tier, tool.granted_by.as_str()),
            ("skill:notes", ToolTier::Auto, "user")
        );
        assert_eq!(
            list.policy,
            host.exec_policy.get().expect("reads the policy")
        );
    }
}
