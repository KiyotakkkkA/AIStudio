import { createRequire } from "node:module";
import { AppError, AppErrorCode } from "@zvs/shared";
import type { StudioPaths } from "../../platform/paths.ts";
import type { ChunkConfig, SectionChunk, TextChunk } from "./ports.ts";

export interface NativeAddon {
  vectorCall(request: string): Promise<string>;
  vectorBeginUpsert(): Promise<string>;
  vectorCancel(operationId: string): Promise<void>;
  vectorRelease(operationId: string): Promise<void>;
  chunkText(text: string, config: ChunkConfig): Promise<TextChunk[]>;
  chunkSegments(
    segments: { text: string; section: string[]; page?: number }[],
    config: ChunkConfig,
  ): Promise<(Omit<SectionChunk, "page"> & { page?: number | null })[]>;
  hashBytes(bytes: Buffer): Promise<string>;
  graphHydrate(graphId: string, content: string): Promise<string>;
  graphStats(graphId: string): Promise<string>;
  graphAddNode(graphId: string, node: string): Promise<boolean>;
  graphAddEdge(graphId: string, edge: string): Promise<boolean>;
  graphRemoveEdge(graphId: string, from: string, to: string, relation: string): Promise<boolean>;
  graphRemoveNode(graphId: string, id: string): Promise<number | null>;
  graphNeighbors(graphId: string, query: string): Promise<string>;
  graphShortestPath(graphId: string, query: string): Promise<string>;
  graphKHopSubgraph(graphId: string, query: string): Promise<string>;
}

const require = createRequire(import.meta.url);
const loaded = new Map<string, NativeAddon>();

export function loadAddon(paths: Pick<StudioPaths, "nativeAddonPath">): NativeAddon {
  const path = paths.nativeAddonPath;
  const cached = loaded.get(path);
  if (cached) return cached;
  try {
    const addon: unknown = require(path);
    if (
      typeof addon !== "object" ||
      addon === null ||
      !("chunkText" in addon) ||
      typeof addon.chunkText !== "function" ||
      !("hashBytes" in addon) ||
      typeof addon.hashBytes !== "function"
    )
      throw new Error("Native addon exports do not match the RustCore interface");
    for (const name of [
      "chunkSegments",
      "vectorCall",
      "vectorBeginUpsert",
      "vectorCancel",
      "vectorRelease",
      "graphHydrate",
      "graphStats",
      "graphAddNode",
      "graphAddEdge",
      "graphRemoveEdge",
      "graphRemoveNode",
      "graphNeighbors",
      "graphShortestPath",
      "graphKHopSubgraph",
    ]) {
      if (!(name in addon) || typeof (addon as Record<string, unknown>)[name] !== "function") {
        throw new Error(`Native addon is missing ${name}`);
      }
    }
    loaded.set(path, addon as NativeAddon);
    return addon as NativeAddon;
  } catch (cause) {
    throw new AppError(
      AppErrorCode.NATIVE_ERROR,
      `Не удалось загрузить Rust addon: ${path}. Выполните pnpm build:rust из корня репозитория.`,
      { cause },
    );
  }
}
