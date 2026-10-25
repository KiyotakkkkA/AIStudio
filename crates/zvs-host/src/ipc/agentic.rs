use std::sync::Arc;

use zvs_contract::{
    AgenticInstructInput, AgenticSessionRef, AppError, CreateAgenticSessionInput,
    ExtendAgenticSessionInput, SetAgenticModeInput, UpdateAgenticPlanInput,
};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "agentic.list",
            |services: Arc<Services>, _input: ()| async move { services.agentic.list() },
        )
        .handle(
            "agentic.get",
            |services: Arc<Services>, input: AgenticSessionRef| async move {
                services.agentic.get(&input.id.to_string())
            },
        )
        .handle(
            "agentic.create",
            |services: Arc<Services>, input: CreateAgenticSessionInput| async move {
                services.agentic.create(&input)
            },
        )
        .handle(
            "agentic.updatePlan",
            |services: Arc<Services>, input: UpdateAgenticPlanInput| async move {
                services.agentic.update_plan(&input)
            },
        )
        .handle(
            "agentic.setMode",
            |services: Arc<Services>, input: SetAgenticModeInput| async move {
                services.agentic.set_mode(&input)
            },
        )
        .handle(
            "agentic.instruct",
            |services: Arc<Services>, input: AgenticInstructInput| async move {
                services.agentic.instruct(&input)
            },
        )
        .handle(
            "agentic.extend",
            |services: Arc<Services>, input: ExtendAgenticSessionInput| async move {
                services.agentic.extend(&input)
            },
        )
        .handle(
            "agentic.cancel",
            |services: Arc<Services>, input: AgenticSessionRef| async move {
                services.agentic.cancel(&input.id.to_string()).await?;
                Ok::<(), AppError>(())
            },
        )
        .handle(
            "agentic.remove",
            |services: Arc<Services>, input: AgenticSessionRef| async move {
                services.agentic.remove(&input.id.to_string())?;
                Ok::<(), AppError>(())
            },
        );
}
