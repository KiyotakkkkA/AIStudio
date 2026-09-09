import { AppErrorCode } from "@zvs/shared";
import { isIpcError } from "@zvs/ipc";

export interface SecretConsumerRef {
  readonly kind: string;
  readonly id: string;
}

export const UNKNOWN_ERROR_COPY = "Не удалось выполнить операцию. Попробуйте ещё раз.";

const COPY: Record<AppErrorCode, string> = {
  [AppErrorCode.UNKNOWN]: UNKNOWN_ERROR_COPY,
  [AppErrorCode.VALIDATION_FAILED]: "Проверьте заполненные поля.",
  [AppErrorCode.NOT_FOUND]: "Секрет не найден — возможно, он уже удалён.",
  [AppErrorCode.CONFLICT]: "Секрет используется и не может быть удалён.",
  [AppErrorCode.PERMISSION_DENIED]: "Недостаточно прав для этой операции.",
  [AppErrorCode.SECRET_MISSING]: "У секрета нет сохранённого значения.",
  [AppErrorCode.SECRET_DECRYPT_FAILED]:
    "Хранилище ключей недоступно, поэтому значение нельзя сохранить или прочитать.",
  [AppErrorCode.PROVIDER_UNREACHABLE]: "Провайдер недоступен.",
  [AppErrorCode.PROVIDER_AUTH_FAILED]: "Провайдер отклонил учётные данные.",
  [AppErrorCode.RATE_LIMITED]: "Слишком много запросов, попробуйте позже.",
  [AppErrorCode.RUN_CANCELLED]: "Операция отменена.",
  [AppErrorCode.RUN_FAILED]: "Операция завершилась ошибкой.",
  [AppErrorCode.APPROVAL_DENIED]: "Действие не подтверждено.",
  [AppErrorCode.SIDECAR_UNAVAILABLE]: "Фоновый сервис недоступен.",
  [AppErrorCode.NATIVE_ERROR]: "Сбой нативного модуля.",
  [AppErrorCode.DB_ERROR]: "Сбой локальной базы данных.",
  [AppErrorCode.UNSUPPORTED_FORMAT]: "Формат не поддерживается.",
};

const ISSUE_COPY: Record<string, string> = {
  invalid_type: "Некорректное значение.",
  invalid_format: "Некорректный формат.",
  invalid_value: "Недопустимое значение.",
  too_small: "Обязательное поле.",
  too_big: "Значение слишком длинное.",
  unrecognized_keys: "Поле не описано схемой типа.",
};

const FIELD_COPY: Record<string, string> = {
  name: "Укажите название.",
  type: "Неизвестный тип секрета.",
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
  if (field !== "fields") return { [field]: FIELD_COPY[field] ?? "Некорректное значение." };

  const issues = Array.isArray(details.issues) ? details.issues : [];
  const errors: Record<string, string> = {};
  for (const issue of issues) {
    if (typeof issue !== "object" || issue === null) continue;
    const entry = issue as { path?: unknown; code?: unknown };
    const path = typeof entry.path === "string" && entry.path.length > 0 ? entry.path : "";
    const code = typeof entry.code === "string" ? entry.code : "";
    const key = path.length === 0 ? "fields" : `fields.${path}`;
    errors[key] = ISSUE_COPY[code] ?? "Некорректное значение.";
  }
  return Object.keys(errors).length > 0 ? errors : { fields: "Проверьте поля схемы." };
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
