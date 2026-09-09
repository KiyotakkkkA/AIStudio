import { z } from "zod";

export const ADAPTER_FAMILIES = [
  "openai-compatible",
  "anthropic",
  "qwen-web",
  "deepseek-web",
] as const;
export const AdapterFamily = z.enum(ADAPTER_FAMILIES);
export type AdapterFamily = z.infer<typeof AdapterFamily>;

export const AUTH_MODES = ["api", "account"] as const;
export const AuthMode = z.enum(AUTH_MODES);
export type AuthMode = z.infer<typeof AuthMode>;
