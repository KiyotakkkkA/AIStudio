import type { ModelCapability, ThinkingEffort } from "@zvs/shared";

export const ACCOUNT_SETUP_UNAVAILABLE = "Не реализовано";
export const EFFORT_LABEL = "Режим рассуждения";
export const EFFORT_UNSUPPORTED =
  "Модель не поддерживает рассуждение: ответ формируется в быстром режиме.";
export const WEB_SEARCH_LABEL = "Поиск в сети";

export const EFFORT_STEPS: readonly ThinkingEffort[] = ["fast", "auto", "thinking"];
export const FIXED_EFFORT_STEPS: readonly ThinkingEffort[] = ["fast", "thinking"];

export const EFFORT_LABELS: Record<ThinkingEffort, string> = {
  fast: "Быстрый",
  auto: "Авто",
  thinking: "Мышление",
};

export const EFFORT_HINTS: Record<ThinkingEffort, string> = {
  fast: "Ответ без этапа рассуждения.",
  auto: "Модель решает, требуется ли рассуждение.",
  thinking: "Рассуждение перед каждым ответом.",
};

export function effortSteps(
  capabilities: readonly ModelCapability[] | undefined,
): readonly ThinkingEffort[] {
  if (capabilities === undefined || capabilities.includes("adaptive-reasoning"))
    return EFFORT_STEPS;
  return FIXED_EFFORT_STEPS;
}

export function shownEffort(
  effort: ThinkingEffort,
  steps: readonly ThinkingEffort[],
): ThinkingEffort {
  return steps.includes(effort) ? effort : "thinking";
}

export function effortStep(
  effort: ThinkingEffort,
  steps: readonly ThinkingEffort[] = EFFORT_STEPS,
): number {
  return Math.max(0, steps.indexOf(effort));
}

export function stepEffort(
  step: number,
  steps: readonly ThinkingEffort[] = EFFORT_STEPS,
): ThinkingEffort {
  const index = Math.min(steps.length - 1, Math.max(0, Math.round(step)));
  return steps[index] ?? "fast";
}
