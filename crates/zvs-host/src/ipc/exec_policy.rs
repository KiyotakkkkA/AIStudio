use std::sync::Arc;

use zvs_contract::{AllowRuleInput, ExecPolicyDto};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "execPolicy.get",
            |services: Arc<Services>, (): ()| async move { services.exec_policy.get() },
        )
        .handle(
            "execPolicy.set",
            |services: Arc<Services>, input: ExecPolicyDto| async move {
                services.exec_policy.set(&input)
            },
        )
        .handle(
            "execPolicy.allowRule",
            |services: Arc<Services>, input: AllowRuleInput| async move {
                services.exec_policy.allow_rule(&input)
            },
        );
}
