import { z } from "zod";
import { GraphRelationTypeId } from "../primitives/branded.js";

export const GRAPH_RELATION_NAME = /^[a-z][a-z0-9_]*$/;

export const GraphRelationTypeDto = z.object({
  id: GraphRelationTypeId,
  name: z.string().trim().min(1).max(64).regex(GRAPH_RELATION_NAME),
  description: z.string().max(512),
  directed: z.boolean(),
});
export type GraphRelationTypeDto = z.infer<typeof GraphRelationTypeDto>;
