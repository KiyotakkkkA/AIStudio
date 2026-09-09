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

export const ACCOUNT_FAMILIES = ["qwen-web", "deepseek-web"] as const;
export const AccountFamily = z.enum(ACCOUNT_FAMILIES);
export type AccountFamily = z.infer<typeof AccountFamily>;

export function isAccountFamily(family: AdapterFamily): family is AccountFamily {
  return (ACCOUNT_FAMILIES as readonly string[]).includes(family);
}

export const ACCOUNT_STATUSES = ["linked", "needs-relink", "revoked"] as const;
export const AccountStatus = z.enum(ACCOUNT_STATUSES);
export type AccountStatus = z.infer<typeof AccountStatus>;

export interface AccountIdentity {
  externalId: string;
  emailMasked?: string;
  displayName?: string;
  avatarUrl?: string;
  expiresAt?: number;
}
