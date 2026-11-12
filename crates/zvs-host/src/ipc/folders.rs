use std::sync::Arc;

use zvs_contract::{AnswerFolderRequestInput, AppError, FolderGrantRef};

use super::IpcServer;
use super::caller::{Caller, granted_by};
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
        .handle_as(
            "folders.answer",
            |services: Arc<Services>, caller: Caller, input: AnswerFolderRequestInput| async move {
                services.folders.answer_as(
                    &input.id,
                    &input.approval_id,
                    input.answer,
                    granted_by(caller),
                )?;
                Ok::<(), AppError>(())
            },
        );
}
