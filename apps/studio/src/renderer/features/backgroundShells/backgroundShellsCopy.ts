import { AppErrorCode, type BackgroundShellDto, type BackgroundShellStatus } from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";
import { plural } from "../connections/connectionPresentation";
import type { StatusTone } from "../../ui/atoms/statusTone";

export const BACKGROUND_LABEL = "Фоновые процессы";
export const IN_BACKGROUND = "В фоне";
export const OUTPUT = "Вывод";
export const STOP = "Остановить";
export const STOPPING = "Остановка…";
export const AGENT_TAB = "Процесс агента";
export const CLOSE_TAB = "Закрыть вкладку";
export const READ_ONLY_HINT = "Вывод процесса, запущенного агентом. Ввод недоступен.";
export const LIST_EMPTY = "Нет фоновых процессов";
export const LIST_LABEL = "Фоновые процессы диалога";
export const FOLDER = "Папка";
export const RUN_TIME = "Время работы";
export const STOP_FAILED = "Не удалось остановить процесс";
export const OUTPUT_FAILED = "Не удалось получить вывод процесса";
export const SHELL_GONE = "Процесс больше не отслеживается";

export function runningCount(count: number): string {
  return plural(count, "фоновый процесс", "фоновых процесса", "фоновых процессов");
}

export function statusLabel(status: BackgroundShellStatus, exitCode: number | null): string {
  switch (status) {
    case "running":
      return "Выполняется";
    case "exited":
      return exitCode === null ? "Завершён" : `Завершён, код ${String(exitCode)}`;
    case "stopped":
      return "Остановлен";
    case "failed":
      return "Ошибка";
    case "timed-out":
      return "Превышено время";
  }
}

export const PROCESS_ENDED = "Фоновый процесс завершён";
export const WOKEN_TURN = "Диалог продолжен автоматически";

export function exitCodeText(exitCode: number | null): string {
  return exitCode === null ? "Без кода выхода" : `Код выхода ${String(exitCode)}`;
}

export function endedTone(shell: Pick<BackgroundShellDto, "status" | "exitCode">): StatusTone {
  if (shell.status === "exited" && shell.exitCode === 0) return "ok";
  if (shell.status === "stopped") return "idle";
  return "err";
}

export function commandText(shell: Pick<BackgroundShellDto, "argv" | "program">): string {
  return shell.argv.length > 0 ? shell.argv.join(" ") : shell.program;
}

export function durationText(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  if (hours > 0) return `${String(hours)} ч ${String(minutes % 60)} мин`;
  if (minutes > 0) return `${String(minutes)} мин ${String(seconds % 60)} с`;
  return `${String(seconds)} с`;
}

export function actionErrorCopy(error: unknown, fallback: string): string {
  if (isIpcError(error) && error.code === AppErrorCode.NOT_FOUND) return SHELL_GONE;
  return fallback;
}
