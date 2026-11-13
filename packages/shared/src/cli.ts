import { z } from "zod";
import { AllowRuleScope, ExecMode } from "./execPolicy.js";
import { FolderPath, WorkingFolderAccess } from "./folders.js";
import { ConversationId } from "./primitives/branded.js";
import { Timestamp } from "./primitives/time.js";

export const CLI_COMMAND = "aistudio";
export const CLI_STEP_DOMAIN = "cli";
export const CLI_IMAGE_MAX = 4096;
export const CLI_CONFIRM_SETTING_KEY = "cli.confirmLoosening";

export const CliOpenInput = z.object({ cwd: FolderPath });
export type CliOpenInput = z.infer<typeof CliOpenInput>;

export const CliSessionDto = z.object({ folder: FolderPath, access: WorkingFolderAccess });
export type CliSessionDto = z.infer<typeof CliSessionDto>;

export const CliInstallDto = z
  .object({ available: z.boolean(), installed: z.boolean(), command: z.string().min(1) })
  .refine((status) => status.available || !status.installed, { path: ["installed"] });
export type CliInstallDto = z.infer<typeof CliInstallDto>;

export const CLI_THEMES = ["ember", "graphite", "moss", "dusk", "frost"] as const;
export const CliTheme = z.enum(CLI_THEMES);
export type CliTheme = z.infer<typeof CliTheme>;

export const CliThemeDto = z.object({ theme: CliTheme });
export type CliThemeDto = z.infer<typeof CliThemeDto>;

export const CLI_PREFERENCES_FILE = "preferences.json";
export const CliPreferencesDto = z.object({ autoCopy: z.boolean().default(true) });
export type CliPreferencesDto = z.infer<typeof CliPreferencesDto>;

export const CliChangeDto = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("mode"), from: ExecMode, to: ExecMode }),
  z.object({ kind: z.literal("judge") }),
  z.object({ kind: z.literal("allowRule"), scope: AllowRuleScope }),
  z.object({ kind: z.literal("alwaysApproval") }),
  z.object({ kind: z.literal("alwaysFolder") }),
  z.object({
    kind: z.literal("openDialog"),
    conversationId: ConversationId,
    title: z.string().max(200),
  }),
]);
export type CliChangeDto = z.infer<typeof CliChangeDto>;

const ConnectionId = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER);
const ClientImage = z.string().min(1).max(CLI_IMAGE_MAX).nullable();

export const CliConnectionDto = z.object({
  id: ConnectionId,
  pid: z.number().int().nonnegative().max(4_294_967_295).nullable(),
  image: ClientImage,
  recognised: z.boolean(),
  connectedAt: Timestamp,
  dialogs: z.number().int().nonnegative().max(4_294_967_295),
});
export type CliConnectionDto = z.infer<typeof CliConnectionDto>;

export const CliConnectionRef = z.object({ id: ConnectionId });
export type CliConnectionRef = z.infer<typeof CliConnectionRef>;

export const CliConsentDto = z
  .object({
    id: z.uuid(),
    connectionId: ConnectionId,
    change: CliChangeDto,
    recognised: z.boolean(),
    image: ClientImage,
    createdAt: Timestamp,
    expiresAt: Timestamp,
  })
  .refine((consent) => consent.expiresAt >= consent.createdAt, { path: ["expiresAt"] });
export type CliConsentDto = z.infer<typeof CliConsentDto>;

export const CliConsentAnswerInput = z.object({ id: z.uuid(), confirm: z.boolean() });
export type CliConsentAnswerInput = z.infer<typeof CliConsentAnswerInput>;
