import { PROVIDER_CAPABILITIES, type ProviderCapability } from "@zvs/shared";
import { CAPABILITY_LABELS } from "./providerPresentation";

export const ACCOUNTS_TAB = "accounts";

export type ProvidersTab = ProviderCapability | typeof ACCOUNTS_TAB;

export const PROVIDERS_TABS: readonly ProvidersTab[] = [...PROVIDER_CAPABILITIES, ACCOUNTS_TAB];

export const TAB_LABELS: Record<ProvidersTab, string> = {
  ...CAPABILITY_LABELS,
  [ACCOUNTS_TAB]: "Аккаунты",
};

export function isCapabilityTab(tab: ProvidersTab): tab is ProviderCapability {
  return tab !== ACCOUNTS_TAB;
}
