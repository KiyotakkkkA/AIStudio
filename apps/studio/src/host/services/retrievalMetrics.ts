import type { VectorEvalMetricsDto } from "@zvs/shared";

export interface RankedCase {
  readonly ranked: readonly string[];
  readonly expected: readonly string[];
}

export interface CaseScore {
  readonly rank: number | null;
  readonly found: number;
  readonly expected: number;
}

export function rankedDocuments(hits: readonly { readonly documentId: string }[]): string[] {
  const seen = new Set<string>();
  for (const hit of hits) seen.add(hit.documentId);
  return [...seen];
}

export function scoreCase({ ranked, expected }: RankedCase, k: number): CaseScore {
  const wanted = new Set(expected);
  const top = ranked.slice(0, k);
  const position = top.findIndex((documentId) => wanted.has(documentId));
  return {
    rank: position === -1 ? null : position + 1,
    found: top.filter((documentId) => wanted.has(documentId)).length,
    expected: wanted.size,
  };
}

export function retrievalMetrics(scores: readonly CaseScore[]): VectorEvalMetricsDto {
  const cases = scores.length;
  if (cases === 0) return { cases: 0, hitAt1: 0, hitAt5: 0, recallAtK: 0, mrr: 0 };
  let hitAt1 = 0;
  let hitAt5 = 0;
  let recall = 0;
  let reciprocal = 0;
  for (const score of scores) {
    if (score.rank !== null) {
      if (score.rank <= 1) hitAt1 += 1;
      if (score.rank <= 5) hitAt5 += 1;
      reciprocal += 1 / score.rank;
    }
    recall += score.expected === 0 ? 0 : Math.min(1, score.found / score.expected);
  }
  return {
    cases,
    hitAt1: hitAt1 / cases,
    hitAt5: hitAt5 / cases,
    recallAtK: recall / cases,
    mrr: reciprocal / cases,
  };
}
