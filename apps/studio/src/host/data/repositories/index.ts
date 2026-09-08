import type { DatabaseHandle } from "../types.ts";
import { SettingRepository } from "./SettingRepository.ts";

export { Repository } from "./Repository.ts";
export { SettingRepository } from "./SettingRepository.ts";

export interface Repositories {
  readonly settings: SettingRepository;
}

export function createRepositories(db: DatabaseHandle): Repositories {
  return { settings: new SettingRepository(db) };
}
