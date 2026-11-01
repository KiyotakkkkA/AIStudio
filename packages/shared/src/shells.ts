import { z } from "zod";
import { ArtifactDto } from "./artifacts.js";
import { BackgroundShellId, StreamId } from "./primitives/branded.js";

export const SHELL_STEP_DOMAIN = "shell";
export const SHELLS_PER_OWNER_MAX = 4;
export const SHELLS_TOTAL_MAX = 16;
export const SHELL_LIFETIME_SECS = 3600;
export const SHELL_BUFFER_MAX = 262_144;
export const SHELL_TAIL_MAX = 16_000;
export const SHELL_ARTIFACTS_MAX = 64;

export const BackgroundShellStatus = z.enum([
  "running",
  "exited",
  "stopped",
  "failed",
  "timed-out",
]);
export type BackgroundShellStatus = z.infer<typeof BackgroundShellStatus>;

const OwnerId = z.uuid();

export const BackgroundShellDto = z
  .object({
    id: BackgroundShellId,
    ownerId: OwnerId,
    program: z.string().min(1).max(32767),
    argv: z.array(z.string().max(4096)).max(64),
    cwd: z.string().min(1),
    status: BackgroundShellStatus,
    exitCode: z.number().int().nullable(),
    streamId: StreamId,
    startedAt: z.number().int().nonnegative(),
    endedAt: z.number().int().nonnegative().nullable(),
  })
  .refine((shell) => (shell.status === "running") === (shell.endedAt === null), {
    path: ["endedAt"],
  });
export type BackgroundShellDto = z.infer<typeof BackgroundShellDto>;

export const BackgroundShellStatusDto = z.object({
  domain: z.literal(SHELL_STEP_DOMAIN),
  id: BackgroundShellId,
  ownerId: OwnerId,
  status: BackgroundShellStatus,
  exitCode: z.number().int().nullable(),
});
export type BackgroundShellStatusDto = z.infer<typeof BackgroundShellStatusDto>;

export const ShellsListInput = z.object({ ownerId: OwnerId });
export type ShellsListInput = z.infer<typeof ShellsListInput>;

export const ShellRef = z.object({ id: BackgroundShellId });
export type ShellRef = z.infer<typeof ShellRef>;

export const ShellsOutputInput = z.object({
  id: BackgroundShellId,
  sinceSeq: z.number().int().nonnegative().optional(),
});
export type ShellsOutputInput = z.infer<typeof ShellsOutputInput>;

export const ShellOutputDto = z.object({
  shell: BackgroundShellDto,
  text: z.string().max(SHELL_BUFFER_MAX),
  nextSeq: z.number().int().nonnegative(),
  truncated: z.boolean(),
  eventSeq: z.number().int().nonnegative(),
});
export type ShellOutputDto = z.infer<typeof ShellOutputDto>;

export const EndedShellDto = z
  .object({
    shell: BackgroundShellDto,
    tail: z.string().max(SHELL_TAIL_MAX),
    artifacts: z.array(ArtifactDto).max(SHELL_ARTIFACTS_MAX).default([]),
  })
  .refine((ended) => ended.shell.status !== "running", { path: ["shell", "status"] });
export type EndedShellDto = z.infer<typeof EndedShellDto>;
