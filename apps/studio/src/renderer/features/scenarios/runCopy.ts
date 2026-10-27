import { isIpcError } from "@zvs/ipc";
import { AppErrorCode, type ScenarioOnError, type ScenarioOverlap } from "@zvs/shared";
import { plural } from "../connections/connectionPresentation";
import type { RunPhase, RunTab, StepStatus } from "./runModel";

export const RUN_START_FAILED = "Не удалось запустить сценарий.";
export const RUN_LOAD_FAILED = "Не удалось загрузить последний запуск.";
export const RUN_CANCEL_FAILED = "Не удалось остановить запуск.";
export const PINNED_INVALID = "Входные данные должны быть корректным JSON";

export const LAST_RUN = "Последний запуск";
export const NO_RUNS = "Запусков нет";
export const NO_STEP_SELECTED = "Выберите шаг слева";
export const NO_STEP_DATA = "Нет данных";
export const NO_STEP_LOGS = "Записей нет";

export const RUN_PHASE_LABELS: Record<RunPhase, string> = {
  running: "Выполняется",
  succeeded: "Успешно",
  failed: "Ошибка",
  cancelled: "Остановлен",
};

export const STEP_STATUS_LABELS: Record<StepStatus, string> = {
  running: "Выполняется",
  succeeded: "Успешно",
  failed: "Ошибка",
  cancelled: "Остановлен",
  abandoned: "Прерван",
};

export const RUN_TAB_LABELS: Record<RunTab, string> = {
  input: "Вход",
  output: "Выход",
  logs: "Журнал",
};

export const ON_ERROR_LABELS: Record<ScenarioOnError, string> = {
  stop: "Остановить запуск",
  continue: "Продолжить со следующего шага",
};

export const OVERLAP_LABELS: Record<ScenarioOverlap, string> = {
  skip: "Пропускать новый запуск",
  queue: "Ставить в очередь",
};

export const OVERLAP_HINT = "Что делать, если триггер срабатывает во время предыдущего запуска.";

export const TOOL_CALL_LABEL = "Вызов инструмента";

export const APPROVAL_DENIED_COPY = "Разрешение на действие не получено: запрос отклонён или истёк";
export const PERMISSION_DENIED_COPY = "Инструмент запрещён настройками разрешений";
export const APPROVAL_PROMPT = "Шаг ждёт разрешения на инструмент";
export const APPROVAL_DECIDE_FAILED = "Не удалось передать решение по разрешению.";

export function permissionCopy(code: string | null): string | null {
  if (code === AppErrorCode.APPROVAL_DENIED) return APPROVAL_DENIED_COPY;
  if (code === AppErrorCode.PERMISSION_DENIED) return PERMISSION_DENIED_COPY;
  return null;
}

const HOST_WORDED = new Set<AppErrorCode>([
  AppErrorCode.VALIDATION_FAILED,
  AppErrorCode.CONFLICT,
  AppErrorCode.NOT_FOUND,
]);

export function runErrorCopy(error: unknown, fallback: string = RUN_START_FAILED): string {
  if (!isIpcError(error)) return fallback;
  if (HOST_WORDED.has(error.code) && error.message.length > 0) return error.message;
  return fallback;
}

export function itemsLabel(count: number): string {
  return plural(count, "элемент", "элемента", "элементов");
}

export function durationLabel(ms: number | null): string {
  if (ms === null) return "…";
  if (ms < 1000) return `${ms.toLocaleString("ru-RU")} мс`;
  return `${(ms / 1000).toLocaleString("ru-RU", { maximumFractionDigits: 1 })} с`;
}

export function clockLabel(timestamp: number): string {
  return new Date(timestamp).toLocaleTimeString("ru-RU", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function attemptLabel(attempt: number): string {
  return `попытка ${attempt.toLocaleString("ru-RU")}`;
}
