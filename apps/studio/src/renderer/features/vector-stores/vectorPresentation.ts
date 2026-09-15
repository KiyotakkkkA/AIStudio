import type { VectorHealth } from "@zvs/shared";

export const healthLabels: Record<VectorHealth, string> = {
  healthy: "Исправно",
  stale: "Нужна сверка",
  pending: "Ожидает индексации",
  broken: "Недоступно",
};
export const healthColors: Record<VectorHealth, string> = {
  healthy: "bg-ok",
  stale: "bg-warn",
  pending: "bg-warn",
  broken: "bg-err",
};

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${String(bytes)} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatIndexedAt(timestamp: number): string {
  return new Date(timestamp).toLocaleString("ru-RU", { hour12: false });
}

/**
 * Coarsens as the estimate grows, because the precision is not real: a corpus with hours left is
 * being estimated from a chunk total that is still converging.
 */
export function formatDuration(milliseconds: number | undefined): string {
  if (milliseconds === undefined || !Number.isFinite(milliseconds)) return "—";
  const seconds = Math.round(milliseconds / 1000);
  if (seconds < 60) return `${String(Math.max(seconds, 1))} с`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${String(minutes)} мин`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0 ? `${String(hours)} ч` : `${String(hours)} ч ${String(rest)} мин`;
}

export function formatThroughput(perSecond: number): string {
  if (perSecond <= 0) return "— фрагм./с";
  const rounded = perSecond >= 10 ? Math.round(perSecond) : Math.round(perSecond * 10) / 10;
  return `${rounded.toLocaleString("ru-RU")} фрагм./с`;
}
