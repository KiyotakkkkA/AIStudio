import { plural } from "../connections/connectionPresentation";

export const SUBAGENTS_ROW = {
  key: "subagents.preferences",
  label: "Субагенты",
  hint: "Модель может поручать отдельные задачи субагентам. Субагент работает в своём контексте с инструментами диалога и возвращает только отчёт.",
} as const;

export const SUBAGENTS_ENABLED = "Использовать субагентов";
export const SUBAGENT_MODELS_TITLE = "Предпочтительные модели";
export const SUBAGENT_MODELS_HINT = "Если ни одна недоступна, используется модель диалога";
export const SUBAGENT_MODELS_EMPTY = "Список пуст, субагенты используют модель диалога.";
export const SUBAGENT_PROVIDER = "Провайдер";
export const SUBAGENT_MODEL = "Модель";
export const SUBAGENT_PROVIDER_LABEL = "Провайдер модели субагента";
export const SUBAGENT_MODEL_LABEL = "Модель субагента";
export const SUBAGENT_NONE = "Не выбрана";
export const SUBAGENT_ADD = "Добавить";
export const SUBAGENT_REMOVE = "Убрать модель";
export const SUBAGENT_UP = "Выше";
export const SUBAGENT_DOWN = "Ниже";
export const SUBAGENT_CONCURRENCY = "Одновременно";
export const SUBAGENT_CONCURRENCY_LABEL = "Число субагентов, выполняемых одновременно";
export const SUBAGENT_ACCOUNT_NOTE = "Модели аккаунтов выполняют субагентов по одному";
export const SUBAGENT_NO_PROVIDERS = "Нет включённых провайдеров текстовых моделей.";
export const SUBAGENT_SAVE_FAILED = "Не удалось сохранить настройки субагентов";
export const SUBAGENT_MISSING_MODEL = "Модель недоступна";

export const SUBAGENT_PRESET_LABELS = {
  explore: "Исследование",
  general: "Общая задача",
} as const;

export type SubagentPreset = keyof typeof SUBAGENT_PRESET_LABELS;

export const SUBAGENT_RUNNING = "Выполняется";
export const SUBAGENT_QUEUED = "В очереди";
export const SUBAGENT_PARTIAL = "Частичный результат";
export const SUBAGENT_STOPPED = "Остановлен";
export const SUBAGENT_FAILED = "Ошибка";
export const SUBAGENT_STOP = "Остановить";
export const SUBAGENT_STEPS_SHOW = "Показать вызовы";
export const SUBAGENT_STEPS_HIDE = "Скрыть вызовы";
export const SUBAGENT_REPORT = "Отчёт";
export const SUBAGENT_GROUP = "Субагенты";
export const SUBAGENT_APPROVAL_PREFIX = "Субагент";
export const SUBAGENT_TOOL_TITLE = "Субагент";

export function subagentApprovalTitle(label: string): string {
  return `${SUBAGENT_APPROVAL_PREFIX}: ${label}`;
}

function counted(count: number, one: string, few: string, many: string): string {
  const word = plural(count, one, few, many).slice(String(count).length + 1);
  return `${count.toLocaleString("ru-RU")} ${word}`;
}

export function subagentTools(count: number): string {
  return counted(count, "инструмент", "инструмента", "инструментов");
}

export function subagentTokens(count: number): string {
  return counted(count, "токен", "токена", "токенов");
}

export function subagentDuration(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000));
  if (seconds < 60) return `${seconds.toLocaleString("ru-RU")} с`;
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return rest === 0
    ? `${minutes.toLocaleString("ru-RU")} мин`
    : `${minutes.toLocaleString("ru-RU")} мин ${rest.toLocaleString("ru-RU")} с`;
}

export function subagentGroupTitle(count: number): string {
  return counted(count, "субагент", "субагента", "субагентов");
}
