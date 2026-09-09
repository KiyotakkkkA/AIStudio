import type { DatabaseHandle } from "../types.ts";
import { SecretRepository } from "./SecretRepository.ts";
import { SettingRepository } from "./SettingRepository.ts";
import { ProviderRepository } from "./ProviderRepository.ts";
import { ModelRepository } from "./ModelRepository.ts";

export { Repository } from "./Repository.ts";
export { SecretRepository } from "./SecretRepository.ts";
export { SettingRepository } from "./SettingRepository.ts";
export type { SecretDraft, SecretFilter, SecretPatch } from "./SecretRepository.ts";
export { ProviderRepository } from "./ProviderRepository.ts";
export { ModelRepository } from "./ModelRepository.ts";
export type { ProviderDraft, ProviderFilter, ProviderPatch } from "./ProviderRepository.ts";
export type { DiscoveredModel } from "./ModelRepository.ts";

export interface Repositories {
  readonly secrets: SecretRepository;
  readonly settings: SettingRepository;
  readonly providers: ProviderRepository;
  readonly models: ModelRepository;
}

export function createRepositories(db: DatabaseHandle): Repositories {
  return {
    secrets: new SecretRepository(db),
    settings: new SettingRepository(db),
    providers: new ProviderRepository(db),
    models: new ModelRepository(db),
  };
}
