import {
  AppErrorCode,
  type Json,
  type McpServerDto,
  type ToolDefaultTier,
  type ToolDto,
  type ToolInvokeResultDto,
} from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";
import type { StatusTone } from "../../ui/atoms/statusTone";
import {
  serverState,
  STATE_LABELS,
  STATE_TONES,
  toolsWord,
} from "../connections/connectionPresentation";
import type { SourceGroup } from "./toolFilters";

export {
  TIER_HINTS,
  TIER_LABELS,
  TIER_TEXT,
  toolsWord,
} from "../connections/connectionPresentation";

export const TIER_FILTER_LABELS = { auto: "auto", ask: "ask", off: "off" } as const;

export const TIER_TONES = { auto: "ok", ask: "warn", off: "idle" } as const satisfies Record<
  ToolDto["tier"],
  StatusTone
>;

export const DEFAULT_TIER_LABELS: Record<ToolDefaultTier, string> = {
  inferred: "По типу инструмента",
  auto: "auto — без подтверждения",
  ask: "ask — спрашивать",
  off: "off — выключен",
};

export const DEFAULT_TIER_HINT =
  "По типу: только чтение — auto, с побочными эффектами — ask, разрушающие — off.";

export function sourcesWord(count: number): string {
  const tail = count % 100;
  const last = count % 10;
  if (tail >= 11 && tail <= 14) return `${String(count)} источников`;
  if (last === 1) return `${String(count)} источника`;
  return `${String(count)} источников`;
}

export function headerLine(tools: number, sources: number): string {
  return `${toolsWord(tools)} из ${sourcesWord(sources)}`;
}

export function groupTone(group: SourceGroup): StatusTone {
  if (group.source === "builtin") return "accent";
  if (group.server === null) return group.source === "mcp" ? "err" : "idle";
  return STATE_TONES[serverState(group.server, null)];
}

export function sourceLabel(tool: ToolDto, groups: readonly SourceGroup[]): string {
  if (tool.source === "builtin") return "built-in";
  const group = groups.find(
    (entry) => entry.source === tool.source && entry.sourceRef === tool.sourceRef,
  );
  return group?.server?.name ?? group?.label ?? tool.source;
}

export function formatLastUsed(timestamp: number | null, now: number): string {
  if (timestamp === null) return "никогда";
  const elapsed = Math.max(0, now - timestamp);
  const minutes = Math.floor(elapsed / 60_000);
  if (minutes < 1) return "только что";
  if (minutes < 60) return `${String(minutes)} м назад`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${String(hours)} ч назад`;
  const days = Math.floor(hours / 24);
  if (days === 1) return "вчера";
  if (days < 7) return `${String(days)} д назад`;
  return new Date(timestamp).toLocaleDateString("ru-RU", { day: "numeric", month: "short" });
}

export interface Unavailability {
  readonly title: string;
  readonly detail: string | null;
}

export function unavailability(tool: ToolDto, server: McpServerDto | null): Unavailability | null {
  if (tool.available) {
    if (tool.source === "mcp" && server !== null && server.status === "failed")
      return {
        title: "Сервер-источник не подключён",
        detail: server.lastError,
      };
    return null;
  }
  if (tool.source === "mcp") {
    if (server === null)
      return {
        title: "MCP-сервер удалён",
        detail: "Инструмент остался в реестре, но его вызов невозможен.",
      };
    const state = serverState(server, null);
    return {
      title: `Источник офлайн: ${server.name} — ${STATE_LABELS[state].toLowerCase()}`,
      detail:
        server.lastError ??
        (state === "disabled"
          ? "Сервер выключен на странице «Коннекторы»."
          : "Подключите сервер на странице «Коннекторы»."),
    };
  }
  return {
    title: "Инструмент недоступен",
    detail: "Источник больше не предоставляет этот инструмент.",
  };
}

function isRecord(value: Json): value is { [key: string]: Json } {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function itemsWord(count: number): string {
  const tail = count % 100;
  const last = count % 10;
  if (tail >= 11 && tail <= 14) return `${String(count)} элементов`;
  if (last === 1) return `${String(count)} элемент`;
  if (last >= 2 && last <= 4) return `${String(count)} элемента`;
  return `${String(count)} элементов`;
}

function outputSummary(output: Json): string {
  if (Array.isArray(output)) return itemsWord(output.length);
  if (isRecord(output)) {
    if (Array.isArray(output.structured)) return itemsWord(output.structured.length);
    if (Array.isArray(output.hits)) return itemsWord(output.hits.length);
    if (Array.isArray(output.content)) return itemsWord(output.content.length);
  }
  return "ok";
}

export function resultLine(result: ToolInvokeResultDto): string {
  const timing = `${String(result.durationMs)} ms`;
  if (!result.ok) return `ошибка · ${timing}`;
  return `${outputSummary(result.output)} · ${timing}`;
}

export function resultPreview(result: ToolInvokeResultDto, limit = 4000): string {
  const output = result.output;
  const text =
    isRecord(output) && typeof output.text === "string" && output.text.length > 0
      ? output.text
      : JSON.stringify(output, null, 2);
  return text.length > limit ? `${text.slice(0, limit)}\n…` : text;
}

export const UNKNOWN_TOOL_ERROR = "Не удалось выполнить операцию. Попробуйте ещё раз.";

const COPY: Partial<Record<AppErrorCode, string>> = {
  [AppErrorCode.VALIDATION_FAILED]: "Входные данные не подходят под схему инструмента.",
  [AppErrorCode.NOT_FOUND]: "Инструмент не найден — возможно, он уже удалён из реестра.",
  [AppErrorCode.DB_ERROR]: "Сбой локальной базы данных.",
  [AppErrorCode.NOT_IMPLEMENTED]: "Функция пока недоступна.",
};

const HOST_WORDED = new Set<AppErrorCode>([
  AppErrorCode.CONFLICT,
  AppErrorCode.PERMISSION_DENIED,
  AppErrorCode.APPROVAL_DENIED,
  AppErrorCode.MCP_UNAVAILABLE,
  AppErrorCode.MCP_TIMEOUT,
  AppErrorCode.VALIDATION_FAILED,
]);

export function toolErrorCopy(error: unknown): string {
  if (!isIpcError(error)) return UNKNOWN_TOOL_ERROR;
  if (HOST_WORDED.has(error.code) && error.message.length > 0) return error.message;
  return COPY[error.code] ?? UNKNOWN_TOOL_ERROR;
}
