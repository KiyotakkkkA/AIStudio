#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::json;
use tokio::sync::OnceCell;
use zvs_contract::{
    AddGraphEdgeInput, AppError, AppErrorCode, DocumentId, GraphDirection, GraphEdgeDto,
    GraphEdgeId, GraphEdgeKey, GraphLinkDto, GraphNeighborDto, GraphNeighborsInput, GraphNodeDto,
    GraphNodeKind, GraphPathDto, GraphReconcileDto, GraphRelationTypeDto, GraphRelationTypeId,
    GraphShortestPathInput, GraphSubgraphDto, GraphSubgraphInput, GraphSubgraphNodeDto,
    VectorStoreId,
};

use super::stored::{stored_id, widen};
use crate::data::repositories::{GraphEdgeDraft, GraphEdgeLookup, VectorStorePatch};
use crate::data::schema::{GraphEdgeRow, GraphRelationRow, VectorDocumentRow, VectorStoreRow};
use crate::data::{Database, Repositories};
use crate::drivers::core::{
    GraphContent, GraphDirection as CoreDirection, GraphEdge, GraphNode, GraphNodeKind as CoreKind,
    NeighborQuery, PathQuery, SharedGraphCore, SubgraphQuery,
};
use crate::platform::clock::SharedClock;
use crate::platform::ids::new_id;
use crate::platform::logger::{LogLevel, SharedLogger};

pub const STORE_NOT_FOUND: &str = "Векторное хранилище не найдено";
pub const RELATION_NOT_FOUND: &str = "Тип связи не найден";
pub const RELATION_NOT_IN_CATALOGUE: &str = "Тип связи не найден в справочнике";
pub const DOCUMENT_NOT_FOUND: &str = "Документ не найден";
pub const CROSS_STORE: &str = "Связь возможна только между документами одного хранилища";
pub const SELF_LINK: &str = "Документ нельзя связать с самим собой";
pub const DUPLICATE_EDGE: &str = "Такая связь уже существует";
pub const EDGE_NOT_FOUND: &str = "Связь не найдена";
pub const ENGINE_WRITE_FAILED: &str = "Не удалось записать связь в граф";
pub const ENGINE_REMOVE_FAILED: &str = "Не удалось удалить связь из графа";
pub const GRAPH_DRIFT: &str = "Граф расходится с базой";

const SCOPE: &str = "graphRelations";

pub struct GraphServiceOptions {
    pub database: Arc<Database>,
    pub core: SharedGraphCore,
    pub logger: SharedLogger,
    pub clock: SharedClock,
}

struct Inner {
    database: Arc<Database>,
    core: SharedGraphCore,
    logger: SharedLogger,
    clock: SharedClock,
    hydrated: Mutex<HashMap<String, Arc<OnceCell<()>>>>,
}

#[derive(Clone)]
pub struct GraphService {
    inner: Arc<Inner>,
}

pub const fn core_direction(direction: GraphDirection) -> CoreDirection {
    match direction {
        GraphDirection::Outgoing => CoreDirection::Outgoing,
        GraphDirection::Incoming => CoreDirection::Incoming,
        GraphDirection::Both => CoreDirection::Both,
    }
}

const fn contract_direction(direction: CoreDirection) -> GraphDirection {
    match direction {
        CoreDirection::Outgoing => GraphDirection::Outgoing,
        CoreDirection::Incoming => GraphDirection::Incoming,
        CoreDirection::Both => GraphDirection::Both,
    }
}

pub fn node_dto(node: &GraphNode) -> GraphNodeDto {
    GraphNodeDto {
        id: node.id.clone(),
        kind: match node.kind {
            CoreKind::Document => GraphNodeKind::Document,
            CoreKind::Chunk => GraphNodeKind::Chunk,
        },
    }
}

pub fn link_dto(edge: &GraphEdge) -> GraphLinkDto {
    GraphLinkDto {
        from: edge.from.clone(),
        to: edge.to.clone(),
        relation: edge.relation.clone(),
        weight: edge.weight.map(widen),
        metadata: edge.metadata.clone(),
    }
}

fn document_node(id: &str) -> GraphNode {
    GraphNode {
        id: id.to_owned(),
        kind: CoreKind::Document,
    }
}

fn engine_edge(row: &GraphEdgeRow, relation: &str) -> GraphEdge {
    GraphEdge {
        from: row.from_document_id.clone(),
        to: row.to_document_id.clone(),
        relation: relation.to_owned(),
        weight: row.weight.map(|weight| weight as f32),
        metadata: row.metadata(),
    }
}

pub fn relation_dto(row: &GraphRelationRow) -> Result<GraphRelationTypeDto, AppError> {
    Ok(GraphRelationTypeDto {
        id: stored_id(GraphRelationTypeId::parse, &row.id)?,
        name: row.name.clone(),
        description: row.description.clone(),
        directed: row.directed,
    })
}

pub fn edge_dto(row: &GraphEdgeRow) -> Result<GraphEdgeDto, AppError> {
    Ok(GraphEdgeDto {
        id: stored_id(GraphEdgeId::parse, &row.id)?,
        store_id: stored_id(VectorStoreId::parse, &row.store_id)?,
        from_document_id: stored_id(DocumentId::parse, &row.from_document_id)?,
        to_document_id: stored_id(DocumentId::parse, &row.to_document_id)?,
        relation_type_id: stored_id(GraphRelationTypeId::parse, &row.relation_type_id)?,
        weight: row.weight,
        metadata: row.metadata(),
        created_at: row.created_at,
    })
}

fn not_found(message: &str) -> AppError {
    AppError::new(AppErrorCode::NotFound, message)
}

impl GraphService {
    pub fn new(options: GraphServiceOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                database: options.database,
                core: options.core,
                logger: options.logger,
                clock: options.clock,
                hydrated: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn list_relations(&self) -> Result<Vec<GraphRelationTypeDto>, AppError> {
        self.inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).graph_relations.list()?)
            })?
            .iter()
            .map(relation_dto)
            .collect()
    }

    pub fn list_edges(&self, store_id: &str) -> Result<Vec<GraphEdgeDto>, AppError> {
        let store = self.require_store(store_id)?;
        self.inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .graph_edges
                    .list_by_store(&store.id)?)
            })?
            .iter()
            .map(edge_dto)
            .collect()
    }

    pub async fn add_edge(&self, input: &AddGraphEdgeInput) -> Result<GraphEdgeDto, AppError> {
        let key = Key::from(&input.key);
        let store = self.require_store(&key.store_id)?;
        let relation = self.require_relation(&key.relation_type_id)?;
        let from = self.require_document(&store.id, &key.from_document_id)?;
        let to = self.require_document(&store.id, &key.to_document_id)?;
        if from.id == to.id {
            return Err(AppError::new(AppErrorCode::ValidationFailed, SELF_LINK));
        }
        if self.find_by_key(&key)?.is_some() {
            return Err(AppError::new(AppErrorCode::Conflict, DUPLICATE_EDGE));
        }
        self.ensure_hydrated(&store.id).await?;
        let draft = GraphEdgeDraft {
            id: new_id().hyphenated().to_string(),
            store_id: key.store_id.clone(),
            from_document_id: key.from_document_id.clone(),
            to_document_id: key.to_document_id.clone(),
            relation_type_id: key.relation_type_id.clone(),
            weight: input.weight(),
            metadata: input.metadata(),
            created_at: self.inner.clock.now(),
        };
        let row = self.inner.database.with_connection(|connection| {
            Ok(Repositories::new(connection).graph_edges.link(&draft)?)
        })?;
        if let Err(cause) = self.write_engine_edge(&store.id, &row, &relation).await {
            let removed = self.inner.database.with_connection(|connection| {
                Ok(Repositories::new(connection).graph_edges.remove(&row.id)?)
            });
            self.inner.logger.log(
                LogLevel::Error,
                SCOPE,
                ENGINE_WRITE_FAILED,
                &[
                    ("storeId", json!(store.id)),
                    ("edgeId", json!(row.id)),
                    ("error", json!(cause.to_string())),
                ],
            );
            removed?;
            return Err(cause);
        }
        edge_dto(&row)
    }

    async fn write_engine_edge(
        &self,
        store_id: &str,
        row: &GraphEdgeRow,
        relation: &GraphRelationRow,
    ) -> Result<(), AppError> {
        let core = &self.inner.core;
        core.add_node(store_id, document_node(&row.from_document_id))
            .await?;
        core.add_node(store_id, document_node(&row.to_document_id))
            .await?;
        core.add_edge(store_id, engine_edge(row, &relation.name))
            .await?;
        Ok(())
    }

    pub async fn remove_edge(&self, key: &GraphEdgeKey) -> Result<GraphEdgeDto, AppError> {
        let key = Key::from(key);
        self.require_store(&key.store_id)?;
        let row = self
            .find_by_key(&key)?
            .ok_or_else(|| not_found(EDGE_NOT_FOUND))?;
        let relation = self.require_relation(&row.relation_type_id)?;
        self.ensure_hydrated(&key.store_id).await?;
        if let Err(cause) = self
            .inner
            .core
            .remove_edge(
                &key.store_id,
                &row.from_document_id,
                &row.to_document_id,
                &relation.name,
            )
            .await
        {
            self.mark_stale(&key.store_id, true)?;
            self.inner.logger.log(
                LogLevel::Error,
                SCOPE,
                ENGINE_REMOVE_FAILED,
                &[
                    ("storeId", json!(key.store_id)),
                    ("edgeId", json!(row.id)),
                    ("error", json!(cause.to_string())),
                ],
            );
            return Err(
                AppError::new(AppErrorCode::NativeError, ENGINE_REMOVE_FAILED)
                    .with_detail("cause", cause.to_string()),
            );
        }
        self.inner.database.with_connection(|connection| {
            Ok(Repositories::new(connection).graph_edges.remove(&row.id)?)
        })?;
        edge_dto(&row)
    }

    pub async fn neighbors(
        &self,
        input: &GraphNeighborsInput,
    ) -> Result<Vec<GraphNeighborDto>, AppError> {
        let store_id = input.store_id.to_string();
        let document_id = input.document_id.to_string();
        self.prepare(&store_id, &[&document_id]).await?;
        let neighbors = self
            .inner
            .core
            .neighbors(
                &store_id,
                NeighborQuery {
                    id: document_id,
                    relation: self.relation_name(input.relation.as_deref())?,
                    direction: core_direction(input.direction()),
                },
            )
            .await?;
        Ok(neighbors
            .iter()
            .map(|neighbor| GraphNeighborDto {
                node: node_dto(&neighbor.node),
                edge: link_dto(&neighbor.edge),
                direction: contract_direction(neighbor.direction),
            })
            .collect())
    }

    pub async fn shortest_path(
        &self,
        input: &GraphShortestPathInput,
    ) -> Result<Option<GraphPathDto>, AppError> {
        let store_id = input.store_id.to_string();
        let from = input.from_document_id.to_string();
        let to = input.to_document_id.to_string();
        self.prepare(&store_id, &[&from, &to]).await?;
        let path = self
            .inner
            .core
            .shortest_path(
                &store_id,
                PathQuery {
                    from,
                    to,
                    relation: self.relation_name(input.relation.as_deref())?,
                    direction: core_direction(input.direction()),
                },
            )
            .await?;
        Ok(path.map(|path| GraphPathDto {
            nodes: path.nodes.iter().map(node_dto).collect(),
            edges: path.edges.iter().map(link_dto).collect(),
        }))
    }

    pub async fn k_hop_subgraph(
        &self,
        input: &GraphSubgraphInput,
    ) -> Result<GraphSubgraphDto, AppError> {
        let store_id = input.store_id.to_string();
        let document_id = input.document_id.to_string();
        self.prepare(&store_id, &[&document_id]).await?;
        let subgraph = self
            .inner
            .core
            .k_hop_subgraph(
                &store_id,
                SubgraphQuery {
                    id: document_id,
                    hops: u32::try_from(input.hops()).unwrap_or(u32::MAX),
                    relation: self.relation_name(input.relation.as_deref())?,
                    direction: core_direction(input.direction()),
                    budget: input
                        .budget
                        .map(|budget| usize::try_from(budget).unwrap_or(usize::MAX)),
                },
            )
            .await?;
        Ok(GraphSubgraphDto {
            root: subgraph.root,
            nodes: subgraph
                .nodes
                .iter()
                .map(|entry| GraphSubgraphNodeDto {
                    node: node_dto(&entry.node),
                    hops: u64::from(entry.hops),
                })
                .collect(),
            edges: subgraph.edges.iter().map(link_dto).collect(),
        })
    }

    pub async fn reconcile(&self, store_id: &str) -> Result<GraphReconcileDto, AppError> {
        let store = self.require_store(store_id)?;
        self.ensure_hydrated(&store.id).await?;
        let recorded_edges = self.inner.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .graph_edges
                .count_by_store(&store.id)?)
        })?;
        let engine_edges = self.inner.core.graph_stats(&store.id).await?.edge_count as u64;
        let graph_stale = recorded_edges != engine_edges;
        self.mark_stale(&store.id, graph_stale)?;
        if graph_stale {
            self.inner.logger.log(
                LogLevel::Warn,
                SCOPE,
                GRAPH_DRIFT,
                &[
                    ("storeId", json!(store.id)),
                    ("recordedEdges", json!(recorded_edges)),
                    ("engineEdges", json!(engine_edges)),
                ],
            );
        }
        Ok(GraphReconcileDto {
            store_id: stored_id(VectorStoreId::parse, &store.id)?,
            recorded_edges,
            engine_edges,
            graph_stale,
        })
    }

    pub fn forget(&self, store_id: &str) {
        self.inner
            .hydrated
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(store_id);
    }

    async fn prepare(&self, store_id: &str, document_ids: &[&str]) -> Result<(), AppError> {
        self.require_store(store_id)?;
        for document_id in document_ids {
            self.require_document(store_id, document_id)?;
        }
        self.ensure_hydrated(store_id).await
    }

    async fn ensure_hydrated(&self, store_id: &str) -> Result<(), AppError> {
        let cell = Arc::clone(
            self.inner
                .hydrated
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(store_id.to_owned())
                .or_default(),
        );
        cell.get_or_try_init(|| self.hydrate(store_id))
            .await
            .copied()
    }

    async fn hydrate(&self, store_id: &str) -> Result<(), AppError> {
        let (rows, relations, documents) = self.inner.database.with_connection(|connection| {
            let repositories = Repositories::new(connection);
            let rows = repositories.graph_edges.list_by_store(store_id)?;
            let ids: Vec<String> = rows
                .iter()
                .map(|row| row.relation_type_id.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            let relations = repositories.graph_relations.list_by_ids(&ids)?;
            let documents = repositories.vector_documents.list_by_store(store_id)?;
            Ok((rows, relations, documents))
        })?;
        let names: HashMap<String, String> = relations
            .into_iter()
            .map(|relation| (relation.id, relation.name))
            .collect();
        let mut order = Vec::new();
        let mut seen = HashSet::new();
        let mut node = |id: &str| {
            if seen.insert(id.to_owned()) {
                order.push(document_node(id));
            }
        };
        for document in &documents {
            node(&document.id);
        }
        for row in &rows {
            node(&row.from_document_id);
            node(&row.to_document_id);
        }
        let edges = rows
            .iter()
            .map(|row| {
                names
                    .get(&row.relation_type_id)
                    .map(|name| engine_edge(row, name))
                    .ok_or_else(|| not_found(RELATION_NOT_IN_CATALOGUE))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.inner
            .core
            .hydrate(
                store_id,
                GraphContent {
                    nodes: order,
                    edges,
                },
            )
            .await?;
        Ok(())
    }

    fn relation_name(&self, relation: Option<&str>) -> Result<Option<String>, AppError> {
        let Some(relation) = relation else {
            return Ok(None);
        };
        self.inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .graph_relations
                    .find_by_name(relation)?)
            })?
            .map(|row| Some(row.name))
            .ok_or_else(|| not_found(RELATION_NOT_IN_CATALOGUE))
    }

    fn mark_stale(&self, store_id: &str, graph_stale: bool) -> Result<(), AppError> {
        let now = self.inner.clock.now();
        self.inner.database.with_connection(|connection| {
            let stores = Repositories::new(connection).vector_stores;
            let Some(store) = stores.find_by_id(store_id)? else {
                return Ok(());
            };
            if store.graph_stale != graph_stale {
                stores.update(
                    store_id,
                    &VectorStorePatch {
                        graph_stale: Some(graph_stale),
                        updated_at: Some(now),
                        ..VectorStorePatch::default()
                    },
                )?;
            }
            Ok(())
        })
    }

    fn find_by_key(&self, key: &Key) -> Result<Option<GraphEdgeRow>, AppError> {
        self.inner.database.with_connection(|connection| {
            Ok(Repositories::new(connection)
                .graph_edges
                .find_by_key(key.lookup())?)
        })
    }

    fn require_store(&self, id: &str) -> Result<VectorStoreRow, AppError> {
        self.inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection).vector_stores.find_by_id(id)?)
            })?
            .ok_or_else(|| not_found(STORE_NOT_FOUND))
    }

    fn require_relation(&self, id: &str) -> Result<GraphRelationRow, AppError> {
        self.inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .graph_relations
                    .find_by_id(id)?)
            })?
            .ok_or_else(|| not_found(RELATION_NOT_FOUND))
    }

    fn require_document(
        &self,
        store_id: &str,
        document_id: &str,
    ) -> Result<VectorDocumentRow, AppError> {
        let document = self
            .inner
            .database
            .with_connection(|connection| {
                Ok(Repositories::new(connection)
                    .vector_documents
                    .find_by_id(document_id)?)
            })?
            .ok_or_else(|| not_found(DOCUMENT_NOT_FOUND))?;
        if document.store_id != store_id {
            return Err(AppError::new(AppErrorCode::ValidationFailed, CROSS_STORE));
        }
        Ok(document)
    }
}

struct Key {
    store_id: String,
    from_document_id: String,
    to_document_id: String,
    relation_type_id: String,
}

impl Key {
    fn lookup(&self) -> GraphEdgeLookup<'_> {
        GraphEdgeLookup {
            store_id: &self.store_id,
            from_document_id: &self.from_document_id,
            to_document_id: &self.to_document_id,
            relation_type_id: &self.relation_type_id,
        }
    }
}

impl From<&GraphEdgeKey> for Key {
    fn from(key: &GraphEdgeKey) -> Self {
        Self {
            store_id: key.store_id.to_string(),
            from_document_id: key.from_document_id.to_string(),
            to_document_id: key.to_document_id.to_string(),
            relation_type_id: key.relation_type_id.to_string(),
        }
    }
}
