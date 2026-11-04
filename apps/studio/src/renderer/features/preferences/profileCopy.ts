import { PROFILE_KEYS, type ReplyStyle } from "./ProfileStore";

export const STYLE_ROW = {
  key: PROFILE_KEYS.style,
  label: "Стиль ответов",
  hint: "Форма и подробность ответов.",
} as const;

export const STYLE_OPTIONS: readonly { value: ReplyStyle; label: string }[] = [
  { value: "standard", label: "Стандартный" },
  { value: "concise", label: "Краткий" },
  { value: "detailed", label: "Подробный" },
  { value: "formal", label: "Деловой" },
  { value: "custom", label: "Пользовательский" },
];

export const STYLE_DESCRIPTIONS: Record<ReplyStyle, string> = {
  standard: "Ответы без дополнительных указаний по стилю.",
  concise: "Краткие ответы без вступлений. Подробности — по запросу.",
  detailed: "Развёрнутые ответы с пояснениями и примерами.",
  formal: "Нейтральный деловой тон без разговорных выражений и эмодзи.",
  custom: "Стиль, описанный ниже.",
};

export const CUSTOM_STYLE_LABEL = "Описание стиля";
export const CUSTOM_STYLE_PLACEHOLDER = "Требования к стилю ответов";
export const CUSTOM_STYLE_EMPTY = "Без описания используется стандартный стиль.";

export const ABOUT_ROW = {
  key: PROFILE_KEYS.about,
  label: "Дополнительная информация",
  hint: "Сведения о пользователе для новых диалогов.",
} as const;

export const ABOUT_PLACEHOLDER = "Сведения для учёта в ответах";
export const ABOUT_NEW_DIALOGS = "Изменения применяются в новых диалогах";

export const TOOL_CALLS_ROW = {
  key: PROFILE_KEYS.toolCalls,
  label: "Лимит вызовов инструментов на ответ",
  hint: `Ограничение на один ответ модели.`,
} as const;

export type ToolCallsMode = "limited" | "unlimited";

export const TOOL_CALLS_MODE_OPTIONS: readonly { value: ToolCallsMode; label: string }[] = [
  { value: "limited", label: "Ограниченно" },
  { value: "unlimited", label: "Без ограничений" },
];

export const TOOL_CALLS_COUNT_LABEL = "Число вызовов";

export const SAVE = "Сохранить";
export const CANCEL = "Отмена";
