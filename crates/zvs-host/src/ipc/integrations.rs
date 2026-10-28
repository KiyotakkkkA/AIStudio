use std::sync::Arc;

use zvs_contract::{
    AppError, CreateIntegrationInput, IntegrationActivityInput, IntegrationRef,
    IntegrationRemoveOutput, UpdateIntegrationInput, WorkerRef,
};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "integrations.kinds",
            |services: Arc<Services>, _input: ()| async move {
                Ok::<_, AppError>(services.integrations.kinds())
            },
        )
        .handle(
            "integrations.list",
            |services: Arc<Services>, _input: ()| async move { services.integrations.list() },
        )
        .handle(
            "integrations.get",
            |services: Arc<Services>, input: IntegrationRef| async move {
                services.integrations.get(&input.id.to_string())
            },
        )
        .handle(
            "integrations.create",
            |services: Arc<Services>, input: CreateIntegrationInput| async move {
                services.integrations.create(&input).await
            },
        )
        .handle(
            "integrations.update",
            |services: Arc<Services>, input: UpdateIntegrationInput| async move {
                services.integrations.update(&input).await
            },
        )
        .handle(
            "integrations.remove",
            |services: Arc<Services>, input: IntegrationRef| async move {
                services.integrations.remove(&input.id.to_string()).await?;
                Ok::<IntegrationRemoveOutput, AppError>(IntegrationRemoveOutput {
                    id: input.id,
                    removed: true,
                })
            },
        )
        .handle(
            "integrations.test",
            |services: Arc<Services>, input: IntegrationRef| async move {
                services.integrations.test(&input.id.to_string()).await
            },
        )
        .handle(
            "integrations.enable",
            |services: Arc<Services>, input: IntegrationRef| async move {
                services.integrations.enable(&input.id.to_string()).await
            },
        )
        .handle(
            "integrations.disable",
            |services: Arc<Services>, input: IntegrationRef| async move {
                services.integrations.disable(&input.id.to_string()).await
            },
        )
        .handle(
            "integrations.activity",
            |services: Arc<Services>, input: IntegrationActivityInput| async move {
                services.integrations.activity(&input)
            },
        )
        .handle(
            "workers.list",
            |services: Arc<Services>, _input: ()| async move {
                Ok::<_, AppError>(services.workers.list())
            },
        )
        .handle(
            "workers.restart",
            |services: Arc<Services>, input: WorkerRef| async move {
                services.workers.restart(&input.key).await
            },
        );
}
