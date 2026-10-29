import { z } from "zod";
import { FolderPath, WorkingFolderAccess } from "./folders.js";

export const CLI_COMMAND = "aistudio";

export const CliOpenInput = z.object({ cwd: FolderPath });
export type CliOpenInput = z.infer<typeof CliOpenInput>;

export const CliSessionDto = z.object({ folder: FolderPath, access: WorkingFolderAccess });
export type CliSessionDto = z.infer<typeof CliSessionDto>;

export const CliInstallDto = z
  .object({ available: z.boolean(), installed: z.boolean(), command: z.string().min(1) })
  .refine((status) => status.available || !status.installed, { path: ["installed"] });
export type CliInstallDto = z.infer<typeof CliInstallDto>;
