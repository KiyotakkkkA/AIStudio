use std::sync::Arc;

use zvs_contract::{AnswerFolderRequestInput, AppError, FolderGrantRef};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "folders.list",
            |services: Arc<Services>, _input: ()| async move { services.folders.list() },
        )
        .handle(
            "folders.revoke",
            |services: Arc<Services>, input: FolderGrantRef| async move {
                services.folders.revoke(&input.id)?;
                Ok::<(), AppError>(())
            },
        )
        .handle(
            "folders.answer",
            |services: Arc<Services>, input: AnswerFolderRequestInput| async move {
                services
                    .folders
                    .answer(&input.id, &input.approval_id, input.answer)?;
                Ok::<(), AppError>(())
            },
        );
}
