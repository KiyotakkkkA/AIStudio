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
