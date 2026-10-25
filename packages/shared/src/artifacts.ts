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
