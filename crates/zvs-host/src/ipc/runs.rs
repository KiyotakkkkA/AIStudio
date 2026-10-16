use std::sync::Arc;

use zvs_contract::{RunIdInput, RunListFilter, RunsApproveInput, RunsDenyInput, StartRunInput};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "runs.approve",
            |services: Arc<Services>, input: RunsApproveInput| async move {
                services
                    .runs
                    .approve(&input.id, &input.approval_id, input.always())
            },
        )
        .handle(
            "runs.deny",
            |services: Arc<Services>, input: RunsDenyInput| async move {
                services.runs.deny(&input.id, &input.approval_id)
            },
        )
        .handle(
            "runs.start",
            |services: Arc<Services>, input: StartRunInput| async move {
                services.runs.start(input)
            },
        )
        .handle(
            "runs.cancel",
            |services: Arc<Services>, input: RunIdInput| async move {
                services.runs.cancel(&input.id).await
            },
        )
        .handle(
            "runs.list",
            |services: Arc<Services>, input: RunListFilter| async move {
                services.runs.list(&input)
            },
        )
        .handle(
            "runs.get",
            |services: Arc<Services>, input: RunIdInput| async move { services.runs.get(&input.id) },
        )
        .handle(
            "runs.steps",
            |services: Arc<Services>, input: RunIdInput| async move {
                services.runs.steps(&input.id)
            },
        )
        .handle(
            "runs.detail",
            |services: Arc<Services>, input: RunIdInput| async move {
                services.runs.detail(&input.id)
            },
        )
        .handle(
            "runs.retry",
            |services: Arc<Services>, input: RunIdInput| async move {
                services.runs.retry(&input.id)
            },
        )
        .handle(
            "runs.clearFinished",
            |services: Arc<Services>, _input: ()| async move { services.runs.clear_finished() },
        );
}

#[cfg(test)]
mod tests;
