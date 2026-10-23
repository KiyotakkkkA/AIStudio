use std::sync::Arc;

use zvs_contract::{
    AppError, ConversationRef, TerminalCreateInput, TerminalRef, TerminalRenameInput,
    TerminalResizeInput, TerminalWriteInput,
};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "terminal.shells",
            |services: Arc<Services>, _input: ()| async move {
                Ok::<_, AppError>(services.terminals.shells())
            },
        )
        .handle(
            "terminal.list",
            |services: Arc<Services>, input: ConversationRef| async move {
                Ok::<_, AppError>(services.terminals.list(&input.id))
            },
        )
        .handle(
            "terminal.create",
            |services: Arc<Services>, input: TerminalCreateInput| async move {
                services.terminals.create(&input)
            },
        )
        .handle(
            "terminal.attach",
            |services: Arc<Services>, input: TerminalRef| async move {
                services.terminals.attach(&input.id)
            },
        )
        .handle(
            "terminal.write",
            |services: Arc<Services>, input: TerminalWriteInput| async move {
                services.terminals.write(&input.id, &input.data)
            },
        )
        .handle(
            "terminal.resize",
            |services: Arc<Services>, input: TerminalResizeInput| async move {
                services.terminals.resize(&input.id, input.cols, input.rows)
            },
        )
        .handle(
            "terminal.rename",
            |services: Arc<Services>, input: TerminalRenameInput| async move {
                services.terminals.rename(&input.id, &input.title)
            },
        )
        .handle(
            "terminal.restart",
            |services: Arc<Services>, input: TerminalRef| async move {
                services.terminals.restart(&input.id)
            },
        )
        .handle(
            "terminal.clear",
            |services: Arc<Services>, input: TerminalRef| async move {
                services.terminals.clear(&input.id)
            },
        )
        .handle(
            "terminal.close",
            |services: Arc<Services>, input: TerminalRef| async move {
                services.terminals.close(&input.id)
            },
        );
}
