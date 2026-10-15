#[cfg(test)]
mod tests;

use std::sync::Arc;

use zvs_contract::{
    AddGraphEdgeInput, GraphNeighborsInput, GraphRemoveEdgeOutput, GraphShortestPathInput,
    GraphSubgraphInput, RemoveGraphEdgeInput, VectorStoreRef,
};

use super::IpcServer;
use crate::services::Services;

pub fn register(server: &mut IpcServer) {
    server
        .handle(
            "graphRelations.list",
            |services: Arc<Services>, _input: ()| async move { services.graph.list_relations() },
        )
        .handle(
            "graphRelations.listEdges",
            |services: Arc<Services>, input: VectorStoreRef| async move {
                services.graph.list_edges(&input.id.to_string())
            },
        )
        .handle(
            "graphRelations.addEdge",
            |services: Arc<Services>, input: AddGraphEdgeInput| async move {
                services.graph.add_edge(&input).await
            },
        )
        .handle(
            "graphRelations.removeEdge",
            |services: Arc<Services>, input: RemoveGraphEdgeInput| async move {
                let removed = services.graph.remove_edge(&input).await?;
                Ok(GraphRemoveEdgeOutput {
                    id: removed.id,
                    removed: true,
                })
            },
        )
        .handle(
            "graphRelations.neighbors",
            |services: Arc<Services>, input: GraphNeighborsInput| async move {
                services.graph.neighbors(&input).await
            },
        )
        .handle(
            "graphRelations.shortestPath",
            |services: Arc<Services>, input: GraphShortestPathInput| async move {
                services.graph.shortest_path(&input).await
            },
        )
        .handle(
            "graphRelations.kHopSubgraph",
            |services: Arc<Services>, input: GraphSubgraphInput| async move {
                services.graph.k_hop_subgraph(&input).await
            },
        )
        .handle(
            "graphRelations.reconcile",
            |services: Arc<Services>, input: VectorStoreRef| async move {
                services.graph.reconcile(&input.id.to_string()).await
            },
        );
}
