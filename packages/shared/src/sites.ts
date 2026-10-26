import { z } from "zod";

export const SITE_URL_MAX = 4096;
export const SITE_ICON_BYTES_MAX = 65536;
export const SITE_ICON_DATA_MAX = Math.ceil(SITE_ICON_BYTES_MAX / 3) * 4;
export const SITE_ICON_MIMES = [
  "image/png",
  "image/x-icon",
  "image/jpeg",
  "image/gif",
  "image/webp",
] as const;

export const WebUrl = z.url({ protocol: /^https?$/ }).max(SITE_URL_MAX);

export const OpenSiteInput = z.object({ url: WebUrl });
export type OpenSiteInput = z.infer<typeof OpenSiteInput>;

export const SiteIconInput = z.object({ url: WebUrl, icon: WebUrl.optional() });
export type SiteIconInput = z.infer<typeof SiteIconInput>;

export const SiteIconDto = z.object({
  mime: z.enum(SITE_ICON_MIMES),
  data: z.string().min(1).max(SITE_ICON_DATA_MAX),
});
export type SiteIconDto = z.infer<typeof SiteIconDto>;
