import type {
  ChatCarriedKind,
  ChatCarriedStatus,
  ChatHistoryStatus,
  ChatPromptSection,
} from "@zvs/shared";
import { plural } from "../../connections/connectionPresentation";

export const STATS_TOGGLE = "Статистика";
export const STATS_EXPORT_GROUP = "Статистика и экспорт";
export const STATS_TITLE = "Статистика контекста";
export const STATS_LOADING = "Расчёт статистики…";
export const STATS_FAILED = "Не удалось рассчитать статистику.";
export const STATS_NO_MODEL = "Статистика доступна после выбора модели.";
export const STATS_RETRY = "Повторить";

export const WINDOW_HEADING = "Контекстное окно";
export const WINDOW_USED = "Занято";
export const WINDOW_LIMIT = "Окно модели";
export const WINDOW_RESERVED = "Резерв под ответ";
export const WINDOW_FREE = "Свободно";
export const SOURCE_TOKENIZER = "Приблизительное количество";

export const SECTIONS_HEADING = "Состав системного промпта";
export const SECTION_EMPTY = "не используется";
export const SECTION_COPY = "Копировать";
export const SKILLS_APPROXIMATE =
  "Каталог навыков подбирается по тексту сообщения, поэтому при отправке он может отличаться.";

export const TOOLS_HEADING = "Инструменты";
export const TOOLS_UNUSED = "не вызывался";
export const TOOLS_DEFERRED =
  "Отложенные инструменты не занимают окно и загружаются по запросу модели";

export const HISTORY_HEADING = "История в следующем запросе";
export const HISTORY_EMPTY = "В диалоге пока нет сообщений.";
export const HISTORY_SOURCES = "Фрагменты из хранилищ добавляются при отправке.";
export const HISTORY_CARRIED = "Передаётся вместе с ответом";
export const CARRIED_TRUNCATED = "обрезано до лимита";
export const REASONING_LABEL = "Рассуждения";

export const CARRIED_STATUS_LABELS: Readonly<Record<ChatCarriedStatus, string>> = {
  sent: "Передаётся",
  shortened: "Сокращено",
  dropped: "Не передаётся",
};

export const CARRIED_KIND_LABELS: Readonly<Record<ChatCarriedKind, string>> = {
  toolCall: "Инструмент",
  source: "Источник",
};

export const TURNS_HEADING = "Расход по ответам";
export const TURNS_EMPTY = "Ответов пока нет.";
export const TURNS_TABLE = "Таблица";
export const TURNS_CHART = "График";
export const SERIES_INPUT = "Вход";
export const SERIES_CACHED = "Вход из кэша";
export const SERIES_OUTPUT = "Выход";
export const TOTAL_INPUT = "Отправлено на вход (с повторами)";
export const TOTAL_OUTPUT = "Получено на выходе";
export const TOTAL_CACHED = "Из кэша";
export const TOTAL_REASONING = "Рассуждения";
export const TURN_ESTIMATED = "оценка";
export const TURN_REPORTED = "провайдер";

export const HINTS_HEADING = "Рекомендации";
export const HINTS_EMPTY = "Явных резервов для сокращения контекста нет.";

export const SECTION_LABELS: Readonly<Record<ChatPromptSection, string>> = {
  dialogPrompt: "Промпт диалога",
  profile: "Профиль",
  memory: "Память",
  knowledge: "Хранилища",
  mcpGuidance: "Инструкции MCP-серверов",
  command: "Команда",
  skills: "Каталог навыков",
  workspace: "Рабочая папка",
  fileTools: "Работа с файлами",
  processTools: "Процессы",
  endedShells: "Завершённые процессы",
  ask: "Вопросы пользователю",
  delegate: "Субагенты",
  deferred: "Отложенные инструменты",
  goal: "Цель",
  toolSpecs: "Описания инструментов",
};

export const STATUS_LABELS: Readonly<Record<ChatHistoryStatus, string>> = {
  included: "в запросе",
  trimmed: "не помещается",
  skipped: "пропущено: пустой ответ",
};

export const ROLE_LABELS = {
  user: "Пользователь",
  assistant: "Ассистент",
  event: "Событие",
} as const;

export function messagesWord(count: number): string {
  return plural(count, "сообщение", "сообщения", "сообщений");
}

export function toolsWord(count: number): string {
  return plural(count, "инструмент", "инструмента", "инструментов");
}

export function answersWord(count: number): string {
  return plural(count, "ответ", "ответа", "ответов");
}

export function callsWord(count: number): string {
  return plural(count, "вызов", "вызова", "вызовов");
}

export function roundsWord(count: number): string {
  return plural(count, "раунд", "раунда", "раундов");
}

export function tokensWord(count: number): string {
  return plural(count, "токен", "токена", "токенов").replace(
    String(count),
    count.toLocaleString("ru-RU"),
  );
}
