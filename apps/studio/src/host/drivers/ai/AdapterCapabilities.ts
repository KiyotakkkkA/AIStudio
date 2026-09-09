import { ADAPTER_FAMILIES, type AdapterFamily, type AuthMode } from "@zvs/shared";

export { ADAPTER_FAMILIES };
export type { AdapterFamily, AuthMode };

export const TUNABLE_PARAMETERS = ["temperature", "topK", "topP", "maxOutputTokens"] as const;
export type TunableParameter = (typeof TUNABLE_PARAMETERS)[number];

export interface AdapterCapabilities {
  readonly family: AdapterFamily;
  readonly authModes: readonly AuthMode[];
  readonly streaming: boolean;
  readonly liveModelList: boolean;
  readonly embedding: boolean;
  readonly image: boolean;
  readonly honours: Readonly<Record<TunableParameter, boolean>>;
}

export function supportsAuthMode(capabilities: AdapterCapabilities, mode: AuthMode): boolean {
  return capabilities.authModes.includes(mode);
}

export function unsupportedParameters(
  capabilities: AdapterCapabilities,
  requested: Partial<Record<TunableParameter, unknown>>,
): TunableParameter[] {
  return TUNABLE_PARAMETERS.filter(
    (parameter) => requested[parameter] !== undefined && !capabilities.honours[parameter],
  );
}
