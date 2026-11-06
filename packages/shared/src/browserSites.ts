import { z } from "zod";

export const SITE_INPUT_MAX = 2048;
export const BROWSER_SITES_MAX = 1000;

export const BROWSER_SITE_ACTIONS = ["browse", "click", "type", "submit", "download"] as const;
export const BrowserSiteAction = z.enum(BROWSER_SITE_ACTIONS);
export type BrowserSiteAction = z.infer<typeof BrowserSiteAction>;

export const BROWSER_SITE_DECISIONS = ["ask", "allow", "block"] as const;
export const BrowserSiteDecision = z.enum(BROWSER_SITE_DECISIONS);
export type BrowserSiteDecision = z.infer<typeof BrowserSiteDecision>;

export const BrowserSiteDefaultsDto = z.object({
  browse: BrowserSiteDecision,
  click: BrowserSiteDecision,
  type: BrowserSiteDecision,
  submit: BrowserSiteDecision,
  download: BrowserSiteDecision,
});
export type BrowserSiteDefaultsDto = z.infer<typeof BrowserSiteDefaultsDto>;

export const BrowserSiteRulesDto = BrowserSiteDefaultsDto.partial();
export type BrowserSiteRulesDto = z.infer<typeof BrowserSiteRulesDto>;

export const BrowserSiteDto = z.object({
  origin: z.string().url().max(8192),
  rules: BrowserSiteRulesDto,
  updatedAt: z.number().int().nonnegative(),
});
export type BrowserSiteDto = z.infer<typeof BrowserSiteDto>;

export const BrowserSitesDto = z.object({
  defaults: BrowserSiteDefaultsDto,
  sites: z.array(BrowserSiteDto).max(BROWSER_SITES_MAX),
});
export type BrowserSitesDto = z.infer<typeof BrowserSitesDto>;

export const BrowserSiteDefaultInput = z.object({
  action: BrowserSiteAction,
  decision: BrowserSiteDecision,
});
export type BrowserSiteDefaultInput = z.infer<typeof BrowserSiteDefaultInput>;

export const BrowserSitePutInput = z.object({
  site: z.string().min(1).max(SITE_INPUT_MAX),
  rules: BrowserSiteRulesDto,
});
export type BrowserSitePutInput = z.infer<typeof BrowserSitePutInput>;

export const BrowserSiteRef = z.object({ origin: z.string().url().max(8192) });
export type BrowserSiteRef = z.infer<typeof BrowserSiteRef>;
