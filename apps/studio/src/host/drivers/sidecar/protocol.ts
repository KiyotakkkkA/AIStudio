import { z } from "zod";

export const SidecarRequest = z.discriminatedUnion("type", [
  z.object({ type: z.literal("ping"), id: z.string().min(1) }),
  z.object({
    type: z.literal("job.start"),
    id: z.string().min(1),
    job: z.string().min(1).max(128),
    params: z.json(),
  }),
  z.object({ type: z.literal("job.cancel"), id: z.string().min(1) }),
]);
export type SidecarRequest = z.infer<typeof SidecarRequest>;

export const SidecarResponse = z.discriminatedUnion("type", [
  z.object({ type: z.literal("ping"), id: z.string() }),
  z.object({
    type: z.literal("job.progress"),
    id: z.string(),
    done: z.number().int().nonnegative(),
    total: z.number().int().nonnegative(),
    rate: z.number().nonnegative().optional(),
    message: z.string().optional(),
  }),
  z.object({ type: z.literal("job.done"), id: z.string(), result: z.json() }),
  z.object({
    type: z.literal("job.error"),
    id: z.string(),
    code: z.string(),
    message: z.string(),
  }),
]);
export type SidecarResponse = z.infer<typeof SidecarResponse>;

export interface JobProgress {
  done: number;
  total: number;
  /** Smoothed bytes per second, present only for jobs that move bytes. */
  rate?: number;
  message?: string;
}
