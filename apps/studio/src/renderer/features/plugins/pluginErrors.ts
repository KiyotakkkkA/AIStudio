import { AppErrorCode } from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";

export const UNKNOWN_PLUGIN_ERROR = "Не удалось выполнить операцию. Попробуйте ещё раз.";
export const SIGN_IN_EXPIRED = "Сеанс истёк. Войдите снова.";

const COPY: Partial<Record<AppErrorCode, string>> = {
  [AppErrorCode.VALIDATION_FAILED]: "Проверьте заполненные поля.",
  [AppErrorCode.NOT_FOUND]: "Аккаунт не найден. Возможно, он уже удалён.",
  [AppErrorCode.PERMISSION_DENIED]: "Недостаточно прав для этой операции.",
  [AppErrorCode.SECRET_MISSING]: "У выбранного секрета нет сохранённого значения.",
  [AppErrorCode.SECRET_DECRYPT_FAILED]: "Хранилище ключей недоступно, значение нельзя прочитать.",
  [AppErrorCode.PROVIDER_SESSION_EXPIRED]: SIGN_IN_EXPIRED,
  [AppErrorCode.PROVIDER_AUTH_FAILED]: "Сервис отклонил учётные данные.",
  [AppErrorCode.PROVIDER_UNREACHABLE]: "Сервис недоступен.",
  [AppErrorCode.RATE_LIMITED]: "Превышен лимит запросов к сервису.",
  [AppErrorCode.DB_ERROR]: "Сбой локальной базы данных.",
  [AppErrorCode.NOT_IMPLEMENTED]: "Функция недоступна.",
};

const HOST_WORDED = new Set<AppErrorCode>([
  AppErrorCode.VALIDATION_FAILED,
  AppErrorCode.CONFLICT,
  AppErrorCode.PROVIDER_AUTH_FAILED,
  AppErrorCode.PROVIDER_UNREACHABLE,
  AppErrorCode.RATE_LIMITED,
  AppErrorCode.LIMIT_EXCEEDED,
  AppErrorCode.UNSUPPORTED_FORMAT,
  AppErrorCode.NOT_IMPLEMENTED,
]);

export function pluginErrorCopy(error: unknown): string {
  if (!isIpcError(error)) return UNKNOWN_PLUGIN_ERROR;
  if (HOST_WORDED.has(error.code) && error.message.length > 0) return error.message;
  return COPY[error.code] ?? UNKNOWN_PLUGIN_ERROR;
}

export function pluginFieldErrors(error: unknown): Record<string, string> {
  if (!isIpcError(error) || error.code !== AppErrorCode.VALIDATION_FAILED) return {};
  const field = error.details?.field;
  if (typeof field !== "string" || field.length === 0) return {};
  return { [field]: error.message.length > 0 ? error.message : "Некорректное значение." };
}

export function isSessionExpired(error: unknown): boolean {
  return isIpcError(error) && error.code === AppErrorCode.PROVIDER_SESSION_EXPIRED;
}
