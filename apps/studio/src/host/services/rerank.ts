import type { VectorSearchHitDto } from "@zvs/shared";

export interface RerankPort {
  rerank(query: string, passages: readonly string[], signal: AbortSignal): Promise<number[]>;
}

export type RerankerSource = (modelRef: string) => RerankPort;

export interface LocalRerankEngine {
  rerank(
    modelRef: string,
    query: string,
    passages: readonly string[],
    signal: AbortSignal,
  ): Promise<number[]>;
}

export function localRerankers(engine: LocalRerankEngine): RerankerSource {
  return (modelRef) => ({
    rerank: (query, passages, signal) => engine.rerank(modelRef, query, passages, signal),
  });
}

export const MAX_RERANK_QUERY_CHARS = 1_000;
export const MAX_RERANK_PASSAGE_CHARS = 2_000;

export function normaliseRerankScore(raw: number): number {
  if (raw >= 0) return 1 / (1 + Math.exp(-raw));
  const exp = Math.exp(raw);
  return exp / (1 + exp);
}

export function passageOf(hit: VectorSearchHitDto): string {
  const payload = hit.payload;
  const text =
    typeof payload === "string"
      ? payload
      : payload !== null &&
          typeof payload === "object" &&
          !Array.isArray(payload) &&
          typeof payload.text === "string"
        ? payload.text
        : hit.path;
  return text.slice(0, MAX_RERANK_PASSAGE_CHARS);
}

export function applyRerank(
  hits: readonly VectorSearchHitDto[],
  rawScores: readonly number[],
): VectorSearchHitDto[] {
  return hits
    .map((hit, index) => {
      const rerankScore = normaliseRerankScore(rawScores[index] ?? Number.NEGATIVE_INFINITY);
      return { ...hit, vectorScore: hit.score, rerankScore, score: rerankScore };
    })
    .sort((left, right) => right.score - left.score || left.id.localeCompare(right.id));
}
