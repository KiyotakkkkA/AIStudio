import { AppError, AppErrorCode } from "@zvs/shared";
import { loadAddon, type NativeAddon } from "./addon.ts";
import type { StudioPaths } from "../../platform/paths.ts";
import type { ChunkConfig, RustCorePort, TextChunk } from "./ports.ts";

const codes = new Set<string>(Object.values(AppErrorCode));

function nativeError(cause: unknown): AppError {
  if (cause instanceof AppError) return cause;
  let payload: unknown = cause;
  if (cause instanceof Error) {
    try {
      payload = JSON.parse(cause.message);
    } catch {
      payload = cause;
    }
  }
  if (
    typeof payload === "object" &&
    payload !== null &&
    "code" in payload &&
    typeof payload.code === "string" &&
    codes.has(payload.code)
  ) {
    return new AppError(
      payload.code as AppErrorCode,
      "message" in payload && typeof payload.message === "string"
        ? payload.message
        : "Ошибка Rust addon",
      { cause },
    );
  }
  return new AppError(AppErrorCode.NATIVE_ERROR, "Ошибка Rust addon", { cause });
}

export class RustCore implements RustCorePort {
  constructor(private readonly addon: () => NativeAddon) {}

  static fromPaths(paths: Pick<StudioPaths, "nativeAddonPath">): RustCore {
    return new RustCore(() => loadAddon(paths));
  }

  async chunk(
    text: string,
    config: ChunkConfig = { size: 256, overlap: 32 },
  ): Promise<TextChunk[]> {
    if (
      !Number.isInteger(config.size) ||
      config.size <= 0 ||
      config.size > 0xffffffff ||
      !Number.isInteger(config.overlap) ||
      config.overlap < 0 ||
      config.overlap >= config.size
    ) {
      throw new AppError(
        AppErrorCode.VALIDATION_FAILED,
        "Некорректный размер или перекрытие фрагментов",
      );
    }
    try {
      return await this.addon().chunkText(text, config);
    } catch (cause) {
      throw nativeError(cause);
    }
  }

  async hash(bytes: Uint8Array): Promise<string> {
    try {
      return await this.addon().hashBytes(Buffer.from(bytes));
    } catch (cause) {
      throw nativeError(cause);
    }
  }
}
