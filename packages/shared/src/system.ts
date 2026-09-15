import { z } from "zod";
import { Timestamp } from "./primitives/time.js";

/**
 * What this machine can realistically run locally. Read by anything that has to choose a
 * default without asking the user — the vector store form's auto-fill is the first caller.
 * `tier` is the host's own verdict, so the renderer never re-derives the thresholds.
 */
export const DeviceTier = z.enum(["low", "medium", "high"]);
export type DeviceTier = z.infer<typeof DeviceTier>;

export const DeviceGpuDto = z.object({
  vendor: z.string().max(200),
  model: z.string().max(200),
  /** Only some platforms report it; `null` means unknown, never zero. */
  vramBytes: z.number().int().nonnegative().nullable(),
  discrete: z.boolean(),
});
export type DeviceGpuDto = z.infer<typeof DeviceGpuDto>;

export const DeviceProfileDto = z.object({
  platform: z.string().max(32),
  arch: z.string().max(32),
  cpuModel: z.string().max(200),
  cpuCores: z.number().int().nonnegative(),
  totalMemoryBytes: z.number().int().nonnegative(),
  freeMemoryBytes: z.number().int().nonnegative(),
  /** Free space where models and vector stores land; zero when it could not be measured. */
  freeDiskBytes: z.number().int().nonnegative(),
  gpu: DeviceGpuDto.nullable(),
  tier: DeviceTier,
  measuredAt: Timestamp,
});
export type DeviceProfileDto = z.infer<typeof DeviceProfileDto>;

/**
 * One reading of what the machine is doing right now, taken while a long job runs. Every GPU
 * field is nullable on purpose: only some vendors expose a counter we can read without
 * shipping a driver library, and a missing reading must never render as zero.
 */
export const ResourceSampleDto = z.object({
  at: Timestamp,
  /** System-wide CPU load across all cores, 0–100. */
  cpuPercent: z.number().min(0).max(100),
  memoryUsedBytes: z.number().int().nonnegative(),
  memoryTotalBytes: z.number().int().nonnegative(),
  /** Resident set of this process, the share of the above the app itself holds. */
  processMemoryBytes: z.number().int().nonnegative(),
  gpuName: z.string().max(200).nullable(),
  gpuPercent: z.number().min(0).max(100).nullable(),
  vramUsedBytes: z.number().int().nonnegative().nullable(),
  vramTotalBytes: z.number().int().nonnegative().nullable(),
});
export type ResourceSampleDto = z.infer<typeof ResourceSampleDto>;
