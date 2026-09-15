import { z } from "zod";
import { DocumentId, VectorStoreId } from "../primitives/branded.js";
import { Json } from "../primitives/json.js";

export const GRAPH_MAX_HOPS = 8;

export const GraphNodeKind = z.enum(["document", "chunk"]);
export type GraphNodeKind = z.infer<typeof GraphNodeKind>;

export const GraphDirection = z.enum(["outgoing", "incoming", "both"]);
export type GraphDirection = z.infer<typeof GraphDirection>;

export const GraphNodeDto = z.object({ id: z.string().min(1), kind: GraphNodeKind });
export type GraphNodeDto = z.infer<typeof GraphNodeDto>;

export const GraphLinkDto = z.object({
  from: z.string().min(1),
  to: z.string().min(1),
  relation: z.string().min(1),
  weight: z.number().finite().nullable(),
  metadata: Json,
});
export type GraphLinkDto = z.infer<typeof GraphLinkDto>;

export const GraphNeighborDto = z.object({
  node: GraphNodeDto,
  edge: GraphLinkDto,
  direction: GraphDirection,
});
export type GraphNeighborDto = z.infer<typeof GraphNeighborDto>;

export const GraphPathDto = z.object({
  nodes: z.array(GraphNodeDto),
  edges: z.array(GraphLinkDto),
});
export type GraphPathDto = z.infer<typeof GraphPathDto>;

export const GraphSubgraphDto = z.object({
  root: z.string(),
  nodes: z.array(z.object({ node: GraphNodeDto, hops: z.number().int().nonnegative() })),
  edges: z.array(GraphLinkDto),
});
export type GraphSubgraphDto = z.infer<typeof GraphSubgraphDto>;

const relation = z.string().trim().min(1).max(64).optional();

export const GraphNeighborsInput = z.object({
  storeId: VectorStoreId,
  documentId: DocumentId,
  relation,
  direction: GraphDirection.default("both"),
});
export type GraphNeighborsInput = z.infer<typeof GraphNeighborsInput>;
export type GraphNeighborsRequest = z.input<typeof GraphNeighborsInput>;

export const GraphShortestPathInput = z.object({
  storeId: VectorStoreId,
  fromDocumentId: DocumentId,
  toDocumentId: DocumentId,
  relation,
  direction: GraphDirection.default("both"),
});
export type GraphShortestPathInput = z.infer<typeof GraphShortestPathInput>;
export type GraphShortestPathRequest = z.input<typeof GraphShortestPathInput>;

export const GraphSubgraphInput = z.object({
  storeId: VectorStoreId,
  documentId: DocumentId,
  hops: z.number().int().min(1).max(GRAPH_MAX_HOPS).default(2),
  relation,
  direction: GraphDirection.default("both"),
  budget: z.number().int().positive().max(65536).optional(),
});
export type GraphSubgraphInput = z.infer<typeof GraphSubgraphInput>;
export type GraphSubgraphRequest = z.input<typeof GraphSubgraphInput>;

export const GraphReconcileDto = z.object({
  storeId: VectorStoreId,
  recordedEdges: z.number().int().nonnegative(),
  engineEdges: z.number().int().nonnegative(),
  graphStale: z.boolean(),
});
export type GraphReconcileDto = z.infer<typeof GraphReconcileDto>;
