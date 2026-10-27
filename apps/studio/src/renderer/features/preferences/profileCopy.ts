import {
  PROFILE_KEYS,
  TOOL_CALLS_DEFAULT,
  TOOL_CALLS_MAX,
  TOOL_CALLS_MIN,
  type ReplyStyle,
} from "./ProfileStore";

export const STYLE_ROW = {
  key: PROFILE_KEYS.style,
  label: "Стиль общения",
  hint: "Как модель формулирует ответы.",
} as const;

export const STYLE_OPTIONS: readonly { value: ReplyStyle; label: string }[] = [
  { value: "standard", label: "Стандартный" },
  { value: "concise", label: "Краткий" },
  { value: "detailed", label: "Подробный" },
  { value: "formal", label: "Деловой" },
  { value: "custom", label: "Свой" },
];

export const STYLE_DESCRIPTIONS: Record<ReplyStyle, string> = {
  standard: "Ответы без дополнительных указаний по стилю.",
  concise: "Сначала ответ, без вступлений и повторения вопроса. Подробности — по запросу.",
  detailed: "Развёрнутые ответы с объяснением, важными частными случаями и примерами.",
  formal: "Нейтральный деловой тон без разговорных выражений и эмодзи.",
  custom: "Стиль, описанный ниже.",
};

export const CUSTOM_STYLE_LABEL = "Описание стиля";
export const CUSTOM_STYLE_PLACEHOLDER =
  "Например: отвечать по пунктам, без вступлений, примеры кода — на TypeScript";
export const CUSTOM_STYLE_EMPTY = "Без описания используется стандартный стиль.";

export const ABOUT_ROW = {
  key: PROFILE_KEYS.about,
  label: "Дополнительная информация",
  hint: "Сведения о вас, которые модель получает в начале каждого диалога.",
} as const;

export const ABOUT_PLACEHOLDER =
  "Например: backend-разработчик, работаю с Rust и PostgreSQL, использую Linux";
export const ABOUT_NEW_DIALOGS = "Сведения о вас применяются в новых диалогах";

export const TOOL_CALLS_ROW = {
  key: PROFILE_KEYS.toolCalls,
  label: "Максимальное количество вызовов инструментов за генерацию",
  hint: `Ограничение на один ответ модели: от ${String(TOOL_CALLS_MIN)} до ${String(TOOL_CALLS_MAX)}, по умолчанию ${String(TOOL_CALLS_DEFAULT)}.`,
} as const;

export const SAVE = "Сохранить";
export const CANCEL = "Отмена";
