import type { DatabaseHandle } from "../types.ts";
import { SecretRepository } from "./SecretRepository.ts";
import { SettingRepository } from "./SettingRepository.ts";

export { Repository } from "./Repository.ts";
export { SecretRepository } from "./SecretRepository.ts";
export { SettingRepository } from "./SettingRepository.ts";
export type { SecretDraft, SecretFilter, SecretPatch } from "./SecretRepository.ts";

export interface Repositories {
  readonly secrets: SecretRepository;
  readonly settings: SettingRepository;
}

export function createRepositories(db: DatabaseHandle): Repositories {
  return { secrets: new SecretRepository(db), settings: new SettingRepository(db) };
}
