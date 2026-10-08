import {
  AddVectorEvalCaseInput,
  AppError,
  AppErrorCode,
  VectorEvalCaseDto,
  VectorEvalRunDto,
  VectorEvalRunInput,
  VectorEvalSetDto,
  type VectorEvalCaseRef,
  type VectorEvalCaseResultDto,
  type VectorEvalMode,
  type VectorEvalModeResultDto,
} from "@zvs/shared";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import type { VectorEvalCaseEntity } from "../data/schema/index.ts";
import type { Logger } from "../platform/logger.ts";
import { createId } from "../platform/ids.ts";
import { rankedDocuments, retrievalMetrics, scoreCase } from "./retrievalMetrics.ts";
import type { VectorStoreService } from "./VectorStoreService.ts";

export interface VectorEvalServiceOptions {
  data: UnitOfWork;
  stores: Pick<VectorStoreService, "requireStore" | "searchTimed">;
  logger?: Logger;
  clock?: () => number;
}

export class VectorEvalService {
  private readonly running = new Set<string>();
  constructor(private readonly options: VectorEvalServiceOptions) {}

  list(storeId: string): VectorEvalSetDto {
    const store = this.options.stores.requireStore(storeId);
    return VectorEvalSetDto.parse({
      cases: this.cases.listByStore(storeId).map(toDto),
      lastRun: store.lastEval ?? null,
    });
  }

  add(raw: AddVectorEvalCaseInput): VectorEvalCaseDto {
    const input = AddVectorEvalCaseInput.parse(raw);
    this.options.stores.requireStore(input.storeId);
    const known = new Set(
      this.options.data.repositories.vectorDocuments
        .listByStore(input.storeId)
        .map((document) => document.id),
    );
    const expected = [...new Set(input.expectedDocumentIds)];
    const missing = expected.filter((id) => !known.has(id));
    if (missing.length > 0)
      throw new AppError(
        AppErrorCode.VALIDATION_FAILED,
        "Ожидаемый документ не найден в хранилище",
        {
          details: { missing },
        },
      );
    return toDto(
      this.cases.add({
        id: createId(),
        storeId: input.storeId,
        question: input.question,
        expectedDocumentIds: expected,
        createdAt: this.now(),
      }),
    );
  }

  remove({ storeId, id }: VectorEvalCaseRef): void {
    if (this.cases.remove(storeId, id) === 0)
      throw new AppError(AppErrorCode.NOT_FOUND, "Вопрос проверки не найден");
  }

  async run(raw: VectorEvalRunInput, signal?: AbortSignal): Promise<VectorEvalRunDto> {
    const input = VectorEvalRunInput.parse(raw);
    const store = this.options.stores.requireStore(input.storeId);
    const cases = this.cases.listByStore(input.storeId);
    if (cases.length === 0)
      throw new AppError(
        AppErrorCode.VALIDATION_FAILED,
        "Добавьте хотя бы один вопрос для проверки",
      );
    if (this.running.has(input.storeId))
      throw new AppError(AppErrorCode.CONFLICT, "Проверка уже идёт");
    this.running.add(input.storeId);
    const started = performance.now();
    try {
      const modes: VectorEvalMode[] = store.rerank.enabled ? ["vector", "rerank"] : ["vector"];
      const results: VectorEvalModeResultDto[] = [];
      for (const mode of modes)
        results.push(await this.runMode(input.storeId, mode, cases, input.k, signal));
      const run = VectorEvalRunDto.parse({
        storeId: input.storeId,
        k: input.k,
        ranAt: this.now(),
        durationMs: performance.now() - started,
        modes: results,
      });
      this.options.data.repositories.vectorStores.update(input.storeId, { lastEval: run });
      this.options.logger?.log("info", "vectorStores", "Ran a retrieval evaluation", {
        storeId: input.storeId,
        cases: cases.length,
        modes: results.map((result) => ({ mode: result.mode, mrr: result.metrics.mrr })),
      });
      return run;
    } finally {
      this.running.delete(input.storeId);
    }
  }

  private async runMode(
    storeId: string,
    mode: VectorEvalMode,
    cases: readonly VectorEvalCaseEntity[],
    k: number,
    signal: AbortSignal | undefined,
  ): Promise<VectorEvalModeResultDto> {
    const results: VectorEvalCaseResultDto[] = [];
    let fallbacks = 0;
    for (const entry of cases) {
      signal?.throwIfAborted();
      try {
        const result = await this.options.stores.searchTimed(
          storeId,
          entry.question,
          { k, minScore: 0, rerank: mode === "rerank" },
          signal,
        );
        if (result.rerank === "fallback") fallbacks += 1;
        const score = scoreCase(
          { ranked: rankedDocuments(result.hits), expected: entry.expectedDocumentIds },
          k,
        );
        results.push({ caseId: entry.id, ...score });
      } catch (error) {
        if (signal?.aborted === true) throw error;
        results.push({
          caseId: entry.id,
          rank: null,
          found: 0,
          expected: new Set(entry.expectedDocumentIds).size,
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }
    return { mode, metrics: retrievalMetrics(results), cases: results, fallbacks };
  }

  private get cases() {
    return this.options.data.repositories.vectorEvalCases;
  }
  private now() {
    return (this.options.clock ?? Date.now)();
  }
}

function toDto(row: VectorEvalCaseEntity): VectorEvalCaseDto {
  return VectorEvalCaseDto.parse(row);
}
