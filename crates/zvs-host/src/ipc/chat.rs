use std::sync::Arc;

use zvs_contract::{
    AppError, ChatContextInput, ChatSendInput, ConversationRef, CreateConversationInput,
    RenameConversationInput, RunIdInput, TruncateConversationInput,
    UpdateConversationRelationsInput, UpdateConversationStoresInput, UpdateConversationToolsInput,
};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "chat.conversations.list",
            |services: Arc<Services>, _input: ()| async move { services.chat.list() },
        )
        .handle(
            "chat.conversations.get",
            |services: Arc<Services>, input: ConversationRef| async move {
                services.chat.get(&input.id)
            },
        )
        .handle(
            "chat.conversations.create",
            |services: Arc<Services>, input: CreateConversationInput| async move {
                services.chat.create(&input)
            },
        )
        .handle(
            "chat.conversations.remove",
            |services: Arc<Services>, input: ConversationRef| async move {
                services.chat.remove(&input.id)
            },
        )
        .handle(
            "chat.conversations.rename",
            |services: Arc<Services>, input: RenameConversationInput| async move {
                services.chat.rename(&input.id, input.title())
            },
        )
        .handle(
            "chat.conversations.updateStores",
            |services: Arc<Services>, input: UpdateConversationStoresInput| async move {
                services
                    .chat
                    .update_stores(&input.id, &input.attached_store_ids)
            },
        )
        .handle(
            "chat.conversations.updateRelations",
            |services: Arc<Services>, input: UpdateConversationRelationsInput| async move {
                services
                    .chat
                    .update_relations(&input.id, input.use_relations)
            },
        )
        .handle(
            "chat.conversations.updateTools",
            |services: Arc<Services>, input: UpdateConversationToolsInput| async move {
                services
                    .chat
                    .update_tools(&input.id, &input.enabled_tools())
            },
        )
        .handle(
            "chat.conversations.truncate",
            |services: Arc<Services>, input: TruncateConversationInput| async move {
                services.chat.truncate(&input.id, &input.message_id)
            },
        )
        .handle(
            "chat.context",
            |services: Arc<Services>, input: ChatContextInput| async move {
                services.chat.context(&input)
            },
        )
        .handle(
            "chat.send",
            |services: Arc<Services>, input: ChatSendInput| async move { services.chat.send(&input) },
        )
        .handle(
            "chat.cancel",
            |services: Arc<Services>, input: RunIdInput| async move {
                services.chat.cancel(&input.id).await?;
                Ok::<(), AppError>(())
            },
        );
}

#[cfg(test)]
mod tests;
