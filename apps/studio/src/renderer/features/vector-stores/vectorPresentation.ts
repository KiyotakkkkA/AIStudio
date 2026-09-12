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
