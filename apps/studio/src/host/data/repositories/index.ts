import { PermissionRepository } from "./PermissionRepository.ts";
import { ChatRepository } from "./ChatRepository.ts";
import type { DatabaseHandle } from "../types.ts";
import { RunRepository } from "./RunRepository.ts";
import { SecretRepository } from "./SecretRepository.ts";
import { SettingRepository } from "./SettingRepository.ts";
import { ProviderRepository } from "./ProviderRepository.ts";
import { ModelRepository } from "./ModelRepository.ts";
import { AccountRepository } from "./AccountRepository.ts";
import { VectorStoreRepository } from "./VectorStoreRepository.ts";
import { VectorDocumentRepository } from "./VectorDocumentRepository.ts";
import { VectorSourceRepository } from "./VectorSourceRepository.ts";
import { DownloadRepository } from "./DownloadRepository.ts";
export { VectorStoreRepository, VectorDocumentRepository, VectorSourceRepository };
export { DownloadRepository };
export type { DownloadPatch } from "./DownloadRepository.ts";

export { Repository } from "./Repository.ts";
export { SecretRepository } from "./SecretRepository.ts";
export { SettingRepository } from "./SettingRepository.ts";
export type { SecretDraft, SecretFilter, SecretPatch } from "./SecretRepository.ts";
export { ProviderRepository } from "./ProviderRepository.ts";
export { ModelRepository } from "./ModelRepository.ts";
export type { ProviderDraft, ProviderPatch } from "./ProviderRepository.ts";
export { AccountRepository } from "./AccountRepository.ts";
export type { AccountDraft } from "./AccountRepository.ts";

export interface Repositories {
  readonly chat: ChatRepository;
  readonly permissions: PermissionRepository;
  readonly runs: RunRepository;
  readonly vectorStores: VectorStoreRepository;
  readonly vectorDocuments: VectorDocumentRepository;
  readonly vectorSources: VectorSourceRepository;
  readonly downloads: DownloadRepository;
  readonly secrets: SecretRepository;
  readonly settings: SettingRepository;
  readonly providers: ProviderRepository;
  readonly models: ModelRepository;
  readonly accounts: AccountRepository;
}

export function createRepositories(db: DatabaseHandle): Repositories {
  return {
    chat: new ChatRepository(db),
    permissions: new PermissionRepository(db),
    runs: new RunRepository(db),
    vectorStores: new VectorStoreRepository(db),
    vectorDocuments: new VectorDocumentRepository(db),
    vectorSources: new VectorSourceRepository(db),
    downloads: new DownloadRepository(db),
    secrets: new SecretRepository(db),
    settings: new SettingRepository(db),
    providers: new ProviderRepository(db),
    models: new ModelRepository(db),
    accounts: new AccountRepository(db),
  };
}
