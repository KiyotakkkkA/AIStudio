import { z } from "zod";

export const ARTIFACT_ID_MAX = 512;
export const ARTIFACT_NAME_MAX = 255;
export const ARTIFACT_FORMAT_MAX = 16;
export const ARTIFACT_PATH_MAX = 32767;

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
