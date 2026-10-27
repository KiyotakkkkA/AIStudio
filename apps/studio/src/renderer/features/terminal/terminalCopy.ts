import { AppErrorCode, TERMINAL_SESSIONS_MAX, type TerminalShell } from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";

export const TERMINAL_PANEL = "Терминал";
export const DOCK_LABEL = "Панель терминала";
export const TABS_LABEL = "Сеансы терминала";
export const RESIZE_DOCK = "Изменить высоту терминала";
export const NEW_SESSION = "Новый сеанс";
export const NEW_SESSION_WITH = "Выбрать оболочку";
export const INTERRUPT = "Прервать (Ctrl+C)";
export const RESTART = "Перезапустить";
export const CLEAR = "Очистить";
export const COPY = "Копировать выделение";
export const CLOSE_SESSION = "Закрыть сеанс";
export const RENAME_SESSION = "Переименовать";
export const HIDE_DOCK = "Скрыть терминал";
export const DISMISS_ERROR = "Скрыть сообщение";
export const NO_DIALOG = "Терминал доступен в сохранённом диалоге";
export const STARTING = "Запуск оболочки…";
export const EMPTY = "Нет открытых сеансов";
export const EMPTY_HINT = "Сеанс запускается в рабочей папке диалога.";
export const STATUS_RUNNING = "Работает";

export const SHELL_LABELS: Record<TerminalShell, string> = {
  pwsh: "PowerShell",
  powershell: "Windows PowerShell",
  cmd: "Командная строка",
  gitBash: "Git Bash",
  wsl: "WSL",
};

export const CREATE_FAILED = "Не удалось запустить командную оболочку";
export const NO_SHELL = "Командная оболочка не найдена";
export const SESSIONS_LIMIT = `Открыто максимальное число сеансов в диалоге: ${String(TERMINAL_SESSIONS_MAX)}`;
export const ACTION_FAILED = "Не удалось выполнить действие с сеансом";
export const SESSION_GONE = "Сеанс завершён";

export function exitLabel(exitCode: number | null): string {
  return exitCode === null ? "Завершён" : `Завершён, код ${String(exitCode)}`;
}

export function createErrorCopy(error: unknown): string {
  if (!isIpcError(error)) return CREATE_FAILED;
  if (error.code === AppErrorCode.NOT_FOUND) return NO_SHELL;
  if (error.code === AppErrorCode.CONFLICT) return SESSIONS_LIMIT;
  return CREATE_FAILED;
}

export function actionErrorCopy(error: unknown): string {
  if (isIpcError(error) && error.code === AppErrorCode.CONFLICT) return SESSION_GONE;
  return ACTION_FAILED;
}
