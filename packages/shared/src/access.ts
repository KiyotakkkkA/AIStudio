import { z } from "zod";
import { BrowserSitesDto } from "./browserSites.js";
import { AppGrantDto } from "./computer.js";
import { ExecPolicyDto, UserRulesDto } from "./execPolicy.js";
import { FolderGrantDto } from "./folders.js";
import { ToolTier } from "./tools/ToolDto.js";

export const ToolGrantDto = z.object({
  id: z.string().min(1).max(64),
  tool: z.string().min(1).max(128),
  displayName: z.string().max(256),
  scope: z.string().min(1).max(512),
  tier: ToolTier,
  grantedBy: z.string().min(1).max(32),
  grantedAt: z.number().int(),
});
export type ToolGrantDto = z.infer<typeof ToolGrantDto>;

export const AccessListDto = z.object({
  policy: ExecPolicyDto,
  folders: z.array(FolderGrantDto).max(10_000),
  sites: BrowserSitesDto,
  apps: z.array(AppGrantDto).max(10_000),
  tools: z.array(ToolGrantDto).max(10_000),
  rules: UserRulesDto,
});
export type AccessListDto = z.infer<typeof AccessListDto>;
