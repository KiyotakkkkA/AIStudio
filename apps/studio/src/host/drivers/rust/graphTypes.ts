import { z } from "zod";

export const MAX_GRAPH_HOPS = 8;
export const MAX_GRAPH_NODE_BUDGET = 65_536;

export const GraphNodeKindSchema = z.enum(["document", "chunk"]);
export const GraphDirectionSchema = z.enum(["outgoing", "incoming", "both"]);

export const GraphNodeSchema = z.object({
  id: z.string().trim().min(1),
  kind: GraphNodeKindSchema,
});

export const GraphEdgeSchema = z.object({
  from: z.string().trim().min(1),
  to: z.string().trim().min(1),
  relation: z.string().trim().min(1),
  weight: z.number().finite().nullable().default(null),
  metadata: z.json().default(null),
});

export const GraphNeighborSchema = z.object({
  node: GraphNodeSchema,
  edge: GraphEdgeSchema,
  direction: GraphDirectionSchema,
});

export const GraphPathSchema = z.object({
  nodes: GraphNodeSchema.array(),
  edges: GraphEdgeSchema.array(),
});

export const GraphSubgraphSchema = z.object({
  root: z.string(),
  nodes: z.object({ node: GraphNodeSchema, hops: z.number().int().nonnegative() }).array(),
  edges: GraphEdgeSchema.array(),
});

export const GraphStatsSchema = z.object({
  nodeCount: z.number().int().nonnegative(),
  edgeCount: z.number().int().nonnegative(),
  documentCount: z.number().int().nonnegative(),
  chunkCount: z.number().int().nonnegative(),
});

export const GraphHydrateSchema = z.object({
  nodes: GraphNodeSchema.array(),
  edges: GraphEdgeSchema.array(),
});

export const GraphNeighborQuerySchema = z.object({
  id: z.string().trim().min(1),
  relation: z.string().trim().min(1).optional(),
  direction: GraphDirectionSchema.default("both"),
});

export const GraphPathQuerySchema = z.object({
  from: z.string().trim().min(1),
  to: z.string().trim().min(1),
  relation: z.string().trim().min(1).optional(),
  direction: GraphDirectionSchema.default("both"),
});

export const GraphSubgraphQuerySchema = z.object({
  id: z.string().trim().min(1),
  hops: z.number().int().nonnegative().max(MAX_GRAPH_HOPS),
  relation: z.string().trim().min(1).optional(),
  direction: GraphDirectionSchema.default("both"),
  budget: z.number().int().positive().max(MAX_GRAPH_NODE_BUDGET).optional(),
});

export type GraphNodeKind = z.infer<typeof GraphNodeKindSchema>;
export type GraphDirection = z.infer<typeof GraphDirectionSchema>;
export type GraphNode = z.infer<typeof GraphNodeSchema>;
export type GraphEdge = z.infer<typeof GraphEdgeSchema>;
export type GraphEdgeInput = z.input<typeof GraphEdgeSchema>;
export type GraphNeighbor = z.infer<typeof GraphNeighborSchema>;
export type GraphPath = z.infer<typeof GraphPathSchema>;
export type GraphSubgraph = z.infer<typeof GraphSubgraphSchema>;
export type GraphStats = z.infer<typeof GraphStatsSchema>;
export type GraphHydrateInput = z.input<typeof GraphHydrateSchema>;
export type GraphNeighborQuery = z.input<typeof GraphNeighborQuerySchema>;
export type GraphPathQuery = z.input<typeof GraphPathQuerySchema>;
export type GraphSubgraphQuery = z.input<typeof GraphSubgraphQuerySchema>;

export interface GraphNamespace {
  hydrate(graphId: string, content: GraphHydrateInput): Promise<GraphStats>;
  stats(graphId: string): Promise<GraphStats>;
  addNode(graphId: string, node: GraphNode): Promise<boolean>;
  addEdge(graphId: string, edge: GraphEdgeInput): Promise<boolean>;
  removeEdge(graphId: string, from: string, to: string, relation: string): Promise<boolean>;
  removeNode(graphId: string, id: string): Promise<number | null>;
  neighbors(graphId: string, query: GraphNeighborQuery): Promise<GraphNeighbor[]>;
  shortestPath(graphId: string, query: GraphPathQuery): Promise<GraphPath | null>;
  kHopSubgraph(graphId: string, query: GraphSubgraphQuery): Promise<GraphSubgraph>;
}

export interface GraphCorePort {
  readonly graph: GraphNamespace;
}
