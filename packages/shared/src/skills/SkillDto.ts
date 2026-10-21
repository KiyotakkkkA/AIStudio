import { z } from "zod";
import { ProviderId, SkillId } from "../primitives/branded.js";
import { Timestamp } from "../primitives/time.js";

export const SKILL_KEY_MAX = 64;
export const SKILL_NAME_MAX = 120;
export const SKILL_DESCRIPTION_MAX = 1024;
export const SKILL_INSTRUCTIONS_MAX = 100_000;
export const SKILL_ALLOWED_TOOLS_MAX = 64;
export const SKILL_PATH_MAX = 240;
export const SKILL_FILES_MAX = 200;
export const SKILL_FILE_MAX_BYTES = 1024 * 1024;
export const SKILL_TOTAL_MAX_BYTES = 10 * 1024 * 1024;
export const SKILL_MESSAGE_MAX = 8000;
export const SKILL_MODEL_MAX = 256;
export const SKILL_ISSUES_MAX = 64;
export const SKILL_FILE_DIRS = ["references", "scripts", "assets"] as const;
export const DEFAULT_SKILL_THRESHOLD = 0.5;
export const DEFAULT_SKILL_CATALOGUE_LIMIT = 20;
export const SKILL_CATALOGUE_MAX = 200;
export const SKILL_TRIGGER_KEY = "skills.trigger";

export const SkillSource = z.enum(["local", "imported"]);
export type SkillSource = z.infer<typeof SkillSource>;

export const SkillStatus = z.enum(["ready", "invalid"]);
export type SkillStatus = z.infer<typeof SkillStatus>;

export const SkillMatchMethod = z.enum(["embedding", "lexical"]);
export type SkillMatchMethod = z.infer<typeof SkillMatchMethod>;

export const SkillFileEncoding = z.enum(["utf8", "base64"]);
export type SkillFileEncoding = z.infer<typeof SkillFileEncoding>;

export const SkillIssueRule = z.enum([
  "required",
  "pattern",
  "tooLong",
  "duplicate",
  "unknownTool",
  "toolOff",
  "toolDisabled",
  "toolUnavailable",
  "path",
  "fileTooLarge",
  "skillTooLarge",
  "tooManyFiles",
  "encoding",
  "manifest",
  "missing",
]);
export type SkillIssueRule = z.infer<typeof SkillIssueRule>;

const FORBIDDEN_PATH_CHARACTER = /[\p{Cc}\\:*?"<>|]/u;

export function isSkillKey(value: string): boolean {
  return value.length <= SKILL_KEY_MAX && /^[a-z][a-z0-9]*(-[a-z0-9]+)*$/.test(value);
}

export function isSkillPath(value: string): boolean {
  if (value.length === 0 || [...value].length > SKILL_PATH_MAX) return false;
  const segments = value.split("/");
  const [head = ""] = segments;
  return (
    segments.length >= 2 &&
    (SKILL_FILE_DIRS as readonly string[]).includes(head) &&
    segments.every(
      (segment) =>
        segment.length > 0 &&
        !segment.startsWith(".") &&
        !segment.endsWith(".") &&
        !segment.endsWith(" ") &&
        !FORBIDDEN_PATH_CHARACTER.test(segment),
    )
  );
}

const ToolKeyList = z.array(z.string().max(128)).max(SKILL_ALLOWED_TOOLS_MAX);

export const SkillModelDto = z.object({
  providerId: ProviderId,
  modelId: z.string().trim().min(1).max(SKILL_MODEL_MAX),
});
export type SkillModelDto = z.infer<typeof SkillModelDto>;

export const SkillIssueDto = z.object({
  field: z.string().max(300),
  rule: SkillIssueRule,
});
export type SkillIssueDto = z.infer<typeof SkillIssueDto>;

export const SkillFileDto = z.object({
  path: z.string().refine(isSkillPath),
  size: z.number().int().nonnegative(),
});
export type SkillFileDto = z.infer<typeof SkillFileDto>;

export const SkillDto = z.object({
  id: SkillId,
  key: z.string().min(1).max(SKILL_KEY_MAX),
  name: z.string().max(SKILL_NAME_MAX),
  description: z.string().max(SKILL_DESCRIPTION_MAX),
  allowedTools: ToolKeyList,
  modelOverride: SkillModelDto.nullable(),
  enabled: z.boolean(),
  source: SkillSource,
  status: SkillStatus,
  issues: z.array(SkillIssueDto).max(SKILL_ISSUES_MAX),
  fileCount: z.number().int().nonnegative(),
  createdAt: Timestamp,
  updatedAt: Timestamp,
});
export type SkillDto = z.infer<typeof SkillDto>;

export const SkillDetailDto = SkillDto.extend({
  instructions: z.string().max(SKILL_INSTRUCTIONS_MAX),
  files: z.array(SkillFileDto).max(SKILL_FILES_MAX),
});
export type SkillDetailDto = z.infer<typeof SkillDetailDto>;

export const SkillRef = z.object({ id: SkillId });
export type SkillRef = z.infer<typeof SkillRef>;

export const SkillFileInput = z.object({
  path: z.string().max(SKILL_PATH_MAX),
  content: z.string(),
  encoding: SkillFileEncoding.optional(),
});
export type SkillFileInput = z.infer<typeof SkillFileInput>;

export const CreateSkillInput = z.object({
  key: z.string().max(SKILL_KEY_MAX),
  name: z.string().max(SKILL_NAME_MAX),
  description: z.string().max(SKILL_DESCRIPTION_MAX),
  instructions: z.string().max(SKILL_INSTRUCTIONS_MAX),
  allowedTools: ToolKeyList.optional(),
  modelOverride: SkillModelDto.nullable().optional(),
  enabled: z.boolean().optional(),
  files: z.array(SkillFileInput).max(SKILL_FILES_MAX).optional(),
});
export type CreateSkillInput = z.infer<typeof CreateSkillInput>;

export const UpdateSkillInput = z.object({
  id: SkillId,
  key: z.string().max(SKILL_KEY_MAX).optional(),
  name: z.string().max(SKILL_NAME_MAX).optional(),
  description: z.string().max(SKILL_DESCRIPTION_MAX).optional(),
  instructions: z.string().max(SKILL_INSTRUCTIONS_MAX).optional(),
  allowedTools: ToolKeyList.optional(),
  modelOverride: SkillModelDto.nullable().optional(),
  enabled: z.boolean().optional(),
  files: z.array(SkillFileInput).max(SKILL_FILES_MAX).optional(),
  removeFiles: z.array(z.string().max(SKILL_PATH_MAX)).max(SKILL_FILES_MAX).optional(),
});
export type UpdateSkillInput = z.infer<typeof UpdateSkillInput>;

const Message = z
  .string()
  .max(SKILL_MESSAGE_MAX)
  .refine((value) => value.trim().length > 0);

export const SkillTestInput = z.object({
  message: Message,
  id: SkillId.optional(),
  description: z.string().max(SKILL_DESCRIPTION_MAX).optional(),
  threshold: z.number().min(0).max(1).optional(),
});
export type SkillTestInput = z.infer<typeof SkillTestInput>;

export const SkillMatchDto = z.object({
  id: SkillId.nullable(),
  key: z.string().max(SKILL_KEY_MAX),
  name: z.string().max(SKILL_NAME_MAX),
  score: z.number().min(-1).max(1),
  triggered: z.boolean(),
});
export type SkillMatchDto = z.infer<typeof SkillMatchDto>;

export const SkillTestResultDto = z.object({
  method: SkillMatchMethod,
  threshold: z.number().min(0).max(1),
  matches: z.array(SkillMatchDto),
});
export type SkillTestResultDto = z.infer<typeof SkillTestResultDto>;

export const SkillRunInput = z.object({
  id: SkillId,
  message: Message,
  providerId: ProviderId.optional(),
  modelId: z.string().trim().min(1).max(SKILL_MODEL_MAX).optional(),
});
export type SkillRunInput = z.infer<typeof SkillRunInput>;

export const SkillImportInput = z.object({
  path: z.string().trim().min(1).max(4096).optional(),
});
export type SkillImportInput = z.infer<typeof SkillImportInput>;

export const SkillImportResultDto = z.object({ skill: SkillDetailDto.nullable() });
export type SkillImportResultDto = z.infer<typeof SkillImportResultDto>;

export const SkillExportInput = z.object({
  id: SkillId,
  path: z.string().trim().min(1).max(4096).optional(),
});
export type SkillExportInput = z.infer<typeof SkillExportInput>;

export const SkillExportResultDto = z.object({ path: z.string().max(4096).nullable() });
export type SkillExportResultDto = z.infer<typeof SkillExportResultDto>;

export const SkillRemoveOutput = z.object({ id: SkillId, removed: z.literal(true) });
export type SkillRemoveOutput = z.infer<typeof SkillRemoveOutput>;

export const SkillTriggerSettingsDto = z.object({
  threshold: z.number().min(0).max(1).default(DEFAULT_SKILL_THRESHOLD),
  catalogueLimit: z
    .number()
    .int()
    .min(1)
    .max(SKILL_CATALOGUE_MAX)
    .default(DEFAULT_SKILL_CATALOGUE_LIMIT),
  embedding: SkillModelDto.optional(),
});
export type SkillTriggerSettingsDto = z.infer<typeof SkillTriggerSettingsDto>;
