import type { BrowserNetworkEntry } from "@zvs/shared";
import { plural } from "./browserCopy";

export const DEVTOOLS_OPEN = "Инструменты разработчика";
export const DEVTOOLS_CLOSE = "Закрыть инструменты разработчика";
export const NETWORK_TAB = "Сеть";
export const RECORD_START = "Начать запись";
export const RECORD_STOP = "Остановить запись";
export const SAVE_LOG = "Сохранить запись в HAR";
export const SAVE_FAILED = "Не удалось сохранить запись.";
export const CLEAR_LOG = "Очистить журнал";
export const FILTER_LABEL = "Фильтр запросов";
export const FILTER_PLACEHOLDER = "Фильтр по адресу";
export const NETWORK_IDLE = "Запись остановлена.";
export const NETWORK_IDLE_HINT =
  "Запись сетевых запросов текущей вкладки начинается кнопкой «Начать запись».";
export const NETWORK_EMPTY = "Запросов нет.";
export const NETWORK_EMPTY_HINT = "Запросы появятся после действий на странице или её обновления.";
export const NETWORK_NOTHING_FOUND = "Ничего не найдено.";
export const NETWORK_FAILED = "Действие не выполнено.";
export const NETWORK_NO_TAB = "Нет активной вкладки.";
export const RECORDING = "Запись";
export const DETAIL_CLOSE = "Закрыть подробности";
export const REDACTED = "скрыто";
export const REDACTED_HINT = "Значения куки и учётных данных не покидают рабочий профиль.";
export const BODY_UNAVAILABLE = "Тело ответа недоступно.";
export const BODY_LOADING = "Загрузка…";
export const BODY_BINARY = "Двоичные данные";
export const BODY_TRUNCATED = "Показан первый 1 МБ ответа.";
export const NO_HEADERS = "Заголовков нет.";
export const STATE_PENDING = "Ожидание";
export const STATE_FAILED = "Ошибка";
export const FROM_CACHE = "кэш";

export const COLUMNS = {
  name: "Имя",
  status: "Статус",
  method: "Метод",
  type: "Тип",
  size: "Размер",
  time: "Время",
} as const;

export const DETAIL_TABS = {
  headers: "Заголовки",
  payload: "Полезная нагрузка",
  response: "Ответ",
} as const;
export type DetailTab = keyof typeof DETAIL_TABS;

export const SECTIONS = {
  general: "Общие",
  response: "Заголовки ответа",
  request: "Заголовки запроса",
} as const;

export const GENERAL_LABELS = {
  url: "Адрес запроса",
  method: "Метод",
  status: "Код состояния",
  type: "Тип",
  started: "Начало",
  error: "Ошибка",
} as const;

export const TYPE_FILTERS = [
  { id: "all", label: "Все" },
  { id: "fetch", label: "Fetch/XHR" },
  { id: "document", label: "Документ" },
  { id: "script", label: "JS" },
  { id: "style", label: "CSS" },
  { id: "image", label: "Изображения" },
  { id: "font", label: "Шрифты" },
  { id: "media", label: "Медиа" },
  { id: "other", label: "Другое" },
] as const;
export type TypeFilter = (typeof TYPE_FILTERS)[number]["id"];

const TYPE_GROUPS: Record<string, TypeFilter> = {
  XHR: "fetch",
  Fetch: "fetch",
  EventSource: "fetch",
  Document: "document",
  Script: "script",
  Stylesheet: "style",
  Image: "image",
  Font: "font",
  Media: "media",
};

export function typeGroup(resourceType: string): TypeFilter {
  return TYPE_GROUPS[resourceType] ?? "other";
}

export function matches(entry: BrowserNetworkEntry, filter: TypeFilter, query: string): boolean {
  if (filter !== "all" && typeGroup(entry.resourceType) !== filter) return false;
  const needle = query.trim().toLowerCase();
  return needle === "" || entry.url.toLowerCase().includes(needle);
}

export function entryName(url: string): { name: string; host: string } {
  try {
    const parsed = new URL(url);
    const segments = parsed.pathname.split("/").filter(Boolean);
    const last = segments.at(-1);
    return {
      name: last ? `${decodeURIComponent(last)}${parsed.search}` : parsed.host,
      host: parsed.host,
    };
  } catch {
    return { name: url, host: "" };
  }
}

function decimal(value: number, digits: number): string {
  return value.toLocaleString("ru-RU", { maximumFractionDigits: digits });
}

export function formatSize(bytes: number | undefined): string {
  if (bytes === undefined) return "—";
  if (bytes < 1024) return `${decimal(bytes, 0)} Б`;
  if (bytes < 1024 * 1024) return `${decimal(bytes / 1024, 1)} КБ`;
  return `${decimal(bytes / (1024 * 1024), 1)} МБ`;
}

export function formatDuration(ms: number | undefined): string {
  if (ms === undefined) return "—";
  if (ms < 1000) return `${decimal(Math.round(ms), 0)} мс`;
  return `${decimal(ms / 1000, 2)} с`;
}

export function formatClock(epochMs: number): string {
  return new Date(epochMs).toLocaleTimeString("ru-RU", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function requestsWord(count: number): string {
  return plural(count, "запрос", "запроса", "запросов");
}

export function transferred(entries: readonly BrowserNetworkEntry[]): number {
  return entries.reduce((total, entry) => total + (entry.size ?? 0), 0);
}

export function droppedLabel(count: number): string {
  return `Старые записи удалены: ${count.toLocaleString("ru-RU")}`;
}

export function pretty(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}

export function savedLabel(fileName: string | undefined): string {
  return fileName ? `Запись сохранена: ${fileName}` : "Запись сохранена.";
}
