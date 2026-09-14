import { readdir, stat } from "node:fs/promises";
import { basename, extname, join } from "node:path";
import { AppError, AppErrorCode } from "@zvs/shared";
import type { Logger } from "../../../platform/logger.ts";
import type { AdapterCapabilities } from "../AdapterCapabilities.ts";
import type { DiscoveredModel, EmbeddingDriver } from "../ports.ts";

export const LOCAL_CAPABILITIES: AdapterCapabilities = {
  family: "local",
  authModes: ["api"],
  streaming: false,
  liveModelList: true,
  embedding: true,
  image: false,
  honours: { temperature: false, topK: false, topP: false, maxOutputTokens: false },
};

const WEIGHT_EXTENSIONS = new Set([".gguf", ".safetensors", ".bin", ".onnx"]);

export interface LocalModelStore {
  list(signal: AbortSignal): Promise<DiscoveredModel[]>;
}

export interface FileSystemModelStoreOptions {
  root: string;
  directories?: readonly string[];
  logger?: Logger;
}

export class FileSystemModelStore implements LocalModelStore {
  constructor(private readonly options: FileSystemModelStoreOptions) {}

  async list(signal: AbortSignal): Promise<DiscoveredModel[]> {
    const folders = this.options.directories ?? ["models", "embeddings"];
    const found = new Map<string, DiscoveredModel>();
    for (const folder of folders) {
      signal.throwIfAborted();
      for (const model of await this.scan(join(this.options.root, folder), signal)) {
        if (!found.has(model.externalId)) found.set(model.externalId, model);
      }
    }
    return [...found.values()].sort((left, right) =>
      left.displayName.localeCompare(right.displayName, "ru"),
    );
  }

  private async scan(directory: string, signal: AbortSignal): Promise<DiscoveredModel[]> {
    let entries;
    try {
      entries = await readdir(directory, { withFileTypes: true });
    } catch {
      return [];
    }
    const models: DiscoveredModel[] = [];
    for (const entry of entries) {
      signal.throwIfAborted();
      if (!entry.isFile()) continue;
      const extension = extname(entry.name).toLowerCase();
      if (!WEIGHT_EXTENSIONS.has(extension)) continue;
      const path = join(directory, entry.name);
      let sizeBytes: number | null = null;
      try {
        sizeBytes = (await stat(path)).size;
      } catch {
        continue;
      }
      models.push({
        externalId: entry.name,
        displayName: basename(entry.name, extension),
        family: extension.replace(".", ""),
        contextWindow: null,
        maxOutput: null,
        sizeBytes,
        capabilities: [],
      });
    }
    return models;
  }
}

export class LocalEmbeddingAdapter implements EmbeddingDriver {
  constructor(
    private readonly store: LocalModelStore,
    private readonly logger?: Logger,
  ) {}

  capabilities(): AdapterCapabilities {
    return LOCAL_CAPABILITIES;
  }

  dimensions(): number | null {
    return null;
  }

  async listModels(signal: AbortSignal): Promise<DiscoveredModel[]> {
    const models = await this.store.list(signal);
    this.logger?.log("debug", "ai", "Listed the local model files", { count: models.length });
    return models;
  }

  async embed(
    texts: readonly string[],
    model: string,
    signal: AbortSignal,
  ): Promise<Float32Array[]> {
    signal.throwIfAborted();
    await Promise.resolve();
    throw new AppError(
      AppErrorCode.CONFLICT,
      "Локальный движок эмбеддингов ещё не подключён: файл модели виден, но посчитать вектор нечем",
      { details: { family: "local", model, texts: texts.length } },
    );
  }
}
