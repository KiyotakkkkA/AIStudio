#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use zvs_contract::{
    AddVectorEvalCaseInput, AddVectorSourceInput, AppError, AppErrorCode, CreateVectorStoreInput,
    GraphNode, PickVectorSourceInput, PickVectorSourceResult, RetryPolicy, RunGraph, RunHandleDto,
    RunKind, StartRunInput, UpdateVectorStoreInput, VectorChunksInput, VectorDocumentsRemoveInput,
    VectorDocumentsRemoveOutput, VectorEvalCaseRef, VectorEvalRemoveOutput, VectorEvalRunInput,
    VectorIndexInput, VectorSearchInput, VectorSourceRef, VectorSourcesRemoveOutput,
    VectorStoreRef, VectorStoresRemoveOutput,
};

use super::IpcServer;
use crate::services::vector_stores::VectorSearchOptions;
use crate::services::{RunService, Services, VectorStoreService};

pub const PICKER_UNAVAILABLE: &str = "Выбор файлов недоступен";

fn options(input: &VectorSearchInput) -> VectorSearchOptions {
    VectorSearchOptions {
        k: input.k,
        min_score: input.min_score,
        mode: input.mode,
        ..VectorSearchOptions::default()
    }
}

pub fn start_index_run(
    stores: &VectorStoreService,
    runs: &RunService,
    input: &VectorIndexInput,
) -> Result<RunHandleDto, AppError> {
    let store = stores.require_store(&input.store_id.to_string())?;
    let mut node = GraphNode::new("index", "vector.index");
    node.dependencies = Some(Vec::new());
    node.bindings = Some(BTreeMap::new());
    node.input = Some(json!({ "storeId": input.store_id, "full": input.full() }));
    node.retry = Some(RetryPolicy::default());
    let mut start = StartRunInput::new(
        RunKind::Indexing,
        RunGraph {
            nodes: vec![node],
            permission_scopes: None,
            allowed_tools: None,
        },
    );
    start.subject_id = Some(store.id.clone());
    start.title = Some(format!("Индексация «{}»", store.name));
    start.input = Some(Value::Null);
    start.concurrency = Some(1);
    runs.start(start)
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
            "vectorStores.index",
            |services: Arc<Services>, input: VectorIndexInput| async move {
                start_index_run(&services.vector_stores, &services.runs, &input)
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
