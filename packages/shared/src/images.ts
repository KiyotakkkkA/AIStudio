import { z } from "zod";
import { ConversationId } from "./primitives/branded.js";

export const IMAGES_MAX = 5;
export const IMAGE_BYTES_MAX = 20 * 1024 * 1024;
export const IMAGE_DATA_URL_MAX = 30 * 1024 * 1024;
export const ImageAttachmentDto = z.object({
  id: z.uuid(),
  name: z.string().trim().min(1).max(255),
  mimeType: z.enum(["image/png", "image/jpeg", "image/gif", "image/webp"]),
  sizeBytes: z.number().int().min(1).max(IMAGE_BYTES_MAX),
});
export type ImageAttachmentDto = z.infer<typeof ImageAttachmentDto>;
export const ImageAttachments = z
  .array(ImageAttachmentDto)
  .max(IMAGES_MAX)
  .refine((images) => new Set(images.map((image) => image.id)).size === images.length);
export const ImageReadInput = z.object({
  id: ConversationId,
  image: ImageAttachmentDto,
});
export type ImageReadInput = z.infer<typeof ImageReadInput>;
export const ImageDataDto = z.object({
  dataUrl: z.string().min(1).max(IMAGE_DATA_URL_MAX),
});
export type ImageDataDto = z.infer<typeof ImageDataDto>;
