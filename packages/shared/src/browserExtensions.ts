import { z } from "zod";

export const EXTENSION_ID_MAX = 128;
export const EXTENSION_TEXT_MAX = 1024;
export const EXTENSIONS_MAX = 64;

const ExtensionId = z
  .string()
  .min(1)
  .max(EXTENSION_ID_MAX)
  .regex(/^[A-Za-z0-9_-][A-Za-z0-9._-]*$/);

export const BrowserExtensionDto = z.object({
  id: ExtensionId,
  name: z.string().min(1).max(EXTENSION_TEXT_MAX),
  version: z.string().max(EXTENSION_TEXT_MAX),
  description: z.string().max(EXTENSION_TEXT_MAX),
  enabled: z.boolean(),
});
export type BrowserExtensionDto = z.infer<typeof BrowserExtensionDto>;

export const BrowserExtensionsDto = z.object({
  items: z.array(BrowserExtensionDto).max(EXTENSIONS_MAX),
});
export type BrowserExtensionsDto = z.infer<typeof BrowserExtensionsDto>;

export const BrowserExtensionRef = z.object({ id: ExtensionId });
export type BrowserExtensionRef = z.infer<typeof BrowserExtensionRef>;

export const BrowserExtensionToggleInput = z.object({ id: ExtensionId, enabled: z.boolean() });
export type BrowserExtensionToggleInput = z.infer<typeof BrowserExtensionToggleInput>;
