import { z } from "zod";
import { RunId, StreamId } from "../primitives/branded.js";
import { Timestamp } from "../primitives/time.js";
import { RunOutcomeDto } from "../events/HostEvent.js";
import { RunKind, RunStatus } from "./RunDto.js";

export const RunProgressDto = z.object({
  done: z.number().int().nonnegative(),
  total: z.number().int().nonnegative(),
});
export type RunProgressDto = z.infer<typeof RunProgressDto>;

export const RunApprovalDto = z.object({
  id: z.string().min(1),
  subject: z.string().min(1),
  scope: z.string().min(1),
  expiresAt: Timestamp,
});
export type RunApprovalDto = z.infer<typeof RunApprovalDto>;

export const RunSummaryDto = z.object({
  id: RunId,
  streamId: StreamId,
  kind: RunKind,
  subjectId: z.string().optional(),
  title: z.string(),
  status: RunStatus,
  progress: RunProgressDto,
  activeNodeId: z.string().optional(),
  approval: RunApprovalDto.optional(),
  outcome: RunOutcomeDto.optional(),
  error: z.string().optional(),
  retryOfId: RunId.optional(),
  createdAt: Timestamp,
  startedAt: Timestamp.optional(),
  finishedAt: Timestamp.optional(),
  prunedAt: Timestamp.optional(),
});
export type RunSummaryDto = z.infer<typeof RunSummaryDto>;
