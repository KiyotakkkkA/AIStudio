import { z } from "zod";

export const ADAPTER_FAMILIES = ["openai-compatible", "qwen-web", "deepseek-web", "local"] as const;
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

export const LOCAL_FAMILY = "local";

export const LOCAL_BASE_URL = "local://models";

export function isLocalFamily(family: AdapterFamily): family is typeof LOCAL_FAMILY {
  return family === LOCAL_FAMILY;
}

export const TUNABLE_PARAMETERS = ["temperature", "topK", "topP", "maxOutputTokens"] as const;
export const TunableParameter = z.enum(TUNABLE_PARAMETERS);
export type TunableParameter = z.infer<typeof TunableParameter>;

export const AdapterCapabilitiesDto = z.object({
  family: AdapterFamily,
  authModes: z.array(AuthMode),
  streaming: z.boolean(),
  liveModelList: z.boolean(),
  embedding: z.boolean(),
  image: z.boolean(),
  honours: z.record(TunableParameter, z.boolean()),
});
export type AdapterCapabilitiesDto = z.infer<typeof AdapterCapabilitiesDto>;

export const AdapterDescriptorDto = AdapterCapabilitiesDto.extend({
  implemented: z
    .boolean()
    .describe("false marks a family declared by the registry but not yet wired to a driver."),
});
export type AdapterDescriptorDto = z.infer<typeof AdapterDescriptorDto>;

export const DELTA_KINDS = ["text", "reasoning"] as const;
export const DeltaKind = z.enum(DELTA_KINDS);
export type DeltaKind = z.infer<typeof DeltaKind>;

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
