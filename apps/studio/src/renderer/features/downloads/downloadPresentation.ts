import {
  mdiBookOpenPageVariantOutline,
  mdiCubeOutline,
  mdiPowerPlugOutline,
  mdiVectorTriangle,
} from "@mdi/js";
import type {
  CatalogueItemState,
  DiskCategory,
  DownloadItemKind,
  DownloadStatus,
} from "@zvs/shared";
import type { StatusTone } from "../../ui/atoms/statusTone";

export const KIND_LABELS: Record<DownloadItemKind, string> = {
  model: "модель",
  embedding: "эмбеддинги",
  mcp: "MCP-сервер",
  skill: "набор навыков",
};

export const KIND_FILTER_LABELS: Record<DownloadItemKind, string> = {
  model: "Модели",
  embedding: "Модели эмбеддингов",
  mcp: "MCP-серверы",
  skill: "Наборы навыков",
};

export const KIND_ICONS: Record<DownloadItemKind, string> = {
  model: mdiCubeOutline,
  embedding: mdiVectorTriangle,
  mcp: mdiPowerPlugOutline,
  skill: mdiBookOpenPageVariantOutline,
};

export const STATE_LABELS: Record<CatalogueItemState, string> = {
  installed: "установлено",
  update: "обновление",
  available: "доступно",
  downloading: "загружается",
  queued: "в очереди",
};

export const STATE_FILTER_LABELS: Record<CatalogueItemState, string> = {
  installed: "Установлено",
  update: "Есть обновление",
  available: "Доступно",
  downloading: "Загружается",
  queued: "В очереди",
};

export const STATE_TONES: Record<CatalogueItemState, StatusTone> = {
  installed: "ok",
  update: "warn",
  available: "idle",
  downloading: "accent",
  queued: "idle",
};

export const STATUS_LABELS: Record<DownloadStatus, string> = {
  queued: "в очереди",
  running: "загружается",
  paused: "приостановлено",
  succeeded: "готово",
  failed: "ошибка",
  cancelled: "отменено",
};

export const STATUS_TONES: Record<DownloadStatus, StatusTone> = {
  queued: "idle",
  running: "accent",
  paused: "warn",
  succeeded: "ok",
  failed: "err",
  cancelled: "idle",
};

export const DISK_CATEGORY_LABELS: Record<DiskCategory, string> = {
  models: "Модели",
  embeddings: "Эмбеддинги",
  mcp: "MCP-серверы",
  skills: "Наборы навыков",
  vectors: "Векторные индексы",
  other: "Прочее",
};

export const DISK_CATEGORY_ORDER: readonly DiskCategory[] = [
  "models",
  "embeddings",
  "vectors",
  "mcp",
  "skills",
  "other",
];

export const DISK_CATEGORY_COLORS: Record<DiskCategory, string> = {
  models: "bg-accent-dark",
  embeddings: "bg-accent-medium",
  vectors: "bg-accent-light",
  mcp: "bg-main-500",
  skills: "bg-main-600",
  other: "bg-main-700",
};

const UNITS = ["Б", "КБ", "МБ", "ГБ", "ТБ"] as const;

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 Б";
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 ? 0 : value < 10 ? 1 : 0;
  return `${value.toFixed(digits)} ${UNITS[unit]}`;
}

export function formatRate(bytesPerSecond: number): string {
  if (bytesPerSecond <= 0) return "—";
  return `${formatBytes(bytesPerSecond)}/с`;
}

export function formatEta(milliseconds: number | undefined): string {
  if (milliseconds === undefined || !Number.isFinite(milliseconds)) return "—";
  const seconds = Math.round(milliseconds / 1000);
  if (seconds < 60) return `${String(Math.max(seconds, 1))} с`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${String(minutes)} мин`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `${String(hours)} ч` : `${String(hours)} ч ${String(rest)} мин`;
}

export function percentOf(done: number, total: number): number {
  if (total <= 0) return 0;
  return Math.min(100, Math.max(0, Math.round((done / total) * 100)));
}

export function shortChecksum(algorithm: string, value: string): string {
  if (value.length <= 12) return `${algorithm}:${value}`;
  return `${algorithm}:${value.slice(0, 4)}…${value.slice(-4)}`;
}
