use std::sync::Arc;

use zvs_contract::{AllowRuleInput, ExecPolicyDto, SubagentPreferencesDto};

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
        )
        .handle(
            "subagents.get",
            |services: Arc<Services>, (): ()| async move { services.subagents.get() },
        )
        .handle(
            "subagents.set",
            |services: Arc<Services>, input: SubagentPreferencesDto| async move {
                services.subagents.set(&input)
            },
        );
}
