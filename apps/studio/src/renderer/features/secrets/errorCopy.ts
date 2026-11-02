import { AppErrorCode } from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";

export interface SecretConsumerRef {
  readonly kind: string;
  readonly id: string;
}

export const UNKNOWN_ERROR_COPY = "Не удалось выполнить операцию. Повторите попытку.";

const COPY: Record<AppErrorCode, string> = {
  [AppErrorCode.UNKNOWN]: UNKNOWN_ERROR_COPY,
  [AppErrorCode.VALIDATION_FAILED]: "Проверьте заполненные поля.",
  [AppErrorCode.NOT_FOUND]: "Секрет не найден.",
  [AppErrorCode.CONFLICT]: "Секрет используется и не может быть удалён.",
  [AppErrorCode.PERMISSION_DENIED]: "Недостаточно прав для этой операции.",
  [AppErrorCode.SECRET_MISSING]: "У секрета нет сохранённого значения.",
  [AppErrorCode.SECRET_DECRYPT_FAILED]:
    "Хранилище ключей недоступно. Чтение и сохранение значений невозможны.",
  [AppErrorCode.PROVIDER_UNREACHABLE]: "Провайдер недоступен.",
  [AppErrorCode.PROVIDER_AUTH_FAILED]: "Провайдер отклонил учётные данные.",
  [AppErrorCode.PROVIDER_SESSION_EXPIRED]:
    "Сессия провайдера истекла. Повторите привязку аккаунта.",
  [AppErrorCode.RATE_LIMITED]: "Превышен лимит запросов. Повторите попытку позже.",
  [AppErrorCode.RUN_CANCELLED]: "Операция отменена.",
  [AppErrorCode.RUN_FAILED]: "Операция завершилась ошибкой.",
  [AppErrorCode.APPROVAL_DENIED]: "Действие не подтверждено.",
  [AppErrorCode.SIDECAR_UNAVAILABLE]: "Фоновый сервис недоступен.",
  [AppErrorCode.MCP_UNAVAILABLE]: "MCP-сервер недоступен.",
  [AppErrorCode.MCP_TIMEOUT]: "MCP-сервер не ответил вовремя.",
  [AppErrorCode.NATIVE_ERROR]: "Сбой нативного модуля.",
  [AppErrorCode.DB_ERROR]: "Сбой локальной базы данных.",
  [AppErrorCode.UNSUPPORTED_FORMAT]: "Формат не поддерживается.",
  [AppErrorCode.LIMIT_EXCEEDED]: "Превышен допустимый предел запроса.",
  [AppErrorCode.NOT_IMPLEMENTED]: "Функция недоступна.",
};

const FIELD_COPY: Record<string, string> = {
  name: "Укажите название.",
  value: "Укажите значение секрета.",
};

export function errorCopy(error: unknown): string {
  if (!isIpcError(error)) return UNKNOWN_ERROR_COPY;
  return COPY[error.code] ?? UNKNOWN_ERROR_COPY;
}

export function fieldErrorsFrom(error: unknown): Record<string, string> {
  if (!isIpcError(error) || error.code !== AppErrorCode.VALIDATION_FAILED) return {};
  const details = error.details ?? {};
  const field = typeof details.field === "string" ? details.field : undefined;
  if (field === undefined) return {};
  return { [field]: FIELD_COPY[field] ?? "Некорректное значение." };
}

export function consumersFrom(error: unknown): SecretConsumerRef[] {
  if (!isIpcError(error) || error.code !== AppErrorCode.CONFLICT) return [];
  const raw = error.details?.consumers;
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((entry) => {
    if (typeof entry !== "object" || entry === null) return [];
    const consumer = entry as { kind?: unknown; id?: unknown };
    if (typeof consumer.kind !== "string" || typeof consumer.id !== "string") return [];
    return [{ kind: consumer.kind, id: consumer.id }];
  });
}
