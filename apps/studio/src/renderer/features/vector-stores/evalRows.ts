import type {
  VectorDocumentDto,
  VectorEvalCaseDto,
  VectorEvalMetricsDto,
  VectorEvalMode,
  VectorEvalRunDto,
} from "@zvs/shared";

export const EVAL_MODE_LABELS: Record<VectorEvalMode, string> = {
  vector: "Только векторы",
  rerank: "С переранжированием",
  hybrid: "Гибридный",
  "hybrid-rerank": "Гибридный + переранжирование",
};

export interface EvalMetricRow {
  readonly key: keyof Omit<VectorEvalMetricsDto, "cases">;
  readonly label: string;
  readonly values: readonly string[];
  readonly delta: string | null;
  readonly improved: boolean | null;
}

export interface EvalCaseRank {
  readonly mode: VectorEvalMode;
  readonly label: string;
  readonly missed: boolean;
  readonly error?: string;
}

export interface EvalCaseRow {
  readonly id: string;
  readonly question: string;
  readonly expected: readonly string[];
  readonly ranks: readonly EvalCaseRank[];
}

const METRICS: readonly { key: EvalMetricRow["key"]; label: string }[] = [
  { key: "hitAt1", label: "hit@1" },
  { key: "hitAt5", label: "hit@5" },
  { key: "recallAtK", label: "recall@k" },
  { key: "mrr", label: "MRR" },
];

export function evalMetricRows(run: VectorEvalRunDto | null): EvalMetricRow[] {
  if (run === null) return [];
  return METRICS.map(({ key, label }) => {
    const values = run.modes.map((mode) => mode.metrics[key]);
    const first = values[0];
    const last = values[values.length - 1];
    const delta =
      values.length > 1 && first !== undefined && last !== undefined ? last - first : null;
    return {
      key,
      label,
      values: values.map((value) => value.toFixed(3)),
      delta: delta === null ? null : `${delta >= 0 ? "+" : "−"}${Math.abs(delta).toFixed(3)}`,
      improved: delta === null || Math.abs(delta) < 0.0005 ? null : delta > 0,
    };
  });
}

export function evalCaseRows(
  cases: readonly VectorEvalCaseDto[],
  run: VectorEvalRunDto | null,
  documents: readonly VectorDocumentDto[],
): EvalCaseRow[] {
  const paths = new Map(documents.map((document) => [document.id as string, document.sourcePath]));
  return cases.map((entry) => ({
    id: entry.id,
    question: entry.question,
    expected: entry.expectedDocumentIds.map((id) => paths.get(id) ?? `удалён · ${id.slice(0, 8)}`),
    ranks: run === null ? [] : caseRanks(run, entry.id),
  }));
}

function caseRanks(run: VectorEvalRunDto, caseId: string): EvalCaseRank[] {
  return run.modes.flatMap((mode) => {
    const result = mode.cases.find((item) => item.caseId === caseId);
    if (result === undefined) return [];
    const label =
      result.error !== undefined
        ? "ошибка"
        : result.rank === null
          ? `нет в топ-${String(run.k)}`
          : `#${String(result.rank)}`;
    return [
      {
        mode: mode.mode,
        label,
        missed: result.rank === null,
        ...(result.error === undefined ? {} : { error: result.error }),
      },
    ];
  });
}
