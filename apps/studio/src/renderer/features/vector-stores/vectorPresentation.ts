import type { StatusTone } from "../../ui/atoms/statusTone";
import { plural } from "../connections/connectionPresentation";
import type { VectorHealth } from "@zvs/shared";

export const healthLabels: Record<VectorHealth, string> = {
  healthy: "Исправно",
  stale: "Нужна сверка",
  pending: "Ожидает индексации",
  broken: "Недоступно",
};
export const healthTones: Record<VectorHealth, StatusTone> = {
  healthy: "ok",
  stale: "warn",
  pending: "warn",
  broken: "err",
};

export function vectorsLine(vectors: number, dimension: number): string {
  return `${plural(vectors, "вектор", "вектора", "векторов")} · ${String(dimension)} изм.`;
}

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

export const VECTOR_STORE_TABS = [
  { value: "Documents", label: "Документы" },
  { value: "Test search", label: "Проверка поиска" },
  { value: "Quality", label: "Качество" },
  { value: "Relations", label: "Связи" },
] as const;

export type PrimaryAction = "stop" | "add" | "reconcile" | "rebuild" | "update";

export const PRIMARY_ACTION_LABELS: Record<PrimaryAction, string> = {
  stop: "Остановить индексацию",
  add: "Добавить папку",
  reconcile: "Сверить",
  rebuild: "Переиндексировать",
  update: "Обновить индекс",
};

export function primaryAction(input: {
  readonly status: VectorHealth;
  readonly indexing: boolean;
  readonly sources: number;
  readonly outdated: boolean;
  readonly vectors: number;
}): PrimaryAction {
  if (input.indexing) return "stop";
  if (input.status === "broken" || input.status === "stale") return "reconcile";
  if (input.sources === 0) return "add";
  if (input.outdated && input.vectors > 0) return "rebuild";
  return "update";
}

export function storeFacts(input: {
  readonly documents: number;
  readonly vectors: number;
  readonly bytes: number;
  readonly lastIndexedAt: number | null;
}): string[] {
  return [
    plural(input.documents, "документ", "документа", "документов"),
    plural(input.vectors, "фрагмент", "фрагмента", "фрагментов"),
    formatBytes(input.bytes),
    input.lastIndexedAt === null
      ? "ещё не индексировалось"
      : `обновлено ${formatIndexedAt(input.lastIndexedAt)}`,
  ];
}

export function splitPath(path: string): { readonly name: string; readonly folder: string } {
  const cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return cut < 0
    ? { name: path, folder: "" }
    : { name: path.slice(cut + 1), folder: path.slice(0, cut) };
}
