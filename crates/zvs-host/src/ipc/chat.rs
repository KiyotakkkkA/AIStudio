use std::sync::Arc;

use zvs_contract::{
    AppError, AppErrorCode, BindFolderInput, ChatAnswerInput, ChatContextInput, ChatEvalInput,
    ChatSendInput, ChatSkipInput, ConversationRef, CreateConversationInput, PickFolderResultDto,
    RenameConversationInput, RunIdInput, TruncateConversationInput,
    UpdateConversationRelationsInput, UpdateConversationRetrievalInput,
    UpdateConversationStoresInput, UpdateConversationToolsInput, VectorSourceKind,
};

use super::IpcServer;
use super::vector_stores::PICKER_UNAVAILABLE;
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
                services.chat.remove(&input.id)?;
                services.terminals.close_conversation(&input.id);
                crate::chat_files::remove_chat_dir(&services.artifacts, &input.id.to_string());
                Ok::<(), zvs_contract::AppError>(())
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
            "chat.evaluate",
            |services: Arc<Services>, input: ChatEvalInput| async move {
                services.chat.evaluate(&input).await
            },
        )
        .handle(
            "chat.conversations.updateRetrieval",
            |services: Arc<Services>, input: UpdateConversationRetrievalInput| async move {
                services
                    .chat
                    .update_retrieval(&input.id, input.retrieval_mode)
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
            "chat.pickFolder",
            |services: Arc<Services>, _input: ()| async move {
                let picker = services
                    .picker
                    .clone()
                    .ok_or_else(|| AppError::new(AppErrorCode::Conflict, PICKER_UNAVAILABLE))?;
                Ok::<_, AppError>(PickFolderResultDto {
                    path: picker
                        .pick(VectorSourceKind::Folder)
                        .await?
                        .into_iter()
                        .next(),
                })
            },
        )
        .handle(
            "chat.bindFolder",
            |services: Arc<Services>, input: BindFolderInput| async move {
                let result = services.chat.bind_folder(&input)?;
                services.explorer.rebind(&input.id);
                Ok::<_, AppError>(result)
            },
        )
        .handle(
            "chat.unbindFolder",
            |services: Arc<Services>, input: ConversationRef| async move {
                let conversation = services.chat.unbind_folder(&input.id)?;
                services.explorer.rebind(&input.id);
                Ok::<_, AppError>(conversation)
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
        )
        .handle(
            "chat.answer",
            |services: Arc<Services>, input: ChatAnswerInput| async move {
                services.runs.questions().answer(
                    &input.id.to_string(),
                    &input.request_id,
                    &input.answers,
                )
            },
        )
        .handle(
            "chat.skip",
            |services: Arc<Services>, input: ChatSkipInput| async move {
                services
                    .runs
                    .questions()
                    .skip(&input.id.to_string(), &input.request_id)
            },
        );
}

#[cfg(test)]
mod tests;
