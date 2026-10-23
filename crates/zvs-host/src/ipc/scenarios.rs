use std::sync::Arc;

use zvs_contract::{
    AppError, CreateScenarioInput, ScenarioRef, ScenarioRemoveOutput, ScenarioValidateInput,
    ScenarioValidationDto, SetScenarioActiveInput, UpdateScenarioInput,
};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "scenarios.list",
            |services: Arc<Services>, _input: ()| async move { services.scenarios.list() },
        )
        .handle(
            "scenarios.get",
            |services: Arc<Services>, input: ScenarioRef| async move {
                services.scenarios.get(&input.id)
            },
        )
        .handle(
            "scenarios.create",
            |services: Arc<Services>, input: CreateScenarioInput| async move {
                services.scenarios.create(&input)
            },
        )
        .handle(
            "scenarios.update",
            |services: Arc<Services>, input: UpdateScenarioInput| async move {
                services.scenarios.update(&input)
            },
        )
        .handle(
            "scenarios.remove",
            |services: Arc<Services>, input: ScenarioRef| async move {
                services.scenarios.remove(&input.id)?;
                Ok::<ScenarioRemoveOutput, AppError>(ScenarioRemoveOutput {
                    id: input.id,
                    removed: true,
                })
            },
        )
        .handle(
            "scenarios.validate",
            |services: Arc<Services>, input: ScenarioValidateInput| async move {
                Ok::<ScenarioValidationDto, AppError>(services.scenarios.validate(&input.graph))
            },
        )
        .handle(
            "scenarios.activate",
            |services: Arc<Services>, input: SetScenarioActiveInput| async move {
                services.scenarios.set_active(&input.id, input.active)
            },
        )
        .handle(
            "scenarios.versions",
            |services: Arc<Services>, input: ScenarioRef| async move {
                services.scenarios.versions(&input.id)
            },
        );
}
