use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use futures::FutureExt;
use tokio_util::sync::CancellationToken;
use zvs_contract::AppError;
use zvs_core::graph::GraphStore;
use zvs_core::index::{SearchMode, VectorIndex, literal};
use zvs_core::{chunk, hash};

use super::graph::{self as validate, GraphRegistry, required};
use super::{
    ChunkConfig, GraphContent, GraphCore, GraphEdge, GraphNeighbor, GraphNode, GraphPath,
    GraphStats, GraphSubgraph, NeighborQuery, PathQuery, SectionChunk, SubgraphQuery, TextChunk,
    TextCore, TextSegment, VectorChunk, VectorCore, VectorHit, VectorMetric, VectorRow,
    VectorSearchRequest, VectorStats, cancelled_error, core_error, native_number, panic_error,
};

pub(super) async fn blocking<T, F>(work: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    match tokio::task::spawn_blocking(move || catch_unwind(AssertUnwindSafe(work))).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) | Err(_) => Err(panic_error()),
    }
}

pub(super) async fn guarded<T>(
    work: impl Future<Output = zvs_core::Result<T>> + Send,
) -> Result<T, AppError> {
    match AssertUnwindSafe(work).catch_unwind().await {
        Ok(result) => result.map_err(|error| core_error(&error)),
        Err(_) => Err(panic_error()),
    }
}

fn core<T>(result: zvs_core::Result<T>) -> Result<T, AppError> {
    result.map_err(|error| core_error(&error))
}

fn core_config(config: ChunkConfig) -> chunk::ChunkConfig {
    chunk::ChunkConfig {
        size: config.size,
        overlap: config.overlap,
    }
}

#[derive(Debug, Default)]
pub struct InProcessCore {
    graphs: Arc<GraphRegistry>,
}

impl InProcessCore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    pub fn graphs(&self) -> &GraphRegistry {
        &self.graphs
    }

    #[cfg(test)]
    pub(super) fn graphs_handle(&self) -> Arc<GraphRegistry> {
        Arc::clone(&self.graphs)
    }

    async fn on_graph<T, F>(&self, graph_id: &str, work: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&GraphRegistry, &str) -> zvs_core::Result<T> + Send + 'static,
    {
        let graph_id = required(graph_id)?;
        let graphs = Arc::clone(&self.graphs);
        blocking(move || core(work(&graphs, &graph_id))).await
    }
}

#[cfg(feature = "debug-panic")]
impl InProcessCore {
    pub async fn debug_panic(&self) -> Result<(), AppError> {
        blocking(|| -> Result<(), AppError> { panic!("deliberate native panic") }).await
    }

    pub async fn graph_debug_panic(&self, graph_id: &str) -> Result<(), AppError> {
        self.on_graph(graph_id, |graphs, id| {
            graphs.write::<()>(id, |_| panic!("deliberate native graph panic"))
        })
        .await
    }

    pub async fn vector_debug_panic(&self) -> Result<(), AppError> {
        guarded(async { panic!("deliberate native vector panic") }).await
    }
}

#[async_trait]
impl TextCore for InProcessCore {
    async fn chunk(&self, text: String, config: ChunkConfig) -> Result<Vec<TextChunk>, AppError> {
        let config = config.validate()?;
        blocking(move || {
            core(chunk::chunk_text(
                &text,
                core_config(config),
                &CancellationToken::new(),
            ))?
            .into_iter()
            .map(|chunk| {
                Ok(TextChunk {
                    text: chunk.text.to_owned(),
                    byte_start: native_number(chunk.byte_range.start)?,
                    byte_end: native_number(chunk.byte_range.end)?,
                    token_count: native_number(chunk.token_count)?,
                })
            })
            .collect()
        })
        .await
    }

    async fn chunk_segments(
        &self,
        segments: Vec<TextSegment>,
        config: ChunkConfig,
    ) -> Result<Vec<SectionChunk>, AppError> {
        let config = config.validate()?;
        blocking(move || {
            let segments: Vec<chunk::Segment> = segments
                .into_iter()
                .map(|segment| chunk::Segment {
                    text: segment.text,
                    section: segment.section,
                    page: segment.page,
                })
                .collect();
            core(chunk::chunk_segments(
                &segments,
                core_config(config),
                &CancellationToken::new(),
            ))?
            .into_iter()
            .map(|chunk| {
                Ok(SectionChunk {
                    text: chunk.text,
                    byte_start: native_number(chunk.byte_range.start)?,
                    byte_end: native_number(chunk.byte_range.end)?,
                    token_count: native_number(chunk.token_count)?,
                    section: chunk.section,
                    page: chunk.page,
                })
            })
            .collect()
        })
        .await
    }

    async fn hash(&self, bytes: Vec<u8>) -> Result<String, AppError> {
        blocking(move || core(hash::content_hash(&bytes, &CancellationToken::new()))).await
    }
}

#[async_trait]
impl VectorCore for InProcessCore {
    async fn create_index(
        &self,
        path: &Path,
        dimension: u32,
        metric: VectorMetric,
    ) -> Result<VectorStats, AppError> {
        guarded(async {
            VectorIndex::create(path, dimension, metric)
                .await?
                .stats()
                .await
        })
        .await
    }

    async fn open_index(&self, path: &Path) -> Result<VectorStats, AppError> {
        guarded(async { VectorIndex::open(path).await?.stats().await }).await
    }

    async fn upsert(
        &self,
        path: &Path,
        rows: Vec<VectorRow>,
        token: &CancellationToken,
    ) -> Result<usize, AppError> {
        if token.is_cancelled() {
            return Err(cancelled_error());
        }
        guarded(async { VectorIndex::open(path).await?.upsert(&rows, token).await }).await
    }

    async fn search(
        &self,
        path: &Path,
        request: VectorSearchRequest,
    ) -> Result<Vec<VectorHit>, AppError> {
        guarded(async {
            let index = VectorIndex::open(path).await?;
            let scope = request.document_ids.map(|ids| {
                format!(
                    "document_id IN ({})",
                    ids.iter()
                        .map(|id| literal(id))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            });
            let filter = match (request.filter, scope) {
                (Some(filter), Some(scope)) => Some(format!("({filter}) AND {scope}")),
                (filter, scope) => filter.or(scope),
            };
            match (request.mode.unwrap_or_default(), request.query) {
                (SearchMode::Hybrid, Some(query)) => {
                    index
                        .hybrid_search(
                            &request.vector,
                            &query,
                            request.k,
                            request.min_score,
                            filter.as_deref(),
                        )
                        .await
                }
                _ => {
                    index
                        .search(
                            &request.vector,
                            request.k,
                            request.min_score,
                            filter.as_deref(),
                        )
                        .await
                }
            }
        })
        .await
    }

    async fn chunks(
        &self,
        path: &Path,
        document_id: &str,
        indices: &[u32],
    ) -> Result<Vec<VectorChunk>, AppError> {
        guarded(async {
            VectorIndex::open(path)
                .await?
                .chunks(document_id, indices)
                .await
        })
        .await
    }

    async fn maintain(&self, path: &Path, ann_threshold: usize) -> Result<VectorStats, AppError> {
        guarded(async { VectorIndex::open(path).await?.maintain(ann_threshold).await }).await
    }

    async fn delete_by_ids(&self, path: &Path, ids: &[String]) -> Result<(), AppError> {
        guarded(async { VectorIndex::open(path).await?.delete_by_ids(ids).await }).await
    }

    async fn delete_by_source(&self, path: &Path, document_id: &str) -> Result<(), AppError> {
        guarded(async {
            VectorIndex::open(path)
                .await?
                .delete_by_source(document_id)
                .await
        })
        .await
    }

    async fn stats(&self, path: &Path) -> Result<VectorStats, AppError> {
        guarded(async { VectorIndex::open(path).await?.stats().await }).await
    }

    async fn remove_index(&self, path: &Path) -> Result<(), AppError> {
        let path = path.to_path_buf();
        blocking(move || core(VectorIndex::remove(&path))).await
    }
}

#[async_trait]
impl GraphCore for InProcessCore {
    async fn hydrate(&self, graph_id: &str, content: GraphContent) -> Result<GraphStats, AppError> {
        let content = validate::content(content)?;
        let graph_id = required(graph_id)?;
        let graphs = Arc::clone(&self.graphs);
        blocking(move || {
            let store = core(GraphStore::hydrate(
                &content.nodes,
                &content.edges,
                &CancellationToken::new(),
            ))?;
            let stats = store.stats();
            graphs.replace(graph_id, store);
            Ok(stats)
        })
        .await
    }

    async fn graph_stats(&self, graph_id: &str) -> Result<GraphStats, AppError> {
        self.on_graph(graph_id, |graphs, id| {
            graphs.read(id, |store| Ok(store.stats()))
        })
        .await
    }

    async fn add_node(&self, graph_id: &str, node: GraphNode) -> Result<bool, AppError> {
        let node = validate::node(node)?;
        self.on_graph(graph_id, move |graphs, id| {
            graphs.write(id, |store| store.add_node(node))
        })
        .await
    }

    async fn add_edge(&self, graph_id: &str, edge: GraphEdge) -> Result<bool, AppError> {
        let edge = validate::edge(edge)?;
        self.on_graph(graph_id, move |graphs, id| {
            graphs.write(id, |store| store.add_edge(edge))
        })
        .await
    }

    async fn remove_edge(
        &self,
        graph_id: &str,
        from: &str,
        to: &str,
        relation: &str,
    ) -> Result<bool, AppError> {
        let (from, to, relation) = (required(from)?, required(to)?, required(relation)?);
        self.on_graph(graph_id, move |graphs, id| {
            graphs.write(id, |store| Ok(store.remove_edge(&from, &to, &relation)))
        })
        .await
    }

    async fn remove_node(&self, graph_id: &str, node_id: &str) -> Result<Option<usize>, AppError> {
        let node_id = required(node_id)?;
        self.on_graph(graph_id, move |graphs, id| {
            graphs.write(id, |store| Ok(store.remove_node(&node_id)))
        })
        .await
    }

    async fn neighbors(
        &self,
        graph_id: &str,
        query: NeighborQuery,
    ) -> Result<Vec<GraphNeighbor>, AppError> {
        let query = validate::neighbor_query(query)?;
        self.on_graph(graph_id, move |graphs, id| {
            graphs.read(id, |store| {
                store.neighbors(&query.id, query.relation.as_deref(), query.direction)
            })
        })
        .await
    }

    async fn shortest_path(
        &self,
        graph_id: &str,
        query: PathQuery,
    ) -> Result<Option<GraphPath>, AppError> {
        let query = validate::path_query(query)?;
        self.on_graph(graph_id, move |graphs, id| {
            graphs.read(id, |store| {
                store.shortest_path(
                    &query.from,
                    &query.to,
                    query.relation.as_deref(),
                    query.direction,
                    &CancellationToken::new(),
                )
            })
        })
        .await
    }

    async fn k_hop_subgraph(
        &self,
        graph_id: &str,
        query: SubgraphQuery,
    ) -> Result<GraphSubgraph, AppError> {
        let query = validate::subgraph_query(query)?;
        self.on_graph(graph_id, move |graphs, id| {
            graphs.read(id, |store| {
                store.k_hop_subgraph(
                    &query.id,
                    query.hops,
                    query.relation.as_deref(),
                    query.direction,
                    query.budget,
                    &CancellationToken::new(),
                )
            })
        })
        .await
    }
}
