use std::sync::Arc;

use zvs_contract::{AppError, ConversationRef, ExplorerListInput, ExplorerReadInput};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "workspace.list",
            |services: Arc<Services>, input: ExplorerListInput| async move {
                services.explorer.list(&input.id, &input.path)
            },
        )
        .handle(
            "workspace.read",
            |services: Arc<Services>, input: ExplorerReadInput| async move {
                services.explorer.read(&input.id, &input.path)
            },
        )
        .handle(
            "workspace.watch",
            |services: Arc<Services>, input: ConversationRef| async move {
                services.explorer.watch(&input.id)
            },
        )
        .handle(
            "workspace.unwatch",
            |services: Arc<Services>, input: ConversationRef| async move {
                services.explorer.unwatch(&input.id);
                Ok::<(), AppError>(())
            },
        );
}
