import {
  AppError,
  AppErrorCode,
  type CatalogueSource,
  type ChecksumDto,
  type DownloadItemKind,
} from "@zvs/shared";
import type { Logger } from "../platform/logger.ts";
import { CURATED_CATALOGUE } from "./curated.ts";

/**
 * One downloadable artefact, shaped so a second source can be added without reshaping the
 * table: nothing here is specific to where the entry came from.
 */
export interface CatalogueItem {
  readonly ref: string;
  readonly kind: DownloadItemKind;
  readonly source: CatalogueSource;
  readonly name: string;
  readonly displayName: string;
  readonly description: string;
  readonly version?: string;
  readonly sizeBytes: number;
  readonly url: string;
  readonly fileName: string;
  readonly checksum?: ChecksumDto;
  /** The source's own identity for this version, compared against what is installed. */
  readonly digest?: string;
  readonly tags: readonly string[];
  readonly dimension?: number;
}

/** What a source reports as already present on this machine. */
export interface InstalledItem {
  readonly kind: DownloadItemKind;
  readonly name: string;
  readonly version?: string;
  readonly sizeBytes: number;
  readonly digest?: string;
}

export interface CatalogueProvider {
  readonly id: CatalogueSource;
  /** Whether reaching this source leaves the machine. Only live providers are refreshed. */
  readonly live: boolean;
  list(signal?: AbortSignal): Promise<readonly CatalogueItem[]>;
  installed?(signal?: AbortSignal): Promise<readonly InstalledItem[]>;
}

export const curatedProvider: CatalogueProvider = {
  id: "curated",
  live: false,
  list: () => Promise.resolve(CURATED_CATALOGUE),
};

export function installedKey(kind: DownloadItemKind, name: string): string {
  return `${kind}:${name}`;
}

interface OllamaTag {
  name?: unknown;
  size?: unknown;
  digest?: unknown;
  details?: { quantization_level?: unknown } | undefined;
}

/**
 * The models Ollama already has. This is a loopback call to the daemon the user configured,
 * not an outbound one, and it is the source of truth for update detection: a tag whose digest
 * differs from the catalogue's is the mockup's amber `update` state.
 */
export function ollamaProvider(options: {
  baseUrl: () => string | undefined;
  fetch?: typeof globalThis.fetch;
  logger?: Logger;
}): CatalogueProvider {
  const request = options.fetch ?? globalThis.fetch;
  return {
    id: "ollama",
    live: true,
    list: () => Promise.resolve([]),
    async installed(signal) {
      const baseUrl = options.baseUrl();
      if (baseUrl === undefined) return [];
      let payload: unknown;
      try {
        const response = await request(`${baseUrl.replace(/\/+$/, "")}/api/tags`, {
          ...(signal === undefined ? {} : { signal }),
        });
        if (!response.ok)
          throw new AppError(
            AppErrorCode.PROVIDER_UNREACHABLE,
            `Ollama ответил ${String(response.status)}`,
          );
        payload = await response.json();
      } catch (error: unknown) {
        options.logger?.log("warn", "downloads", "Could not list the installed Ollama models", {
          baseUrl,
          error: String(error),
        });
        return [];
      }
      const models =
        payload !== null && typeof payload === "object" && "models" in payload
          ? (payload as { models: unknown }).models
          : undefined;
      if (!Array.isArray(models)) return [];
      return models.flatMap((raw: OllamaTag) => {
        if (typeof raw.name !== "string") return [];
        const digest =
          typeof raw.digest === "string" ? raw.digest.replace(/^sha256:/, "") : undefined;
        const quantisation = raw.details?.quantization_level;
        return [
          {
            kind: (/embed/i.test(raw.name) ? "embedding" : "model") as DownloadItemKind,
            name: raw.name,
            sizeBytes: typeof raw.size === "number" ? raw.size : 0,
            ...(typeof quantisation === "string" ? { version: quantisation } : {}),
            ...(digest === undefined ? {} : { digest }),
          },
        ];
      });
    },
  };
}

export interface CatalogueServiceOptions {
  providers?: readonly CatalogueProvider[];
  /** How long a merged catalogue and installed snapshot stay usable. */
  ttlMs?: number;
  clock?: () => number;
  logger?: Logger;
}

interface Snapshot<T> {
  value: T;
  at: number;
}

const DEFAULT_TTL_MS = 60_000;

/**
 * Merges every source into one provider-agnostic list and caches it briefly, so opening the
 * Downloads page does not re-query anything. `refresh` is the user's explicit live query.
 */
export class CatalogueService {
  private readonly providers: readonly CatalogueProvider[];
  private items: Snapshot<readonly CatalogueItem[]> | undefined;
  private present: Snapshot<ReadonlyMap<string, InstalledItem>> | undefined;

  constructor(private readonly options: CatalogueServiceOptions = {}) {
    this.providers = options.providers ?? [curatedProvider];
  }

  async list(refresh = false): Promise<readonly CatalogueItem[]> {
    if (!refresh && this.fresh(this.items)) return this.items.value;
    const collected = new Map<string, CatalogueItem>();
    for (const provider of this.providers) {
      if (!refresh && provider.live) continue;
      for (const item of await this.safely(provider, () => provider.list()))
        collected.set(item.ref, item);
    }
    // A refresh that reaches nothing must not empty the page.
    const merged = collected.size === 0 && this.items ? this.items.value : [...collected.values()];
    this.items = { value: merged, at: this.now() };
    return merged;
  }

  async installed(refresh = false): Promise<ReadonlyMap<string, InstalledItem>> {
    if (!refresh && this.fresh(this.present)) return this.present.value;
    const collected = new Map<string, InstalledItem>();
    for (const provider of this.providers) {
      if (provider.installed === undefined) continue;
      for (const item of await this.safely(provider, () => provider.installed!()))
        collected.set(installedKey(item.kind, item.name), item);
    }
    this.present = { value: collected, at: this.now() };
    return collected;
  }

  async find(ref: string): Promise<CatalogueItem> {
    const item = (await this.list()).find((candidate) => candidate.ref === ref);
    if (!item) throw new AppError(AppErrorCode.NOT_FOUND, "Элемент каталога не найден");
    return item;
  }

  /** Drops the caches so the next read rebuilds them. */
  invalidate(): void {
    this.items = undefined;
    this.present = undefined;
  }

  private async safely<T>(
    provider: CatalogueProvider,
    read: () => Promise<readonly T[]>,
  ): Promise<readonly T[]> {
    try {
      return await read();
    } catch (error: unknown) {
      this.options.logger?.log("warn", "downloads", "A catalogue source failed", {
        source: provider.id,
        error: String(error),
      });
      return [];
    }
  }

  private fresh<T>(snapshot: Snapshot<T> | undefined): snapshot is Snapshot<T> {
    if (!snapshot) return false;
    return this.now() - snapshot.at < (this.options.ttlMs ?? DEFAULT_TTL_MS);
  }

  private now(): number {
    return (this.options.clock ?? Date.now)();
  }
}
