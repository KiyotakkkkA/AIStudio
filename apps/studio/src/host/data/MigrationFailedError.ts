import { AppError, AppErrorCode } from "@zvs/shared";

export class MigrationFailedError extends AppError {
  readonly backupsDir: string;

  constructor(backupsDir: string, cause: unknown) {
    super(AppErrorCode.DB_ERROR, describe(cause), { cause, details: { backupsDir } });
    this.name = "MigrationFailedError";
    this.backupsDir = backupsDir;
  }
}

function describe(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

export function isMigrationFailedError(value: unknown): value is MigrationFailedError {
  return value instanceof MigrationFailedError;
}
