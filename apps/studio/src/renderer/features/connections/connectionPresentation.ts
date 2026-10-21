import type {
  McpHealthDto,
  McpServerDto,
  McpStartup,
  McpTestResultDto,
  ToolTier,
} from "@zvs/shared";
import type { StatusTone } from "../../ui/atoms/statusTone";

export type ServerState =
  "never" | "stopped" | "starting" | "connected" | "reauth" | "restarting" | "failed" | "disabled";

export const STATE_LABELS: Record<ServerState, string> = {
  never: "Подключение не выполнялось",
  stopped: "Остановлен",
  starting: "Запускается",
  connected: "Подключён",
  reauth: "Нужна повторная авторизация",
  restarting: "Сбой · перезапуск",
  failed: "Ошибка подключения",
  disabled: "Выключен",
};

export const STATE_TONES: Record<ServerState, StatusTone> = {
  never: "idle",
  stopped: "idle",
  starting: "accent",
  connected: "ok",
  reauth: "warn",
  restarting: "warn",
  failed: "err",
  disabled: "idle",
};

export const STARTUP_LABELS: Record<McpStartup, string> = {
  manual: "Вручную",
  launch: "При запуске приложения",
};

export const TIER_LABELS: Record<ToolTier, string> = {
  auto: "auto",
  ask: "ask",
  off: "off",
};

export const TIER_HINTS: Record<ToolTier, string> = {
  auto: "Выполняется без подтверждения",
  ask: "Каждый вызов требует подтверждения",
  off: "Вызов запрещён",
};

export const TIER_TEXT: Record<ToolTier, string> = {
  auto: "text-ok",
  ask: "text-warn",
  off: "text-main-400",
};

const AUTH_FAILURE = /\b(401|403)\b|учётные данные|требует входа|unauthori[sz]ed|forbidden/i;
const SIGN_IN_FAILURE = /требует входа/i;

export function needsReauthorisation(lastError: string | null): boolean {
  return lastError !== null && AUTH_FAILURE.test(lastError);
}

export function serverState(server: McpServerDto, health: McpHealthDto | null): ServerState {
  if (!server.enabled || server.status === "disabled") return "disabled";
  if (server.status === "connected") return "connected";
  if (server.status === "starting") return health?.restarting === true ? "restarting" : "starting";
  if (server.status === "failed") {
    if (server.oauth === "required" || needsReauthorisation(server.lastError)) return "reauth";
    return health?.restarting === true ? "restarting" : "failed";
  }
  return server.lastConnectedAt === null ? "never" : "stopped";
}

function plural(count: number, one: string, few: string, many: string): string {
  const tail = count % 100;
  const last = count % 10;
  if (tail >= 11 && tail <= 14) return `${String(count)} ${many}`;
  if (last === 1) return `${String(count)} ${one}`;
  if (last >= 2 && last <= 4) return `${String(count)} ${few}`;
  return `${String(count)} ${many}`;
}

export function toolsWord(count: number): string {
  return plural(count, "инструмент", "инструмента", "инструментов");
}

export function resourcesWord(count: number): string {
  return plural(count, "ресурс", "ресурса", "ресурсов");
}

export function serversWord(count: number): string {
  return plural(count, "сервер", "сервера", "серверов");
}

export function scenariosWord(count: number): string {
  return plural(count, "сценарий", "сценария", "сценариев");
}

export function skillsWord(count: number): string {
  return plural(count, "навык", "навыка", "навыков");
}

export function serverLine(server: McpServerDto, state: ServerState): string {
  if (state !== "connected") return STATE_LABELS[state];
  const parts = [STATE_LABELS.connected, toolsWord(server.toolCount)];
  if (server.resourceCount > 0) parts.push(resourcesWord(server.resourceCount));
  return parts.join(" · ");
}

export function handshakeLine(result: McpTestResultDto): string {
  const parts = ["handshake ok", `${String(result.latencyMs)} ms`];
  if (result.protocolVersion !== null) parts.push(`protocol ${result.protocolVersion}`);
  if (result.serverVersion !== null) {
    parts.push(
      result.serverName === null
        ? `server v${result.serverVersion}`
        : `${result.serverName} v${result.serverVersion}`,
    );
  } else if (result.serverName !== null) {
    parts.push(result.serverName);
  }
  return parts.join(" · ");
}

export function failureCopy(error: string | null): { title: string; hint: string | null } {
  const message = error ?? "Сервер не ответил на рукопожатие.";
  if (SIGN_IN_FAILURE.test(message)) {
    return {
      title: message,
      hint: "Войдите в аккаунт в открывшемся браузере — подключение завершится автоматически.",
    };
  }
  if (needsReauthorisation(message)) {
    return {
      title: message,
      hint: "Проверьте токен в заголовках или войдите через браузер кнопкой «Войти».",
    };
  }
  if (/ENOENT|not found|не найден/i.test(message)) {
    return {
      title: message,
      hint: "Команда не найдена. Укажите полный путь или проверьте, что она есть в PATH.",
    };
  }
  if (/timeout|тайм-?аут|не ответил/i.test(message)) {
    return {
      title: message,
      hint: "Превышено время ожидания. Проверьте аргументы запуска и вывод протокола в stdout.",
    };
  }
  if (/секрет|secret/i.test(message)) {
    return { title: message, hint: "Один из секретов окружения недоступен — выберите другой." };
  }
  if (/выключен/i.test(message)) {
    return { title: message, hint: "Включите сервер и повторите проверку." };
  }
  if (/ECONNREFUSED|недоступен|fetch failed/i.test(message)) {
    return { title: message, hint: "Адрес не отвечает. Убедитесь, что сервер запущен и доступен." };
  }
  return { title: message, hint: null };
}

export function formatUptime(ms: number | null): string {
  if (ms === null) return "—";
  const minutes = Math.floor(ms / 60_000);
  if (minutes < 1) return `${String(Math.max(0, Math.floor(ms / 1000)))} с`;
  const hours = Math.floor(minutes / 60);
  if (hours < 1) return `${String(minutes)} м`;
  const days = Math.floor(hours / 24);
  if (days < 1) return `${String(hours)} ч ${String(minutes % 60)} м`;
  return `${String(days)} д ${String(hours % 24)} ч`;
}

export function formatLatency(ms: number | null): string {
  return ms === null ? "—" : `${String(ms)} мс`;
}

export function formatLogTime(timestamp: number): string {
  return new Date(timestamp).toLocaleTimeString("ru-RU", { hour12: false });
}

export const LOG_LEVEL_TEXT: Record<McpHealthDto["recentLog"][number]["level"], string> = {
  debug: "text-main-500",
  info: "text-main-400",
  warn: "text-warn",
  error: "text-err",
};
