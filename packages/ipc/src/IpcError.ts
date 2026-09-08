import type { AppErrorCode } from "@zvs/shared";

export class IpcError extends Error {
  readonly code: AppErrorCode;
  readonly channel: string;
  readonly details?: Record<string, unknown>;

  constructor(
    code: AppErrorCode,
    message: string,
    channel: string,
    details?: Record<string, unknown>,
  ) {
    super(message);
    this.name = "IpcError";
    this.code = code;
    this.channel = channel;
    this.details = details;
  }
}

export function isIpcError(value: unknown): value is IpcError {
  return value instanceof IpcError;
}
