import { ADAPTER_FAMILIES, type AdapterDescriptorDto, type Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import { adapterEntry } from "../drivers/ai/adapters/index.ts";
import { toProbeResultDto, type ProviderService } from "../services/ProviderService.ts";

function adapterDescriptors(): AdapterDescriptorDto[] {
  return ADAPTER_FAMILIES.map((family) => {
    const entry = adapterEntry(family);
    return {
      family,
      authModes: [...entry.capabilities.authModes],
      streaming: entry.capabilities.streaming,
      liveModelList: entry.capabilities.liveModelList,
      embedding: entry.capabilities.embedding,
      image: entry.capabilities.image,
      honours: { ...entry.capabilities.honours },
      implemented: entry.implemented,
    };
  });
}

export type ProviderHandlers = Pick<
  IpcHandlers<Contract>,
  | "providers.adapters"
  | "providers.list"
  | "providers.get"
  | "providers.create"
  | "providers.update"
  | "providers.remove"
  | "providers.probe"
  | "providers.setDefaultModel"
  | "providers.refreshAll"
>;

export function createProviderHandlers(providers: ProviderService): ProviderHandlers {
  return {
    "providers.adapters": () => adapterDescriptors(),

    "providers.list": (filter) => providers.list(filter),

    "providers.get": ({ id }) => providers.get(id),

    "providers.create": (input) => providers.create(input),

    "providers.update": (input) => providers.update(input),

    "providers.remove": ({ id }) => {
      providers.remove(id);
      return { id, removed: true };
    },

    "providers.probe": async (request) =>
      toProbeResultDto(
        await providers.probe("draft" in request ? { draft: request.draft } : { id: request.id }),
      ),

    "providers.setDefaultModel": ({ id, modelId }) => providers.setDefaultModel(id, modelId),

    "providers.refreshAll": () => providers.refreshAll({ onlyIdle: true }),
  };
}
