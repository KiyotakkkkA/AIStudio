import { AppErrorCode } from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";

export const UNKNOWN_CONNECTION_ERROR = "Не удалось выполнить операцию. Попробуйте ещё раз.";

const COPY: Partial<Record<AppErrorCode, string>> = {
  [AppErrorCode.VALIDATION_FAILED]: "Проверьте заполненные поля.",
  [AppErrorCode.NOT_FOUND]: "Сервер не найден — возможно, он уже удалён.",
  [AppErrorCode.PERMISSION_DENIED]: "Недостаточно прав для этой операции.",
  [AppErrorCode.SECRET_MISSING]: "У выбранного секрета нет сохранённого значения.",
  [AppErrorCode.SECRET_DECRYPT_FAILED]: "Хранилище ключей недоступно — значение нельзя прочитать.",
  [AppErrorCode.DB_ERROR]: "Сбой локальной базы данных.",
  [AppErrorCode.NOT_IMPLEMENTED]: "Эта функция ещё не перенесена в новую оболочку приложения.",
};

const HOST_WORDED = new Set<AppErrorCode>([
  AppErrorCode.CONFLICT,
  AppErrorCode.MCP_UNAVAILABLE,
  AppErrorCode.MCP_TIMEOUT,
]);

export function connectionErrorCopy(error: unknown): string {
  if (!isIpcError(error)) return UNKNOWN_CONNECTION_ERROR;
  if (HOST_WORDED.has(error.code) && error.message.length > 0) return error.message;
  return COPY[error.code] ?? UNKNOWN_CONNECTION_ERROR;
}

export function connectionFieldErrors(error: unknown): Record<string, string> {
  if (!isIpcError(error) || error.code !== AppErrorCode.VALIDATION_FAILED) return {};
  const field = error.details?.field;
  if (typeof field !== "string" || field.length === 0) return {};
  return { [field]: error.message.length > 0 ? error.message : "Некорректное значение." };
}
