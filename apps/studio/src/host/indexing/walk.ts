import { readdir, stat } from "node:fs/promises";
import { join, relative, resolve, basename } from "node:path";
import type { VectorSourceEntity } from "../data/schema/index.ts";
import { toExtendedPath } from "../platform/longPath.ts";
import { accepts, normalizeRelative } from "./patterns.ts";

export interface DiscoveredFile {
  readonly path: string;
  readonly bytes: number;
  readonly sourceId: string;
}

export interface WalkLimits {
  maxFiles?: number;
  maxFileBytes?: number;
}

const DEFAULT_MAX_FILES = 200_000;
const DEFAULT_MAX_FILE_BYTES = 8 * 1024 * 1024;

export interface WalkOutcome {
  readonly files: DiscoveredFile[];
  readonly notes: string[];
}

/** A walk swallows its errors to keep going, so the note is the only place the cause shows. */
function reason(error: unknown): string {
  const code = (error as { code?: unknown } | null)?.code;
  if (code === "ENOENT") return "путь не найден";
  if (code === "EPERM" || code === "EACCES") return "нет доступа";
  if (code === "ENAMETOOLONG") return "слишком длинный путь";
  return error instanceof Error ? error.message : String(error);
}

export async function walkSources(
  sources: readonly VectorSourceEntity[],
  limits: WalkLimits = {},
  signal?: AbortSignal,
): Promise<WalkOutcome> {
  const maxFiles = limits.maxFiles ?? DEFAULT_MAX_FILES;
  const maxFileBytes = limits.maxFileBytes ?? DEFAULT_MAX_FILE_BYTES;
  const found = new Map<string, DiscoveredFile>();
  const notes: string[] = [];
  const note = (message: string) => {
    if (notes.length < 200) notes.push(message);
  };

  for (const source of sources) {
    signal?.throwIfAborted();
    const root = resolve(source.path);
    let entry;
    try {
      entry = await stat(toExtendedPath(root));
    } catch (error: unknown) {
      note(`Источник недоступен: ${source.path} — ${reason(error)}`);
      continue;
    }
    if (entry.isFile()) {
      if (!accepts(basename(root), source.include, source.exclude)) continue;
      if (entry.size > maxFileBytes) {
        note(`Файл слишком велик: ${root}`);
        continue;
      }
      found.set(root, { path: root, bytes: entry.size, sourceId: source.id });
      continue;
    }
    if (!entry.isDirectory()) {
      note(`Источник не является файлом или папкой: ${source.path}`);
      continue;
    }
    const queue = [root];
    while (queue.length > 0) {
      signal?.throwIfAborted();
      const directory = queue.pop()!;
      let entries;
      try {
        entries = await readdir(toExtendedPath(directory), { withFileTypes: true });
      } catch (error: unknown) {
        note(`Папка недоступна: ${directory} — ${reason(error)}`);
        continue;
      }
      for (const child of entries) {
        const absolute = join(directory, child.name);
        const relativePath = normalizeRelative(relative(root, absolute));
        if (child.isDirectory()) {
          if (!source.recursive) continue;
          if (accepts(relativePath, [], source.exclude)) queue.push(absolute);
          continue;
        }
        if (!child.isFile()) continue;
        if (!accepts(relativePath, source.include, source.exclude)) continue;
        if (found.has(absolute)) continue;
        let info;
        try {
          info = await stat(toExtendedPath(absolute));
        } catch (error: unknown) {
          note(`Файл недоступен: ${absolute} — ${reason(error)}`);
          continue;
        }
        if (info.size > maxFileBytes) {
          note(`Файл слишком велик: ${absolute}`);
          continue;
        }
        found.set(absolute, { path: absolute, bytes: info.size, sourceId: source.id });
        if (found.size >= maxFiles) {
          note(`Достигнут предел в ${String(maxFiles)} файлов`);
          queue.length = 0;
          break;
        }
      }
    }
  }
  return { files: [...found.values()].sort((a, b) => a.path.localeCompare(b.path)), notes };
}
