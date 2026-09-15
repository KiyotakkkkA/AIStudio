import { z } from "zod";
import { DeviceGpuDto } from "../system.js";
import { ItemRef } from "../downloads/DownloadDto.js";

export const ModelFormat = z.enum(["gguf", "onnx"]);
export type ModelFormat = z.infer<typeof ModelFormat>;

export const Accelerator = z.enum(["cpu", "cuda", "vulkan", "metal"]);
export type Accelerator = z.infer<typeof Accelerator>;

export const RuntimeEngine = z.enum(["llama-cpp", "onnxruntime"]);
export type RuntimeEngine = z.infer<typeof RuntimeEngine>;

/** `unsupported` — no such build exists for this platform — is distinct from `available`. */
export const RuntimeState = z.enum([
  "unsupported",
  "available",
  "installing",
  "installed",
  "starting",
  "ready",
  "failed",
]);
export type RuntimeState = z.infer<typeof RuntimeState>;

export const RuntimeDto = z.object({
  /** `<engine>-<accelerator>`, stable across releases so a setting can name one. */
  id: z.string().min(3).max(64),
  engine: RuntimeEngine,
  accelerator: Accelerator,
  displayName: z.string().min(1).max(120),
  description: z.string().max(600).default(""),
  formats: z.array(ModelFormat).min(1),
  state: RuntimeState,
  installedVersion: z.string().max(64).optional(),
  installPath: z.string().max(4096).optional(),
  executablePath: z.string().max(4096).optional(),
  sizeBytes: z.number().int().nonnegative(),
  recommended: z.boolean(),
  blockedReason: z.string().max(400).optional(),
  error: z.string().max(2000).optional(),
  refs: z.array(ItemRef).max(8).default([]),
});
export type RuntimeDto = z.infer<typeof RuntimeDto>;

/**
 * The machine's verdict and its reasoning. `reason` is shown verbatim, so a user who disagrees
 * can pick a different build knowing what was detected.
 */
export const RuntimePlanDto = z.object({
  accelerator: Accelerator,
  reason: z.string().max(400),
  gpu: DeviceGpuDto.nullable(),
  ready: z.boolean(),
});
export type RuntimePlanDto = z.infer<typeof RuntimePlanDto>;

export const RuntimeOverviewDto = z.object({
  plan: RuntimePlanDto,
  runtimes: z.array(RuntimeDto),
  recommendedModels: z.array(ItemRef).max(16).default([]),
});
export type RuntimeOverviewDto = z.infer<typeof RuntimeOverviewDto>;

export const RuntimeRef = z.object({ id: z.string().min(3).max(64) });
export type RuntimeRef = z.infer<typeof RuntimeRef>;

/** `id` omitted means "install whatever the plan recommends". */
export const InstallRuntimeInput = z.object({ id: z.string().min(3).max(64).optional() });
export type InstallRuntimeInput = z.infer<typeof InstallRuntimeInput>;
