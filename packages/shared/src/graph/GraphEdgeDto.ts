import { z } from "zod";
import {
  DocumentId,
  GraphEdgeId,
  GraphRelationTypeId,
  VectorStoreId,
} from "../primitives/branded.js";
import { Json } from "../primitives/json.js";
import { Timestamp } from "../primitives/time.js";

export const GraphEdgeDto = z.object({
  id: GraphEdgeId,
  storeId: VectorStoreId,
  fromDocumentId: DocumentId,
  toDocumentId: DocumentId,
  relationTypeId: GraphRelationTypeId,
  weight: z.number().finite().nullable(),
  metadata: Json,
  createdAt: Timestamp,
});
export type GraphEdgeDto = z.infer<typeof GraphEdgeDto>;

export const GraphEdgeRef = z.object({ id: GraphEdgeId });
export type GraphEdgeRef = z.infer<typeof GraphEdgeRef>;

export const GraphEdgeKey = GraphEdgeDto.pick({
  storeId: true,
  fromDocumentId: true,
  toDocumentId: true,
  relationTypeId: true,
});
export type GraphEdgeKey = z.infer<typeof GraphEdgeKey>;

export const AddGraphEdgeInput = GraphEdgeKey.extend({
  weight: GraphEdgeDto.shape.weight.default(null),
  metadata: Json.default(null),
});
export type AddGraphEdgeInput = z.infer<typeof AddGraphEdgeInput>;
export type AddGraphEdgeRequest = z.input<typeof AddGraphEdgeInput>;

export const RemoveGraphEdgeInput = GraphEdgeKey;
export type RemoveGraphEdgeInput = z.infer<typeof RemoveGraphEdgeInput>;
