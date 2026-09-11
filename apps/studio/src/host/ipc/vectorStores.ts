import type { Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { VectorStoreService } from "../services/VectorStoreService.ts";

export function createVectorStoreHandlers(
  service: VectorStoreService,
): Pick<
  IpcHandlers<Contract>,
  | "vectorStores.list"
  | "vectorStores.get"
  | "vectorStores.create"
  | "vectorStores.update"
  | "vectorStores.remove"
  | "vectorStores.search"
  | "vectorStores.reconcile"
> {
  return {
    "vectorStores.list": () => service.list(),
    "vectorStores.get": ({ id }) => service.get(id),
    "vectorStores.create": (input) => service.create(input),
    "vectorStores.update": (input) => service.update(input),
    "vectorStores.remove": async ({ id }) => {
      await service.remove(id);
      return { id, removed: true };
    },
    "vectorStores.search": ({ storeId, query, k, minScore }) =>
      service.search(storeId, query, { k, minScore }),
    "vectorStores.reconcile": ({ id }) => service.reconcile(id),
  };
}
