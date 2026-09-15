import { AppError, AppErrorCode } from "@zvs/shared";
import type {
  GraphCorePort,
  GraphDirection,
  GraphEdge,
  GraphEdgeInput,
  GraphHydrateInput,
  GraphNamespace,
  GraphNeighbor,
  GraphNeighborQuery,
  GraphNode,
  GraphPath,
  GraphPathQuery,
  GraphStats,
  GraphSubgraph,
  GraphSubgraphQuery,
} from "../../apps/studio/src/host/drivers/rust/graphTypes.ts";

interface FakeGraph {
  nodes: Map<string, GraphNode>;
  edges: GraphEdge[];
}

function normalise(edge: GraphEdgeInput): GraphEdge {
  return {
    from: edge.from,
    to: edge.to,
    relation: edge.relation,
    weight: edge.weight ?? null,
    metadata: edge.metadata ?? null,
  };
}

export class FakeGraphCore implements GraphCorePort {
  readonly graphs = new Map<string, FakeGraph>();
  hydrations = 0;

  readonly graph: GraphNamespace = {
    hydrate: async (graphId: string, content: GraphHydrateInput): Promise<GraphStats> => {
      this.hydrations += 1;
      this.graphs.set(graphId, {
        nodes: new Map(content.nodes.map((node) => [node.id, node])),
        edges: content.edges.map(normalise),
      });
      return this.graph.stats(graphId);
    },

    stats: async (graphId: string): Promise<GraphStats> => {
      const graph = this.at(graphId);
      const documentCount = [...graph.nodes.values()].filter(
        (node) => node.kind === "document",
      ).length;
      return {
        nodeCount: graph.nodes.size,
        edgeCount: graph.edges.length,
        documentCount,
        chunkCount: graph.nodes.size - documentCount,
      };
    },

    addNode: async (graphId: string, node: GraphNode): Promise<boolean> => {
      const graph = this.at(graphId);
      const fresh = !graph.nodes.has(node.id);
      graph.nodes.set(node.id, node);
      return fresh;
    },

    addEdge: async (graphId: string, edge: GraphEdgeInput): Promise<boolean> => {
      const graph = this.at(graphId);
      const added = normalise(edge);
      for (const id of [added.from, added.to]) {
        if (!graph.nodes.has(id))
          throw new AppError(AppErrorCode.VALIDATION_FAILED, `Unknown node ${id}`);
      }
      const index = graph.edges.findIndex((candidate) => this.same(candidate, added));
      if (index >= 0) {
        graph.edges[index] = added;
        return false;
      }
      graph.edges.push(added);
      return true;
    },

    removeEdge: async (
      graphId: string,
      from: string,
      to: string,
      relation: string,
    ): Promise<boolean> => {
      const graph = this.at(graphId);
      const index = graph.edges.findIndex((edge) =>
        this.same(edge, { from, to, relation, weight: null, metadata: null }),
      );
      if (index < 0) return false;
      graph.edges.splice(index, 1);
      return true;
    },

    removeNode: async (graphId: string, id: string): Promise<number | null> => {
      const graph = this.at(graphId);
      if (!graph.nodes.delete(id)) return null;
      const kept = graph.edges.filter((edge) => edge.from !== id && edge.to !== id);
      const removed = graph.edges.length - kept.length;
      graph.edges = kept;
      return removed;
    },

    neighbors: async (graphId: string, query: GraphNeighborQuery): Promise<GraphNeighbor[]> => {
      const graph = this.at(graphId);
      this.requireNode(graph, query.id);
      const direction = query.direction ?? "both";
      const neighbors: GraphNeighbor[] = [];
      for (const edge of graph.edges) {
        if (query.relation !== undefined && edge.relation !== query.relation) continue;
        if (edge.from === query.id && direction !== "incoming")
          neighbors.push({ node: graph.nodes.get(edge.to)!, edge, direction: "outgoing" });
        if (edge.to === query.id && direction !== "outgoing")
          neighbors.push({ node: graph.nodes.get(edge.from)!, edge, direction: "incoming" });
      }
      return neighbors;
    },

    shortestPath: async (graphId: string, query: GraphPathQuery): Promise<GraphPath | null> => {
      const graph = this.at(graphId);
      this.requireNode(graph, query.from);
      this.requireNode(graph, query.to);
      const direction = query.direction ?? "both";
      const previous = new Map<string, { node: string; edge: GraphEdge }>();
      const queue = [query.from];
      const seen = new Set(queue);
      while (queue.length > 0) {
        const current = queue.shift()!;
        if (current === query.to) return this.trace(graph, previous, query.from, query.to);
        for (const edge of this.stepsFrom(graph, current, query.relation, direction)) {
          const next = edge.from === current ? edge.to : edge.from;
          if (seen.has(next)) continue;
          seen.add(next);
          previous.set(next, { node: current, edge });
          queue.push(next);
        }
      }
      return null;
    },

    kHopSubgraph: async (graphId: string, query: GraphSubgraphQuery): Promise<GraphSubgraph> => {
      const graph = this.at(graphId);
      this.requireNode(graph, query.id);
      const direction = query.direction ?? "both";
      const hops = new Map<string, number>([[query.id, 0]]);
      const edges: GraphEdge[] = [];
      const queue = [query.id];
      while (queue.length > 0) {
        const current = queue.shift()!;
        const depth = hops.get(current)!;
        if (depth >= query.hops) continue;
        for (const edge of this.stepsFrom(graph, current, query.relation, direction)) {
          if (!edges.includes(edge)) edges.push(edge);
          const next = edge.from === current ? edge.to : edge.from;
          if (hops.has(next)) continue;
          hops.set(next, depth + 1);
          queue.push(next);
        }
      }
      return {
        root: query.id,
        nodes: [...hops].map(([id, depth]) => ({ node: graph.nodes.get(id)!, hops: depth })),
        edges,
      };
    },
  };

  edgesIn(graphId: string): GraphEdge[] {
    return [...this.at(graphId).edges];
  }

  private stepsFrom(
    graph: FakeGraph,
    id: string,
    relation: string | undefined,
    direction: GraphDirection,
  ): GraphEdge[] {
    return graph.edges.filter((edge) => {
      if (relation !== undefined && edge.relation !== relation) return false;
      if (edge.from === id) return direction !== "incoming";
      if (edge.to === id) return direction !== "outgoing";
      return false;
    });
  }

  private trace(
    graph: FakeGraph,
    previous: Map<string, { node: string; edge: GraphEdge }>,
    from: string,
    to: string,
  ): GraphPath {
    const nodes = [to];
    const edges: GraphEdge[] = [];
    let current = to;
    while (current !== from) {
      const step = previous.get(current)!;
      edges.unshift(step.edge);
      nodes.unshift(step.node);
      current = step.node;
    }
    return { nodes: nodes.map((id) => graph.nodes.get(id)!), edges };
  }

  private requireNode(graph: FakeGraph, id: string): void {
    if (!graph.nodes.has(id))
      throw new AppError(AppErrorCode.VALIDATION_FAILED, `Unknown node ${id}`);
  }

  private same(left: GraphEdge, right: GraphEdge): boolean {
    return left.from === right.from && left.to === right.to && left.relation === right.relation;
  }

  private at(graphId: string): FakeGraph {
    const existing = this.graphs.get(graphId);
    if (existing) return existing;
    const fresh: FakeGraph = { nodes: new Map(), edges: [] };
    this.graphs.set(graphId, fresh);
    return fresh;
  }
}
