import { AppErrorCode } from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";

export const UNKNOWN_PROVIDER_ERROR = "Не удалось выполнить операцию. Попробуйте ещё раз.";

const COPY: Partial<Record<AppErrorCode, string>> = {
  [AppErrorCode.VALIDATION_FAILED]: "Проверьте заполненные поля.",
  [AppErrorCode.NOT_FOUND]: "Провайдер не найден — возможно, он уже удалён.",
  [AppErrorCode.CONFLICT]: "Провайдер занят другой операцией — дождитесь её окончания.",
  [AppErrorCode.PERMISSION_DENIED]: "Недостаточно прав для этой операции.",
  [AppErrorCode.SECRET_MISSING]: "У выбранного секрета нет сохранённого значения.",
  [AppErrorCode.SECRET_DECRYPT_FAILED]: "Хранилище ключей недоступно — значение нельзя прочитать.",
  [AppErrorCode.PROVIDER_UNREACHABLE]: "Провайдер недоступен.",
  [AppErrorCode.PROVIDER_AUTH_FAILED]: "Провайдер отклонил учётные данные.",
  [AppErrorCode.PROVIDER_SESSION_EXPIRED]: "Сессия истекла — войдите в аккаунт заново.",
  [AppErrorCode.RATE_LIMITED]: "Слишком много запросов, попробуйте позже.",
  [AppErrorCode.DB_ERROR]: "Сбой локальной базы данных.",
};

export function providerErrorCopy(error: unknown): string {
  if (!isIpcError(error)) return UNKNOWN_PROVIDER_ERROR;
  return COPY[error.code] ?? UNKNOWN_PROVIDER_ERROR;
}

export function providerFieldErrors(error: unknown): Record<string, string> {
  if (!isIpcError(error) || error.code !== AppErrorCode.VALIDATION_FAILED) return {};
  const field = error.details?.field;
  if (typeof field !== "string" || field.length === 0) return {};
  return { [field]: "Некорректное значение." };
}
