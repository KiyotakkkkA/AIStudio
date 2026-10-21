import type { RetrievalMode, ToolMode } from "@zvs/shared";

export const RETRIEVAL_MODES: readonly { value: RetrievalMode; label: string }[] = [
  { value: "auto", label: "Перед ответом" },
  { value: "hybrid", label: "Гибрид" },
  { value: "agentic", label: "Модель сама" },
];

const HINTS: Record<RetrievalMode, string> = {
  auto: "Перед каждым ответом будут найдены релевантные фрагменты документов.",
  hybrid:
    "Перед ответом подбираются несколько фрагментов, а модель может искать дальше сама, если их мало.",
  agentic:
    "Модель сама решает, когда искать, формулирует запросы и дочитывает разделы. На приветствие поиск не запускается.",
};

export function retrievalHint(mode: RetrievalMode, toolMode: ToolMode): string {
  if (mode !== "auto" && toolMode === "none")
    return "Эта модель не вызывает инструменты, поэтому поиск выполняется перед ответом.";
  return HINTS[mode];
}
