import { AppError, AppErrorCode } from "@zvs/shared";
import { z } from "zod";
import {
  VectorHitSchema,
  VectorStatsSchema,
  type VectorCorePort,
  type VectorMetric,
  type VectorRow,
} from "./vectorTypes.ts";
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

export class RustCore implements RustCorePort, VectorCorePort {
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

  private async vectorCall<T>(request: object, schema: z.ZodType<T>): Promise<T> {
    try {
      return schema.parse(JSON.parse(await this.addon().vectorCall(JSON.stringify(request))));
    } catch (cause) {
      throw nativeError(cause);
    }
  }

  async createVectorIndex(path: string, dimension: number, metric: VectorMetric = "cosine") {
    return this.vectorCall({ operation: "create", path, dimension, metric }, VectorStatsSchema);
  }

  async openVectorIndex(path: string) {
    return this.vectorCall({ operation: "open", path }, VectorStatsSchema);
  }

  async upsertVectors(path: string, rows: VectorRow[], signal?: AbortSignal): Promise<number> {
    if (signal?.aborted) throw new AppError(AppErrorCode.RUN_CANCELLED, "Операция отменена");
    let addon: NativeAddon;
    let operationId: string;
    try {
      addon = this.addon();
      operationId = await addon.vectorBeginUpsert();
    } catch (cause) {
      throw nativeError(cause);
    }
    let cancellation: Promise<void> | undefined;
    const cancel = () => {
      cancellation ??= addon.vectorCancel(operationId);
      void cancellation.catch(() => undefined);
    };
    signal?.addEventListener("abort", cancel, { once: true });
    try {
      if (signal?.aborted) {
        cancel();
        await cancellation;
        throw new AppError(AppErrorCode.RUN_CANCELLED, "Операция отменена");
      }
      const request = JSON.stringify({ operation: "upsert", path, rows, operationId });
      return z
        .number()
        .int()
        .nonnegative()
        .parse(JSON.parse(await addon.vectorCall(request)));
    } catch (cause) {
      throw nativeError(cause);
    } finally {
      signal?.removeEventListener("abort", cancel);
      await cancellation?.catch(() => undefined);
      await addon.vectorRelease(operationId).catch(() => undefined);
    }
  }

  async searchVectors(
    path: string,
    vector: number[],
    k: number,
    minScore: number,
    filter?: string,
  ) {
    return this.vectorCall(
      { operation: "search", path, vector, k, minScore, filter },
      VectorHitSchema.array(),
    );
  }

  async deleteVectorsByIds(path: string, ids: string[]): Promise<void> {
    await this.vectorCall({ operation: "deleteByIds", path, ids }, z.null());
  }

  async deleteVectorsBySource(path: string, documentId: string): Promise<void> {
    await this.vectorCall({ operation: "deleteBySource", path, documentId }, z.null());
  }

  async vectorStats(path: string) {
    return this.vectorCall({ operation: "stats", path }, VectorStatsSchema);
  }

  async removeVectorIndex(path: string): Promise<void> {
    await this.vectorCall({ operation: "remove", path }, z.null());
  }
}
