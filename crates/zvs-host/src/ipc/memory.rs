use std::sync::Arc;

use zvs_contract::{AppError, ClearMemoryInput, CreateMemoryInput, MemoryRef, UpdateMemoryInput};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "memory.list",
            |services: Arc<Services>, _input: ()| async move { services.memory.list() },
        )
        .handle(
            "memory.create",
            |services: Arc<Services>, input: CreateMemoryInput| async move {
                services.memory.create(&input)
            },
        )
        .handle(
            "memory.update",
            |services: Arc<Services>, input: UpdateMemoryInput| async move {
                services.memory.update(&input)
            },
        )
        .handle(
            "memory.remove",
            |services: Arc<Services>, input: MemoryRef| async move {
                services.memory.remove(&input.id.to_string())?;
                Ok::<(), AppError>(())
            },
        )
        .handle(
            "memory.clear",
            |services: Arc<Services>, input: ClearMemoryInput| async move {
                services.memory.clear(&input)?;
                Ok::<(), AppError>(())
            },
        );
}
