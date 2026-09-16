import type { Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { GraphService } from "../services/GraphService.ts";

type GraphRelationChannels =
  | "graphRelations.list"
  | "graphRelations.listEdges"
  | "graphRelations.addEdge"
  | "graphRelations.removeEdge"
  | "graphRelations.neighbors"
  | "graphRelations.shortestPath"
  | "graphRelations.kHopSubgraph"
  | "graphRelations.reconcile";

export function createGraphRelationHandlers(
  service: GraphService,
): Pick<IpcHandlers<Contract>, GraphRelationChannels> {
  return {
    "graphRelations.list": () => service.listRelations(),
    "graphRelations.listEdges": ({ id }) => service.listEdges(id),
    "graphRelations.addEdge": (input) => service.addEdge(input),
    "graphRelations.removeEdge": async (input) => {
      const removed = await service.removeEdge(input);
      return { id: removed.id, removed: true };
    },
    "graphRelations.neighbors": (input) => service.neighbors(input),
    "graphRelations.shortestPath": (input) => service.shortestPath(input),
    "graphRelations.kHopSubgraph": (input) => service.kHopSubgraph(input),
    "graphRelations.reconcile": ({ id }) => service.reconcile(id),
  };
}
