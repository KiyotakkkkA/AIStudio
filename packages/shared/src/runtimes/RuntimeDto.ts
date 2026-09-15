import { z } from "zod";
import { DeviceGpuDto } from "../system.js";
import { ItemRef } from "../downloads/DownloadDto.js";

/**
 * A weight file format. The format decides which engine can load it, which is why it is a
 * first-class value rather than an extension parsed at the call site.
 */
export const ModelFormat = z.enum(["gguf", "onnx"]);
export type ModelFormat = z.infer<typeof ModelFormat>;

/**
 * How an engine build talks to the hardware. `cpu` always works; the rest each need a driver
 * stack the machine may or may not have, which is what `RuntimePlanDto` decides.
 */
export const Accelerator = z.enum(["cpu", "cuda", "vulkan", "metal"]);
export type Accelerator = z.infer<typeof Accelerator>;

export const RuntimeEngine = z.enum(["llama-cpp", "onnxruntime"]);
export type RuntimeEngine = z.infer<typeof RuntimeEngine>;

/**
 * `unsupported` is the honest answer for a build that does not exist for this platform — a
 * CUDA build on macOS — and is distinct from `available`, which the user can act on.
 */
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

/** A single installable engine build: one engine, one accelerator, one platform. */
export const RuntimeDto = z.object({
  /** `<engine>-<accelerator>`, stable across releases so settings can name one. */
  id: z.string().min(3).max(64),
  engine: RuntimeEngine,
  accelerator: Accelerator,
  displayName: z.string().min(1).max(120),
  description: z.string().max(600).default(""),
  formats: z.array(ModelFormat).min(1),
  state: RuntimeState,
  /** Build tag of what is installed, e.g. llama.cpp's `b10970`. */
  installedVersion: z.string().max(64).optional(),
  /** Where the unpacked build lives; absent until something is installed. */
  installPath: z.string().max(4096).optional(),
  /** The server executable inside `installPath`, once it has been found. */
  executablePath: z.string().max(4096).optional(),
  /** Bytes on disk, across every archive this build needs. */
  sizeBytes: z.number().int().nonnegative(),
  /** True for the build this machine should actually use. */
  recommended: z.boolean(),
  /** Why this build cannot be installed or started right now. */
  blockedReason: z.string().max(400).optional(),
  error: z.string().max(2000).optional(),
  /** Catalogue refs queued for this build, so the page can find its progress rows. */
  refs: z.array(ItemRef).max(8).default([]),
});
export type RuntimeDto = z.infer<typeof RuntimeDto>;

/**
 * The machine's verdict: which build to install, and why. The reason is shown verbatim, so a
 * user who disagrees can pick a different build knowing what was detected.
 */
export const RuntimePlanDto = z.object({
  accelerator: Accelerator,
  reason: z.string().max(400),
  gpu: DeviceGpuDto.nullable(),
  /** False when no engine is installed at all — nothing local can embed yet. */
  ready: z.boolean(),
});
export type RuntimePlanDto = z.infer<typeof RuntimePlanDto>;

export const RuntimeOverviewDto = z.object({
  plan: RuntimePlanDto,
  runtimes: z.array(RuntimeDto),
  /** Catalogue refs of the embedding models that pair with a local engine. */
  recommendedModels: z.array(ItemRef).max(16).default([]),
});
export type RuntimeOverviewDto = z.infer<typeof RuntimeOverviewDto>;

export const RuntimeRef = z.object({ id: z.string().min(3).max(64) });
export type RuntimeRef = z.infer<typeof RuntimeRef>;

/** `id` omitted means "install whatever the plan recommends". */
export const InstallRuntimeInput = z.object({ id: z.string().min(3).max(64).optional() });
export type InstallRuntimeInput = z.infer<typeof InstallRuntimeInput>;
