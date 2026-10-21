import type { RetrievalMode, ToolMode } from "@zvs/shared";

export const RETRIEVAL_MODES: readonly { value: RetrievalMode; label: string }[] = [
  { value: "auto", label: "Перед ответом" },
  { value: "hybrid", label: "Гибрид" },
  { value: "agentic", label: "Решает модель" },
];

const HINTS: Record<RetrievalMode, string> = {
  auto: "Релевантные фрагменты документов подбираются перед каждым ответом.",
  hybrid:
    "Фрагменты подбираются перед ответом; при недостатке результатов модель выполняет дополнительный поиск.",
  agentic:
    "Модель определяет необходимость поиска, формулирует запросы и дочитывает разделы документов.",
};

export function retrievalHint(mode: RetrievalMode, toolMode: ToolMode): string {
  if (mode !== "auto" && toolMode === "none")
    return "Модель не вызывает инструменты: поиск выполняется перед ответом.";
  return HINTS[mode];
}
