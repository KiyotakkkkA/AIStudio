import {
  ADAPTER_FAMILIES,
  TUNABLE_PARAMETERS,
  type AdapterFamily,
  type AuthMode,
  type TunableParameter,
} from "@zvs/shared";

export { ADAPTER_FAMILIES, TUNABLE_PARAMETERS };
export type { AuthMode, TunableParameter };

export interface AdapterCapabilities {
  readonly family: AdapterFamily;
  readonly authModes: readonly AuthMode[];
  readonly streaming: boolean;
  readonly liveModelList: boolean;
  readonly embedding: boolean;
  /** Image generation through ImageDriver; not image/vision input. */
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
