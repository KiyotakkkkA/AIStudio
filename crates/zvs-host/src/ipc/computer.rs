use std::sync::Arc;

use zvs_contract::{AnswerAppRequestInput, AppError, AppGrantRef};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "computer.grants",
            |services: Arc<Services>, _input: ()| async move { services.computer.app_grants() },
        )
        .handle(
            "computer.revoke",
            |services: Arc<Services>, input: AppGrantRef| async move {
                services.computer.revoke(&input.id)?;
                Ok::<(), AppError>(())
            },
        )
        .handle(
            "computer.answer",
            |services: Arc<Services>, input: AnswerAppRequestInput| async move {
                services.computer.answer(
                    &input.id.to_string(),
                    &input.approval_id,
                    input.answer,
                )?;
                Ok::<(), AppError>(())
            },
        );
}
