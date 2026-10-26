import { AppErrorCode, type StorageCategory, type StoragePart } from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";

export const STORAGE_TITLE = "Занято на диске";
export const STORAGE_BREAKDOWN = "По категориям";
export const STORAGE_LOADING = "Подсчёт…";
export const STORAGE_UNAVAILABLE = "Не удалось подсчитать занятое место.";
export const STORAGE_REFRESH = "Пересчитать";
export const STORAGE_CLEAR_CACHE = "Очистить кэш";
export const STORAGE_CLEAR_CACHE_HINT =
  "Временные файлы скриптов, запусков кода и открытых документов.";
export const STORAGE_CLEAR_CONFIRM = "Очистить кэш?";
export const STORAGE_CLEAR_CONSEQUENCE = "Временные файлы будут удалены.";
export const STORAGE_CLEAR = "Очистить";
export const STORAGE_CLEAR_ID = "storage.clearCache";
export const STORAGE_CLEAR_FAILED = "Не удалось очистить кэш.";
export const STORAGE_CACHE_BUSY = "Кэш нельзя очистить, пока выполняются задачи.";

export const STORAGE_CATEGORY_LABELS: Record<StorageCategory, string> = {
  conversations: "Диалоги",
  dashboards: "Дашборды",
  skills: "Навыки",
  vectors: "Векторные хранилища",
  downloads: "Загрузки",
  toolchain: "Компоненты",
  browser: "Браузер",
  cache: "Кэш",
  logs: "Журналы",
  backups: "Резервные копии",
  database: "База данных",
};

export const STORAGE_CATEGORY_FILLS: Record<StorageCategory, string> = {
  conversations: "bg-accent-dark",
  dashboards: "bg-accent-medium",
  skills: "bg-accent-line",
  vectors: "bg-info",
  downloads: "bg-ok",
  toolchain: "bg-ok/60",
  browser: "bg-info/50",
  cache: "bg-warn",
  logs: "bg-warn/50",
  backups: "bg-main-400",
  database: "bg-main-500/45",
};

export const STORAGE_PART_LABELS: Record<StoragePart, string> = {
  models: "Модели",
  embeddings: "Модели эмбеддингов",
  runtimes: "Движки и среды выполнения",
  mcp: "MCP-серверы",
  skills: "Наборы навыков",
  node: "Node.js",
  npm: "Пакеты npm",
  uv: "uv",
  python: "Python",
  "python-packages": "Пакеты Python",
  other: "Прочее",
};

export const STORAGE_PART_FILLS: Record<StoragePart, string> = {
  models: "bg-accent-dark",
  embeddings: "bg-accent-medium",
  runtimes: "bg-accent-light",
  mcp: "bg-info",
  skills: "bg-info/50",
  node: "bg-ok",
  npm: "bg-ok/60",
  uv: "bg-info",
  python: "bg-info/60",
  "python-packages": "bg-info/35",
  other: "bg-main-500/45",
};

const BYTE_UNITS = ["Б", "КБ", "МБ", "ГБ", "ТБ"] as const;

export function storageSize(bytes: number): string {
  let value = Math.max(0, bytes);
  let unit = 0;
  while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 10 ? 0 : 1;
  const number = value.toLocaleString("ru-RU", {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  });
  return `${number} ${BYTE_UNITS[unit]}`;
}

export function clearedCopy(bytes: number): string {
  return bytes > 0 ? `Освобождено ${storageSize(bytes)}.` : "Кэш уже пуст.";
}

export function clearErrorCopy(error: unknown): string {
  if (isIpcError(error) && error.code === AppErrorCode.CONFLICT) return STORAGE_CACHE_BUSY;
  return STORAGE_CLEAR_FAILED;
}
