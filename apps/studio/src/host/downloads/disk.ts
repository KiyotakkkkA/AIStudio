import { readdir, stat, statfs } from "node:fs/promises";
import { join } from "node:path";
import type { DiskCategory, DiskUsageDto, Timestamp } from "@zvs/shared";
import { DOWNLOAD_DIRECTORIES } from "../platform/paths.ts";
import type { Logger } from "../platform/logger.ts";

export interface FreeSpace {
  totalBytes: number;
  freeBytes: number;
}

export interface DiskProbe {
  free(path: string): Promise<FreeSpace>;
  size(path: string): Promise<number>;
}

const CATEGORY_OF_KIND: Record<keyof typeof DOWNLOAD_DIRECTORIES, DiskCategory> = {
  model: "models",
  embedding: "embeddings",
  runtime: "runtimes",
  mcp: "mcp",
  skill: "skills",
};

export const defaultProbe: DiskProbe = {
  async free(path) {
    const stats = await statfs(path);
    const block = Number(stats.bsize);
    return {
      totalBytes: Number(stats.blocks) * block,
      freeBytes: Number(stats.bavail) * block,
    };
  },
  size: (path) => directorySize(path),
};

/** Recursive byte total, ignoring anything that vanishes or refuses to be read mid-walk. */
export async function directorySize(path: string): Promise<number> {
  let entries;
  try {
    entries = await readdir(path, { withFileTypes: true });
  } catch {
    return 0;
  }
  let total = 0;
  for (const entry of entries) {
    const child = join(path, entry.name);
    if (entry.isDirectory()) {
      total += await directorySize(child);
      continue;
    }
    if (!entry.isFile()) continue;
    try {
      total += (await stat(child)).size;
    } catch {
      // A partial file being renamed into place disappears between readdir and stat.
    }
  }
  return total;
}

export interface DiskServiceOptions {
  downloadsDir: string;
  vectorStoresDir: string;
  userDataDir: string;
  probe?: DiskProbe;
  /** How long a measurement stays usable. Walking a models directory is not free. */
  ttlMs?: number;
  clock?: () => number;
  logger?: Logger;
}

const DEFAULT_TTL_MS = 30_000;

/**
 * Per-category usage for the breakdown bar plus the free space the precondition checks.
 * Measured on demand and cached briefly rather than recomputed on every render.
 */
export class DiskService {
  private cached: DiskUsageDto | undefined;
  private pending: Promise<DiskUsageDto> | undefined;

  constructor(private readonly options: DiskServiceOptions) {}

  categoryOf(kind: keyof typeof DOWNLOAD_DIRECTORIES): DiskCategory {
    return CATEGORY_OF_KIND[kind];
  }

  async usage(refresh = false): Promise<DiskUsageDto> {
    if (!refresh && this.cached && this.now() - this.cached.measuredAt < this.ttlMs)
      return this.cached;
    this.pending ??= this.measure().finally(() => {
      this.pending = undefined;
    });
    return this.pending;
  }

  async freeBytes(): Promise<number> {
    const { freeBytes } = await this.probe.free(this.options.downloadsDir);
    return freeBytes;
  }

  private async measure(): Promise<DiskUsageDto> {
    const { downloadsDir, vectorStoresDir, userDataDir } = this.options;
    const space = await this.probe
      .free(downloadsDir)
      .catch(() => this.probe.free(userDataDir))
      .catch(() => ({ totalBytes: 0, freeBytes: 0 }));

    const categories: { category: DiskCategory; bytes: number }[] = [];
    let accounted = 0;
    for (const [kind, folder] of Object.entries(DOWNLOAD_DIRECTORIES)) {
      const bytes = await this.probe.size(join(downloadsDir, folder));
      accounted += bytes;
      categories.push({
        category: CATEGORY_OF_KIND[kind as keyof typeof DOWNLOAD_DIRECTORIES],
        bytes,
      });
    }
    const vectors = await this.probe.size(vectorStoresDir);
    accounted += vectors;
    categories.push({ category: "vectors", bytes: vectors });
    const everything = await this.probe.size(userDataDir);
    categories.push({ category: "other", bytes: Math.max(0, everything - accounted) });

    const usage: DiskUsageDto = {
      root: downloadsDir,
      totalBytes: space.totalBytes,
      freeBytes: space.freeBytes,
      usedBytes: Math.max(0, space.totalBytes - space.freeBytes),
      categories,
      measuredAt: this.now() as Timestamp,
    };
    this.cached = usage;
    return usage;
  }

  private get probe(): DiskProbe {
    return this.options.probe ?? defaultProbe;
  }
  private get ttlMs(): number {
    return this.options.ttlMs ?? DEFAULT_TTL_MS;
  }
  private now(): number {
    return (this.options.clock ?? Date.now)();
  }
}
