import { mdiKeyOutline } from "@mdi/js";
import type { SecretScope, SecretSummaryDto } from "@zvs/shared";
import type { StatusTone } from "../../ui/atoms/statusTone";

export const SCOPE_LABELS: Record<SecretScope, string> = {
  personal: "Личный",
  shared: "Общий",
  public: "Публичный",
};

export const SCOPE_OPTIONS: readonly { value: SecretScope; label: string }[] = [
  { value: "personal", label: "Личный" },
  { value: "shared", label: "Доступен сценариям" },
  { value: "public", label: "Публичные метаданные" },
];

const TYPE_ICONS: Record<string, string> = {
  "ollama-cloud": mdiKeyOutline,
};

export function typeIcon(type: string): string | undefined {
  return TYPE_ICONS[type];
}

export function typeInitials(type: string): string {
  const letters = type.replace(/[^a-z]/gi, "");
  return letters.slice(0, 2).toUpperCase();
}

export function maskedHint(hint: string | null): string {
  return hint === null || hint.length === 0 ? "••••••••••••" : hint;
}

export function statusTone(secret: SecretSummaryDto): StatusTone {
  if (secret.rotationStatus === "overdue") return "err";
  if (secret.rotationStatus === "due-soon") return "warn";
  return secret.usageCount === 0 ? "idle" : "ok";
}

export function usageLabel(usageCount: number): string {
  return usageCount === 0 ? "Не используется" : `Используется: ${String(usageCount)}`;
}

export function metaLabel(secret: SecretSummaryDto, now: number): string {
  if (secret.rotationStatus === "overdue") return "Ротация просрочена";
  if (secret.rotationStatus === "due-soon" && secret.rotatesAt !== null) {
    return `Ротация через ${String(daysBetween(now, secret.rotatesAt))} дн.`;
  }
  return `Обновлён ${relativeTime(secret.updatedAt, now)}`;
}

export function relativeTime(at: number, now: number): string {
  const minutes = Math.max(0, Math.round((now - at) / 60_000));
  if (minutes < 1) return "только что";
  if (minutes < 60) return `${String(minutes)} мин назад`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${String(hours)} ч назад`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${String(days)} дн. назад`;
  const months = Math.round(days / 30);
  if (months < 12) return `${String(months)} мес. назад`;
  return `${String(Math.round(months / 12))} г. назад`;
}

export function consumerLabel(kind: string): string {
  switch (kind) {
    case "provider":
      return "Провайдер";
    case "vector_store":
      return "Векторное хранилище";
    case "mcp_server":
      return "MCP-сервер";
    case "integration":
      return "Интеграция";
    default:
      return kind;
  }
}

function daysBetween(now: number, at: number): number {
  return Math.max(0, Math.round((at - now) / 86_400_000));
}
