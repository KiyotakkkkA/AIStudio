import { z } from "zod";

export const BROWSER_LINK_TARGETS = ["studio", "system"] as const;
export const BrowserLinkTarget = z.enum(BROWSER_LINK_TARGETS);
export type BrowserLinkTarget = z.infer<typeof BrowserLinkTarget>;

export const BrowserSettingsDto = z.object({
  enabled: z.boolean(),
  webLinks: BrowserLinkTarget,
  localLinks: BrowserLinkTarget,
  fullUrl: z.boolean(),
  passwordAutosave: z.boolean(),
  formAutofill: z.boolean(),
  downloadDir: z.string().min(1).max(32_767).nullable(),
  askDownload: z.boolean(),
});
export type BrowserSettingsDto = z.infer<typeof BrowserSettingsDto>;
