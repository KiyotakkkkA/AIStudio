import { z } from "zod";
import { RunId, StreamId } from "../primitives/branded.js";
import { RunOutcomeDto } from "../events/HostEvent.js";

export const RunStatus = z.enum([
  "queued",
  "running",
  "blocked",
  "succeeded",
  "failed",
  "cancelled",
  "interrupted",
]);
export const RunKind = z.enum(["chat", "scenario", "agentic", "job", "browser"]);
export const GraphNode = z.object({
  id: z.string().min(1).max(128),
  type: z.string().min(1).max(128),
  dependencies: z.array(z.string()).default([]),
  input: z.json().default(null),
  bindings: z
    .record(
      z.string(),
      z.union([
        z.object({ source: z.literal("run") }),
        z.object({ source: z.literal("node"), nodeId: z.string() }),
      ]),
    )
    .default({}),
  retry: z
    .object({
      maxAttempts: z.number().int().min(1).max(100),
      backoffMs: z.number().int().min(0).max(60_000),
    })
    .default({ maxAttempts: 1, backoffMs: 0 }),
});
export type GraphNode = z.infer<typeof GraphNode>;
export const RunGraph = z.object({ nodes: z.array(GraphNode).max(10_000) });
export type RunGraph = z.infer<typeof RunGraph>;
export const StartRunInput = z.object({
  kind: RunKind,
  subjectId: z.string().min(1).optional(),
  graph: RunGraph,
  input: z.json().default(null),
  concurrency: z.number().int().min(1).max(64).default(4),
});
export type StartRunInput = z.infer<typeof StartRunInput>;
export const RunIdInput = z.object({ id: RunId });
export const RunHandleDto = z.object({ id: RunId, streamId: StreamId });
export type RunHandleDto = z.infer<typeof RunHandleDto>;
export const RunDto = z.object({
  ...RunHandleDto.shape,
  kind: RunKind,
  subjectId: z.string().optional(),
  status: RunStatus,
  graph: RunGraph,
  input: z.json(),
  outcome: RunOutcomeDto.optional(),
  concurrency: z.number().int().positive(),
  createdAt: z.number().int(),
  startedAt: z.number().int().optional(),
  finishedAt: z.number().int().optional(),
  error: z.string().optional(),
});
export type RunDto = z.infer<typeof RunDto>;
