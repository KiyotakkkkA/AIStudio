import { AppError, AppErrorCode, type Contract, type VectorIndexInput } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { VectorStoreService } from "../services/VectorStoreService.ts";
import type { IndexingService } from "../indexing/IndexingService.ts";
import type { RunService } from "../services/RunService.ts";

export type SourcePicker = (kind: "file" | "folder") => Promise<string[]>;

export interface VectorStoreHandlerDependencies {
  service: VectorStoreService;
  indexing?: IndexingService;
  runs?: RunService;
  pick?: SourcePicker;
}

type VectorStoreChannels =
  | "vectorStores.list"
  | "vectorStores.get"
  | "vectorStores.create"
  | "vectorStores.update"
  | "vectorStores.remove"
  | "vectorStores.search"
  | "vectorStores.searchTimed"
  | "vectorStores.reconcile"
  | "vectorStores.sources.list"
  | "vectorStores.sources.add"
  | "vectorStores.sources.remove"
  | "vectorStores.sources.pick"
  | "vectorStores.documents.list"
  | "vectorStores.documents.remove"
  | "vectorStores.index";

export function createVectorStoreHandlers(
  dependencies: VectorStoreService | VectorStoreHandlerDependencies,
): Pick<IpcHandlers<Contract>, VectorStoreChannels> {
  const options: VectorStoreHandlerDependencies =
    "service" in dependencies ? dependencies : { service: dependencies };
  const service = options.service;
  const indexing = (): IndexingService => {
    if (!options.indexing)
      throw new AppError(AppErrorCode.CONFLICT, "Сервис индексации недоступен");
    return options.indexing;
  };
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
    "vectorStores.searchTimed": ({ storeId, query, k, minScore }) =>
      service.searchTimed(storeId, query, { k, minScore }),
    "vectorStores.reconcile": ({ id }) => service.reconcile(id),
    "vectorStores.sources.list": ({ id }) => indexing().listSources(id),
    "vectorStores.sources.add": (input) => indexing().addSource(input),
    "vectorStores.sources.remove": ({ id }) => {
      indexing().removeSource(id);
      return { id, removed: true };
    },
    "vectorStores.sources.pick": async ({ kind }) => {
      if (!options.pick) throw new AppError(AppErrorCode.CONFLICT, "Выбор файлов недоступен");
      return { paths: await options.pick(kind) };
    },
    "vectorStores.documents.list": ({ id }) => indexing().listDocuments(id),
    "vectorStores.documents.remove": async ({ storeId, id }) => {
      await indexing().removeDocument(storeId, id);
      return { id, removed: true };
    },
    "vectorStores.index": (input) => startIndexRun(options, input),
  };
}

function startIndexRun(
  options: VectorStoreHandlerDependencies,
  input: VectorIndexInput,
): ReturnType<RunService["start"]> {
  if (!options.runs) throw new AppError(AppErrorCode.CONFLICT, "Сервис выполнения недоступен");
  const store = options.service.requireStore(input.storeId);
  return options.runs.start({
    kind: "indexing",
    subjectId: store.id,
    title: `Индексация «${store.name}»`,
    graph: {
      nodes: [
        {
          id: "index",
          type: "vector.index",
          dependencies: [],
          input: { storeId: input.storeId, full: input.full },
          bindings: {},
          retry: { maxAttempts: 1, backoffMs: 0 },
        },
      ],
    },
    input: null,
    concurrency: 1,
  });
}
