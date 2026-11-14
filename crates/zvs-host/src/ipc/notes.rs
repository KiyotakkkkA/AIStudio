use std::sync::Arc;

use zvs_contract::{
    AppError, ConversationRef, NoteReadInput, NoteRef, NoteRenameInput, NoteSaveInput,
};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "notes.list",
            |services: Arc<Services>, input: ConversationRef| async move {
                Ok::<_, AppError>(services.notes.list(&input.id))
            },
        )
        .handle(
            "notes.read",
            |services: Arc<Services>, input: NoteReadInput| async move {
                services.notes.read(&input)
            },
        )
        .handle(
            "notes.save",
            |services: Arc<Services>, input: NoteSaveInput| async move {
                services.notes.save(&input)
            },
        )
        .handle(
            "notes.rename",
            |services: Arc<Services>, input: NoteRenameInput| async move {
                services.notes.rename(&input)
            },
        )
        .handle(
            "notes.remove",
            |services: Arc<Services>, input: NoteRef| async move { services.notes.remove(&input) },
        );
}
