use std::sync::Arc;

use zvs_contract::{
    AllowRuleInput, AppError, ConfirmRulePackInput, ExecPolicyDto, PolicyRuleDto, RemoveRuleInput,
    SubagentPreferencesDto,
};

use super::IpcServer;
use crate::services::Services;

fn allow_rule(services: &Services, input: &AllowRuleInput) -> Result<PolicyRuleDto, AppError> {
    let review = services
        .runs
        .approvals()
        .reviewed(&input.run_id.to_string(), &input.approval_id)?;
    let working = match review.conversation.as_deref() {
        Some(conversation) => services
            .folders
            .working_folder_of(conversation)?
            .map(|folder| folder.to_string_lossy().into_owned()),
        None => None,
    };
    services
        .exec_policy
        .allow_rule(&review.action, input.scope, working.as_deref())
}

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
                allow_rule(&services, &input)
            },
        )
        .handle(
            "execPolicy.rules",
            |services: Arc<Services>, (): ()| async move { services.exec_policy.user_rules() },
        )
        .handle(
            "execPolicy.removeRule",
            |services: Arc<Services>, input: RemoveRuleInput| async move {
                services.exec_policy.remove_rule(&input.id)
            },
        )
        .handle(
            "execPolicy.confirmPack",
            |services: Arc<Services>, input: ConfirmRulePackInput| async move {
                services.exec_policy.confirm_pack(&input.pack)
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
