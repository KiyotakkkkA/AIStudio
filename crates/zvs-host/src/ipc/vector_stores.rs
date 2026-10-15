#[cfg(test)]
mod tests;

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AddVectorEvalCaseInput, AddVectorSourceInput, AppError, AppErrorCode, CreateVectorStoreInput,
    PickVectorSourceInput, PickVectorSourceResult, UpdateVectorStoreInput, VectorChunksInput,
    VectorDocumentsRemoveInput, VectorDocumentsRemoveOutput, VectorEvalCaseRef,
    VectorEvalRemoveOutput, VectorEvalRunInput, VectorSearchInput, VectorSourceRef,
    VectorSourcesRemoveOutput, VectorStoreRef, VectorStoresRemoveOutput,
};

use super::IpcServer;
use crate::services::Services;
use crate::services::vector_stores::VectorSearchOptions;

pub const PICKER_UNAVAILABLE: &str = "Выбор файлов недоступен";

fn options(input: &VectorSearchInput) -> VectorSearchOptions {
    VectorSearchOptions {
        k: input.k,
        min_score: input.min_score,
        mode: input.mode,
        ..VectorSearchOptions::default()
    }
}

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "vectorStores.list",
            |services: Arc<Services>, _input: ()| async move { services.vector_stores.list().await },
        )
        .handle(
            "vectorStores.get",
            |services: Arc<Services>, input: VectorStoreRef| async move {
                services.vector_stores.get(&input.id.to_string()).await
            },
        )
        .handle(
            "vectorStores.create",
            |services: Arc<Services>, input: CreateVectorStoreInput| async move {
                services.vector_stores.create(&input).await
            },
        )
        .handle(
            "vectorStores.update",
            |services: Arc<Services>, input: UpdateVectorStoreInput| async move {
                services.vector_stores.update(&input).await
            },
        )
        .handle(
            "vectorStores.remove",
            |services: Arc<Services>, input: VectorStoreRef| async move {
                services.vector_stores.remove(&input.id.to_string()).await?;
                Ok(VectorStoresRemoveOutput {
                    id: input.id,
                    removed: true,
                })
            },
        )
        .handle(
            "vectorStores.search",
            |services: Arc<Services>, input: VectorSearchInput| async move {
                services
                    .vector_stores
                    .search(
                        &input.store_id.to_string(),
                        &input.query,
                        options(&input),
                        &CancellationToken::new(),
                    )
                    .await
            },
        )
        .handle(
            "vectorStores.searchTimed",
            |services: Arc<Services>, input: VectorSearchInput| async move {
                services
                    .vector_stores
                    .search_timed(
                        &input.store_id.to_string(),
                        &input.query,
                        options(&input),
                        &CancellationToken::new(),
                    )
                    .await
            },
        )
        .handle(
            "vectorStores.chunks",
            |services: Arc<Services>, input: VectorChunksInput| async move {
                services
                    .vector_stores
                    .chunks(
                        &input.store_id.to_string(),
                        &input.document_id,
                        &input.indices,
                    )
                    .await
            },
        )
        .handle(
            "vectorStores.clear",
            |services: Arc<Services>, input: VectorStoreRef| async move {
                services.vector_stores.clear(&input.id.to_string()).await
            },
        )
        .handle(
            "vectorStores.reconcile",
            |services: Arc<Services>, input: VectorStoreRef| async move {
                services.vector_stores.reconcile(&input.id.to_string()).await
            },
        )
        .handle(
            "vectorStores.sources.list",
            |services: Arc<Services>, input: VectorStoreRef| async move {
                services.vector_sources.list_sources(&input.id.to_string())
            },
        )
        .handle(
            "vectorStores.sources.add",
            |services: Arc<Services>, input: AddVectorSourceInput| async move {
                services.vector_sources.add_source(&input)
            },
        )
        .handle(
            "vectorStores.sources.remove",
            |services: Arc<Services>, input: VectorSourceRef| async move {
                services.vector_sources.remove_source(&input.id)?;
                Ok(VectorSourcesRemoveOutput {
                    id: input.id,
                    removed: true,
                })
            },
        )
        .handle(
            "vectorStores.sources.pick",
            |services: Arc<Services>, input: PickVectorSourceInput| async move {
                let picker = services
                    .picker
                    .clone()
                    .ok_or_else(|| AppError::new(AppErrorCode::Conflict, PICKER_UNAVAILABLE))?;
                Ok(PickVectorSourceResult {
                    paths: picker.pick(input.kind).await?,
                })
            },
        )
        .handle(
            "vectorStores.documents.list",
            |services: Arc<Services>, input: VectorStoreRef| async move {
                services.vector_sources.list_documents(&input.id.to_string())
            },
        )
        .handle(
            "vectorStores.documents.remove",
            |services: Arc<Services>, input: VectorDocumentsRemoveInput| async move {
                services
                    .vector_sources
                    .remove_document(&input.store_id.to_string(), &input.id.to_string())
                    .await?;
                Ok(VectorDocumentsRemoveOutput {
                    id: input.id,
                    removed: true,
                })
            },
        )
        .handle(
            "vectorStores.eval.list",
            |services: Arc<Services>, input: VectorStoreRef| async move {
                services.vector_eval.list(&input.id.to_string())
            },
        )
        .handle(
            "vectorStores.eval.add",
            |services: Arc<Services>, input: AddVectorEvalCaseInput| async move {
                services.vector_eval.add(&input)
            },
        )
        .handle(
            "vectorStores.eval.remove",
            |services: Arc<Services>, input: VectorEvalCaseRef| async move {
                services
                    .vector_eval
                    .remove(&input.store_id.to_string(), &input.id.to_string())?;
                Ok(VectorEvalRemoveOutput {
                    id: input.id,
                    removed: true,
                })
            },
        )
        .handle(
            "vectorStores.eval.run",
            |services: Arc<Services>, input: VectorEvalRunInput| async move {
                services
                    .vector_eval
                    .run(&input, &CancellationToken::new())
                    .await
            },
        );
}
