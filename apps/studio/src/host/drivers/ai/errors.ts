import { AppError, AppErrorCode, isAppError, type AuthMode, type ErrorDetails } from "@zvs/shared";

const UNREACHABLE_CODES = new Set([
  "ECONNREFUSED",
  "ECONNRESET",
  "ENOTFOUND",
  "EAI_AGAIN",
  "EHOSTUNREACH",
  "ENETUNREACH",
  "EPIPE",
  "ETIMEDOUT",
  "UND_ERR_CONNECT_TIMEOUT",
  "UND_ERR_HEADERS_TIMEOUT",
  "UND_ERR_SOCKET",
]);

export const SESSION_EXPIRED_CODE = AppErrorCode.PROVIDER_SESSION_EXPIRED;

export interface HttpFailure {
  readonly status: number;
  readonly headers?: Readonly<Record<string, string>>;
  readonly body?: string;
  readonly details?: ErrorDetails;
  readonly authMode?: AuthMode;
}

export function sessionExpired(details: ErrorDetails = {}): AppError {
  return new AppError(
    AppErrorCode.PROVIDER_SESSION_EXPIRED,
    "Сессия аккаунта истекла — требуется повторная привязка",
    { details },
  );
}

export function isSessionExpired(error: unknown): boolean {
  return isAppError(error) && error.code === AppErrorCode.PROVIDER_SESSION_EXPIRED;
}

export function isSignedOutResponse(
  status: number,
  headers: Readonly<Record<string, string>>,
): boolean {
  if (status >= 300 && status < 400) return true;
  return (headers["content-type"] ?? "").toLowerCase().includes("text/html");
}

export function isNotFoundResponse(error: unknown): boolean {
  return isAppError(error) && error.details?.status === 404;
}

export function isCancellation(error: unknown): boolean {
  return (
    (error instanceof Error && error.name === "AbortError") ||
    (isAppError(error) && error.code === AppErrorCode.RUN_CANCELLED)
  );
}

export function cancelled(): AppError {
  return new AppError(AppErrorCode.RUN_CANCELLED, "Запрос к провайдеру отменён");
}

export function timedOut(seconds: number): AppError {
  return new AppError(AppErrorCode.PROVIDER_UNREACHABLE, "Провайдер не ответил вовремя", {
    details: { timeoutSeconds: seconds },
  });
}

export function httpFailure(failure: HttpFailure): AppError {
  const { status } = failure;
  const details: ErrorDetails = { status, ...failure.details };
  const vendorMessage = extractMessage(failure.body);
  if (vendorMessage !== null) details.vendorMessage = vendorMessage;

  if (status === 401 || status === 403) {
    if (failure.authMode === "account") return sessionExpired(details);
    return new AppError(AppErrorCode.PROVIDER_AUTH_FAILED, "Провайдер отклонил учётные данные", {
      details,
    });
  }
  if (status === 429) {
    const retryAfter = parseRetryAfter(failure.headers?.["retry-after"]);
    if (retryAfter !== null) details.retryAfter = retryAfter;
    return new AppError(AppErrorCode.RATE_LIMITED, "Провайдер ограничил частоту запросов", {
      details,
    });
  }
  if (status === 408 || status === 504) {
    return new AppError(AppErrorCode.PROVIDER_UNREACHABLE, "Провайдер не ответил вовремя", {
      details,
    });
  }
  if (status >= 500) {
    return new AppError(AppErrorCode.PROVIDER_UNREACHABLE, "Провайдер вернул ошибку сервера", {
      details,
    });
  }
  return new AppError(AppErrorCode.UNKNOWN, "Провайдер вернул неизвестную ошибку", { details });
}

export function networkFailure(error: unknown): AppError {
  if (isCancellation(error)) return cancelled();
  const code = errorCode(error);
  if (code !== null && UNREACHABLE_CODES.has(code)) {
    return new AppError(AppErrorCode.PROVIDER_UNREACHABLE, "Провайдер недоступен", {
      cause: error,
      details: { syscall: code },
    });
  }
  if (error instanceof Error && /timed?\s?out/i.test(error.message)) {
    return new AppError(AppErrorCode.PROVIDER_UNREACHABLE, "Провайдер не ответил вовремя", {
      cause: error,
    });
  }
  return new AppError(AppErrorCode.UNKNOWN, "Не удалось выполнить запрос к провайдеру", {
    cause: error,
  });
}

export function toAppError(error: unknown): AppError {
  if (isAppError(error)) return error;
  return networkFailure(error);
}

function errorCode(error: unknown): string | null {
  for (let current: unknown = error, depth = 0; current !== undefined && depth < 4; depth += 1) {
    if (typeof current !== "object" || current === null) break;
    const code = (current as { code?: unknown }).code;
    if (typeof code === "string") return code;
    current = (current as { cause?: unknown }).cause;
  }
  return null;
}

function parseRetryAfter(header: string | undefined): number | null {
  if (header === undefined) return null;
  const seconds = Number(header);
  if (Number.isFinite(seconds) && seconds >= 0) return Math.round(seconds);
  const at = Date.parse(header);
  if (Number.isNaN(at)) return null;
  return Math.max(0, Math.round((at - Date.now()) / 1000));
}

function extractMessage(body: string | undefined): string | null {
  if (body === undefined || body.trim().length === 0) return null;
  const parsed = parseJson(body);
  if (typeof parsed === "object" && parsed !== null) {
    const error = (parsed as { error?: unknown }).error;
    if (typeof error === "string") return error;
    if (typeof error === "object" && error !== null) {
      const nested = (error as { message?: unknown }).message;
      if (typeof nested === "string") return nested;
    }
    const message = (parsed as { message?: unknown }).message;
    if (typeof message === "string") return message;
  }
  return body.slice(0, 500);
}

function parseJson(raw: string): unknown {
  try {
    return JSON.parse(raw) as unknown;
  } catch {
    return null;
  }
}
