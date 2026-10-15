mod graph;
mod in_process;

#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use zvs_contract::{AppError, AppErrorCode};

pub use self::graph::{GraphRegistry, NeighborQuery, PathQuery, SubgraphQuery};
pub use self::in_process::InProcessCore;
pub use zvs_core::graph::{
    Direction as GraphDirection, Edge as GraphEdge, GraphPath, GraphStats, MAX_HOPS,
    MAX_NODE_BUDGET, Neighbor as GraphNeighbor, Node as GraphNode, NodeKind as GraphNodeKind,
    Subgraph as GraphSubgraph, SubgraphNode as GraphSubgraphNode,
};
pub use zvs_core::index::{
    IndexStats as VectorStats, Metric as VectorMetric, Provenance as VectorProvenance,
    SearchHit as VectorHit, SearchMode as VectorSearchMode, StoredChunk as VectorChunk, VectorRow,
};

pub const CHUNK_CONFIG_MESSAGE: &str = "Некорректный размер или перекрытие фрагментов";
pub const GRAPH_REQUEST_MESSAGE: &str = "Некорректный запрос к графу";
pub const CANCELLED_MESSAGE: &str = "Операция отменена";
pub const PANIC_MESSAGE: &str = "Rust operation panicked";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkConfig {
    pub size: usize,
    pub overlap: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            size: 256,
            overlap: 32,
        }
    }
}

impl ChunkConfig {
    pub fn validate(self) -> Result<Self, AppError> {
        let valid = self.size > 0 && u32::try_from(self.size).is_ok() && self.overlap < self.size;
        if valid {
            Ok(self)
        } else {
            Err(AppError::new(
                AppErrorCode::ValidationFailed,
                CHUNK_CONFIG_MESSAGE,
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChunk {
    pub text: String,
    pub byte_start: u32,
    pub byte_end: u32,
    pub token_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSegment {
    pub text: String,
    pub section: Vec<String>,
    pub page: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionChunk {
    pub text: String,
    pub byte_start: u32,
    pub byte_end: u32,
    pub token_count: u32,
    pub section: Vec<String>,
    pub page: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VectorSearchRequest {
    pub vector: Vec<f32>,
    pub k: usize,
    pub min_score: f32,
    pub mode: Option<VectorSearchMode>,
    pub query: Option<String>,
    pub document_ids: Option<Vec<String>>,
    pub filter: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct GraphContent {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[async_trait]
pub trait TextCore: Send + Sync {
    async fn chunk(&self, text: String, config: ChunkConfig) -> Result<Vec<TextChunk>, AppError>;

    async fn chunk_segments(
        &self,
        segments: Vec<TextSegment>,
        config: ChunkConfig,
    ) -> Result<Vec<SectionChunk>, AppError>;

    async fn hash(&self, bytes: Vec<u8>) -> Result<String, AppError>;
}

#[async_trait]
pub trait VectorCore: Send + Sync {
    async fn create_index(
        &self,
        path: &Path,
        dimension: u32,
        metric: VectorMetric,
    ) -> Result<VectorStats, AppError>;

    async fn open_index(&self, path: &Path) -> Result<VectorStats, AppError>;

    async fn upsert(
        &self,
        path: &Path,
        rows: Vec<VectorRow>,
        token: &CancellationToken,
    ) -> Result<usize, AppError>;

    async fn search(
        &self,
        path: &Path,
        request: VectorSearchRequest,
    ) -> Result<Vec<VectorHit>, AppError>;

    async fn chunks(
        &self,
        path: &Path,
        document_id: &str,
        indices: &[u32],
    ) -> Result<Vec<VectorChunk>, AppError>;

    async fn maintain(&self, path: &Path, ann_threshold: usize) -> Result<VectorStats, AppError>;

    async fn delete_by_ids(&self, path: &Path, ids: &[String]) -> Result<(), AppError>;

    async fn delete_by_source(&self, path: &Path, document_id: &str) -> Result<(), AppError>;

    async fn stats(&self, path: &Path) -> Result<VectorStats, AppError>;

    async fn remove_index(&self, path: &Path) -> Result<(), AppError>;
}

#[async_trait]
pub trait GraphCore: Send + Sync {
    async fn hydrate(&self, graph_id: &str, content: GraphContent) -> Result<GraphStats, AppError>;

    async fn graph_stats(&self, graph_id: &str) -> Result<GraphStats, AppError>;

    async fn add_node(&self, graph_id: &str, node: GraphNode) -> Result<bool, AppError>;

    async fn add_edge(&self, graph_id: &str, edge: GraphEdge) -> Result<bool, AppError>;

    async fn remove_edge(
        &self,
        graph_id: &str,
        from: &str,
        to: &str,
        relation: &str,
    ) -> Result<bool, AppError>;

    async fn remove_node(&self, graph_id: &str, node_id: &str) -> Result<Option<usize>, AppError>;

    async fn neighbors(
        &self,
        graph_id: &str,
        query: NeighborQuery,
    ) -> Result<Vec<GraphNeighbor>, AppError>;

    async fn shortest_path(
        &self,
        graph_id: &str,
        query: PathQuery,
    ) -> Result<Option<GraphPath>, AppError>;

    async fn k_hop_subgraph(
        &self,
        graph_id: &str,
        query: SubgraphQuery,
    ) -> Result<GraphSubgraph, AppError>;
}

pub type SharedTextCore = Arc<dyn TextCore>;
pub type SharedVectorCore = Arc<dyn VectorCore>;
pub type SharedGraphCore = Arc<dyn GraphCore>;

pub fn core_error(error: &zvs_core::Error) -> AppError {
    let code = error.code();
    let code = AppErrorCode::ALL
        .iter()
        .copied()
        .find(|candidate| candidate.as_str() == code)
        .unwrap_or(AppErrorCode::NativeError);
    AppError::new(code, error.to_string())
}

pub fn panic_error() -> AppError {
    AppError::new(AppErrorCode::NativeError, PANIC_MESSAGE)
}

pub fn cancelled_error() -> AppError {
    AppError::new(AppErrorCode::RunCancelled, CANCELLED_MESSAGE)
}

pub(crate) fn native_number(value: usize) -> Result<u32, AppError> {
    u32::try_from(value).map_err(|_| {
        core_error(&zvs_core::Error::InvalidInput(
            "Value exceeds native limits".into(),
        ))
    })
}
