import type { AppErrorCode } from "./AppErrorCode.js";
import type { ErrorDetails } from "./Result.js";

export class AppError extends Error {
  readonly code: AppErrorCode;
  readonly details?: ErrorDetails;

  constructor(
    code: AppErrorCode,
    message: string,
    options?: { cause?: unknown; details?: ErrorDetails },
  ) {
    super(message, options?.cause === undefined ? undefined : { cause: options.cause });
    this.name = "AppError";
    this.code = code;
    this.details = options?.details;
  }
}

export function isAppError(value: unknown): value is AppError {
  return value instanceof AppError;
}
