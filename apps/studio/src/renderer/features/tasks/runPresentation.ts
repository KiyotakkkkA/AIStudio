import {
  mdiChatOutline,
  mdiCreationOutline,
  mdiGraphOutline,
  mdiTrayArrowDown,
  mdiWeb,
} from "@mdi/js";
import type { RunDto, RunSummaryDto } from "@zvs/shared";
import type { StatusTone } from "../../ui/atoms/statusTone";

type RunKind = RunDto["kind"];
type RunStatus = RunDto["status"];

export const KIND_LABELS: Record<RunKind, string> = {
  chat: "чат",
  scenario: "сценарий",
  agentic: "агент",
  job: "задание",
  browser: "браузер",
};

export const KIND_FILTER_LABELS: Record<RunKind, string> = {
  chat: "Ходы чата",
  scenario: "Сценарии",
  agentic: "Сессии агента",
  job: "Задания",
  browser: "Браузер",
};

export const KIND_ICONS: Record<RunKind, string> = {
  chat: mdiChatOutline,
  scenario: mdiGraphOutline,
  agentic: mdiCreationOutline,
  job: mdiTrayArrowDown,
  browser: mdiWeb,
};

export const STATUS_LABELS: Record<RunStatus, string> = {
  queued: "в очереди",
  running: "выполняется",
  blocked: "ждёт решения",
  succeeded: "готово",
  failed: "ошибка",
  cancelled: "остановлено",
  interrupted: "прервано",
};

export const STATUS_TONES: Record<RunStatus, StatusTone> = {
  queued: "idle",
  running: "accent",
  blocked: "warn",
  succeeded: "ok",
  failed: "err",
  cancelled: "idle",
  interrupted: "warn",
};

export const STEP_TONES: Record<string, StatusTone> = {
  running: "accent",
  succeeded: "ok",
  failed: "err",
  cancelled: "idle",
  abandoned: "warn",
};

const pad = (value: number): string => String(value).padStart(2, "0");

export function formatElapsed(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  return hours > 0
    ? `${String(hours)}:${pad(minutes)}:${pad(seconds)}`
    : `${pad(minutes)}:${pad(seconds)}`;
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${String(Math.max(0, Math.round(ms)))} мс`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} с`;
  return formatElapsed(ms);
}

export function formatTime(timestamp: number): string {
  return new Date(timestamp).toLocaleTimeString("ru-RU", { hour12: false });
}

export function formatDateTime(timestamp: number): string {
  return new Date(timestamp).toLocaleString("ru-RU", { hour12: false });
}

export function runElapsed(run: RunSummaryDto, now: number): number {
  const started = run.startedAt ?? run.createdAt;
  return Math.max(0, (run.finishedAt ?? now) - started);
}

export function progressPercent(progress: RunSummaryDto["progress"]): number {
  if (progress.total <= 0) return 0;
  return Math.min(100, Math.round((progress.done / progress.total) * 100));
}

export function shortRunId(id: string): string {
  return `#${id.replace(/-/g, "").slice(0, 6)}`;
}

export function runSubline(run: RunSummaryDto): string {
  const { done, total } = run.progress;
  if (run.kind === "job")
    return total > 0 ? `${String(done)} из ${String(total)} ед. обработано` : "единицы работы";
  if (total === 0) return "без шагов";
  if (run.status === "running" || run.status === "blocked") {
    const current = Math.min(done + 1, total);
    const node = run.activeNodeId === undefined ? "" : ` · ${run.activeNodeId}`;
    return `шаг ${String(current)} из ${String(total)}${node}`;
  }
  return `${String(done)} из ${String(total)} шагов`;
}

export function runNote(run: RunSummaryDto): string {
  if (run.status === "blocked")
    return run.approval === undefined
      ? "ожидает решения"
      : `ожидает решения — ${run.approval.subject}`;
  if (run.error !== undefined) return run.error;
  if (run.outcome?.message !== undefined) return run.outcome.message;
  return "";
}
