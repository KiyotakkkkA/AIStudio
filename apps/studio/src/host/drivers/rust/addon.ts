import { createRequire } from "node:module";
import { AppError, AppErrorCode } from "@zvs/shared";
import type { StudioPaths } from "../../platform/paths.ts";
import type { ChunkConfig, TextChunk } from "./ports.ts";

export interface NativeAddon {
  chunkText(text: string, config: ChunkConfig): Promise<TextChunk[]>;
  hashBytes(bytes: Buffer): Promise<string>;
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
