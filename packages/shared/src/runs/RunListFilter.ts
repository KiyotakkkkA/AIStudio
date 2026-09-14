import { z } from "zod";
import { Page } from "../primitives/paging.js";
import { Timestamp } from "../primitives/time.js";
import { RunKind, RunStatus } from "./RunDto.js";
import { RunSummaryDto } from "./RunSummaryDto.js";

export const RunListFilter = z.object({
  statuses: z.array(RunStatus).max(RunStatus.options.length).optional(),
  kinds: z.array(RunKind).max(RunKind.options.length).optional(),
  query: z.string().trim().max(200).optional(),
  from: Timestamp.optional(),
  to: Timestamp.optional(),
  live: z.boolean().default(false),
  limit: z.number().int().min(1).max(200).default(50),
  cursor: z.string().min(1).max(128).optional(),
});
export type RunListFilter = z.infer<typeof RunListFilter>;

export const RunCountsDto = z.object({
  total: z.number().int().nonnegative(),
  byStatus: z.record(RunStatus, z.number().int().nonnegative()),
  byKind: z.record(RunKind, z.number().int().nonnegative()),
});
export type RunCountsDto = z.infer<typeof RunCountsDto>;

export const RunPageDto = Page(RunSummaryDto).extend({ counts: RunCountsDto });
export type RunPageDto = z.infer<typeof RunPageDto>;
