use std::sync::Arc;

use zvs_contract::{AppError, ShellRef, ShellsListInput, ShellsOutputInput};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "shells.list",
            |services: Arc<Services>, input: ShellsListInput| async move {
                Ok::<_, AppError>(services.shells.list(&input.owner_id))
            },
        )
        .handle(
            "shells.output",
            |services: Arc<Services>, input: ShellsOutputInput| async move {
                services.shells.output(None, &input.id, input.since_seq)
            },
        )
        .handle(
            "shells.kill",
            |services: Arc<Services>, input: ShellRef| async move {
                services.shells.kill(None, &input.id).await
            },
        );
}
