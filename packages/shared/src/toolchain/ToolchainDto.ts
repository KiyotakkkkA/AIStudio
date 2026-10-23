import { z } from "zod";
import { ItemRef } from "../downloads/DownloadDto.js";

export const TOOLCHAIN_VERSION_MAX = 64;
export const TOOLCHAIN_PATH_MAX = 4096;
export const TOOLCHAIN_PROBLEM_MAX = 600;
export const TOOLCHAIN_MANAGED_MAX = 16;
export const TOOLCHAIN_REFS_MAX = 4;

export const ToolchainRuntime = z.enum(["uv", "node"]);
export type ToolchainRuntime = z.infer<typeof ToolchainRuntime>;

export const ToolchainSource = z.enum(["managed", "path", "manual"]);
export type ToolchainSource = z.infer<typeof ToolchainSource>;

export const ToolchainStatus = z.enum(["missing", "installing", "ready", "broken"]);
export type ToolchainStatus = z.infer<typeof ToolchainStatus>;

const ToolchainVersion = z.string().min(1).max(TOOLCHAIN_VERSION_MAX);

export const ToolchainDto = z
  .object({
    runtime: ToolchainRuntime,
    displayName: z.string().min(1).max(120),
    status: ToolchainStatus,
    source: ToolchainSource.optional(),
    version: ToolchainVersion.optional(),
    executablePath: z.string().min(1).max(TOOLCHAIN_PATH_MAX).optional(),
    minimumVersion: ToolchainVersion,
    latestVersion: ToolchainVersion.optional(),
    updateAvailable: z.boolean(),
    managedVersions: z.array(ToolchainVersion).max(TOOLCHAIN_MANAGED_MAX).default([]),
    problem: z.string().min(1).max(TOOLCHAIN_PROBLEM_MAX).optional(),
    refs: z.array(ItemRef).max(TOOLCHAIN_REFS_MAX).default([]),
  })
  .refine((dto) => !dto.updateAvailable || dto.latestVersion !== undefined, {
    path: ["updateAvailable"],
    message: "latestVersion",
  });
export type ToolchainDto = z.infer<typeof ToolchainDto>;

export const ToolchainListInput = z.object({ checkUpdates: z.boolean().default(false) });
export type ToolchainListInput = z.infer<typeof ToolchainListInput>;

export const ToolchainRef = z.object({ runtime: ToolchainRuntime });
export type ToolchainRef = z.infer<typeof ToolchainRef>;

export const ToolchainSelectInput = z.object({
  runtime: ToolchainRuntime,
  path: z.string().min(1).max(TOOLCHAIN_PATH_MAX).optional(),
});
export type ToolchainSelectInput = z.infer<typeof ToolchainSelectInput>;
