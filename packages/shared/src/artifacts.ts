import { z } from "zod";

export const ARTIFACT_ID_MAX = 512;
export const ARTIFACT_NAME_MAX = 255;
export const ARTIFACT_FORMAT_MAX = 16;
export const ARTIFACT_PATH_MAX = 32767;
export const BROWSER_ID_MAX = 256;
export const BROWSERS_MAX = 32;

export const ArtifactId = z
  .string()
  .min(1)
  .max(ARTIFACT_ID_MAX)
  .refine((id) => {
    const parts = id.split("/");
    return (
      parts.length === 2 &&
      parts.every(
        (part) => part.length > 0 && part !== "." && part !== ".." && !part.includes("\\"),
      )
    );
  });

export const ArtifactDto = z.object({
  id: ArtifactId,
  name: z.string().min(1).max(ARTIFACT_NAME_MAX),
  format: z.string().min(1).max(ARTIFACT_FORMAT_MAX),
  sizeBytes: z.number().int().nonnegative(),
});
export type ArtifactDto = z.infer<typeof ArtifactDto>;

export const ArtifactRef = z.object({ id: ArtifactId });
export type ArtifactRef = z.infer<typeof ArtifactRef>;

export const ArtifactSaveResult = z.object({
  path: z.string().min(1).max(ARTIFACT_PATH_MAX).optional(),
});
export type ArtifactSaveResult = z.infer<typeof ArtifactSaveResult>;

export const ArtifactApplicationDto = z.object({
  name: z.string().min(1).max(256).optional(),
});
export type ArtifactApplicationDto = z.infer<typeof ArtifactApplicationDto>;

export const InstalledBrowserDto = z.object({
  id: z.string().min(1).max(BROWSER_ID_MAX),
  name: z.string().min(1).max(256),
});
export type InstalledBrowserDto = z.infer<typeof InstalledBrowserDto>;

export const ArtifactBrowsersDto = z.object({
  browsers: z.array(InstalledBrowserDto).max(BROWSERS_MAX),
});
export type ArtifactBrowsersDto = z.infer<typeof ArtifactBrowsersDto>;

export const OpenArtifactInBrowserInput = z.object({
  id: ArtifactId,
  browser: z.string().min(1).max(BROWSER_ID_MAX),
});
export type OpenArtifactInBrowserInput = z.infer<typeof OpenArtifactInBrowserInput>;

export const DASHBOARD_INTERVAL_MIN = 15;
export const DASHBOARD_INTERVAL_MAX = 86400;
export const DASHBOARD_SCRIPTS_MAX = 16;
export const DASHBOARD_SCRIPT_SOURCE_MAX = 65536;
export const DASHBOARD_ERROR_MAX = 2000;

export const DashboardInterval = z
  .number()
  .int()
  .refine(
    (value) => value === 0 || (value >= DASHBOARD_INTERVAL_MIN && value <= DASHBOARD_INTERVAL_MAX),
  );

export const Sha256Hex = z.string().regex(/^[0-9a-f]{64}$/);

export const DashboardScriptDto = z.object({
  name: z.string().min(1).max(ARTIFACT_NAME_MAX),
  sha256: Sha256Hex,
  approved: z.boolean(),
  source: z.string().max(DASHBOARD_SCRIPT_SOURCE_MAX),
  truncated: z.boolean(),
});
export type DashboardScriptDto = z.infer<typeof DashboardScriptDto>;

export const DashboardStatusDto = z.object({
  id: ArtifactId,
  live: z.boolean(),
  interval: DashboardInterval,
  specInterval: DashboardInterval,
  overridden: z.boolean(),
  refreshedAt: z.number().int().nonnegative().optional(),
  refreshing: z.boolean(),
  error: z.string().min(1).max(DASHBOARD_ERROR_MAX).optional(),
  scripts: z.array(DashboardScriptDto).max(DASHBOARD_SCRIPTS_MAX),
});
export type DashboardStatusDto = z.infer<typeof DashboardStatusDto>;

export const DashboardApproveInput = z.object({
  id: ArtifactId,
  hashes: z.array(Sha256Hex).min(1).max(DASHBOARD_SCRIPTS_MAX),
});
export type DashboardApproveInput = z.infer<typeof DashboardApproveInput>;

export const DashboardIntervalInput = z.object({
  id: ArtifactId,
  interval: DashboardInterval.nullable().optional(),
});
export type DashboardIntervalInput = z.infer<typeof DashboardIntervalInput>;

export const GALLERY_ITEMS_MAX = 4096;
export const GALLERY_OWNER_MAX = 256;
export const GALLERY_THUMBNAIL_BYTES_MAX = 16 * 1024 * 1024;
export const GALLERY_THUMBNAIL_DATA_MAX = Math.ceil(GALLERY_THUMBNAIL_BYTES_MAX / 3) * 4;

export const GalleryKind = z.enum(["dashboard", "document", "table", "image", "other"]);
export type GalleryKind = z.infer<typeof GalleryKind>;

export const GalleryItemDto = z.object({
  id: ArtifactId,
  name: z.string().min(1).max(ARTIFACT_NAME_MAX),
  format: z.string().min(1).max(ARTIFACT_FORMAT_MAX),
  kind: GalleryKind,
  sizeBytes: z.number().int().nonnegative(),
  modifiedAt: z.number().int().nonnegative(),
  owner: z.string().min(1).max(GALLERY_OWNER_MAX),
  live: z.boolean(),
});
export type GalleryItemDto = z.infer<typeof GalleryItemDto>;

export const GalleryListDto = z.object({
  items: z.array(GalleryItemDto).max(GALLERY_ITEMS_MAX),
});
export type GalleryListDto = z.infer<typeof GalleryListDto>;

export const GalleryMediaType = z.enum([
  "image/png",
  "image/jpeg",
  "image/svg+xml",
  "application/pdf",
]);
export type GalleryMediaType = z.infer<typeof GalleryMediaType>;

export const GalleryThumbnailDto = z.union([
  z.object({
    mediaType: GalleryMediaType,
    data: z.string().min(1).max(GALLERY_THUMBNAIL_DATA_MAX),
  }),
  z.object({}).strict(),
]);
export type GalleryThumbnailDto = z.infer<typeof GalleryThumbnailDto>;
