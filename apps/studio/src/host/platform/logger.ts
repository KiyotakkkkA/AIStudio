import { appendFileSync, existsSync, mkdirSync, renameSync, rmSync, statSync } from "node:fs";
import { logFilePath } from "./paths.ts";

export type LogLevel = "debug" | "info" | "warn" | "error";
const priorities: Record<LogLevel, number> = { debug: 10, info: 20, warn: 30, error: 40 };

export interface Logger {
  log(level: LogLevel, scope: string, message: string, fields?: Record<string, unknown>): void;
  close(): void;
}

export function createLogger(options: {
  directory: string;
  level: LogLevel;
  development: boolean;
  maxBytes?: number;
  backups?: number;
  now?: () => number;
}): Logger {
  const maxBytes = options.maxBytes ?? 5 * 1024 * 1024;
  const backups = options.backups ?? 3;
  if (maxBytes < 1 || !Number.isInteger(backups) || backups < 1) {
    throw new Error("Logger rotation limits must be positive");
  }
  mkdirSync(options.directory, { recursive: true });
  const file = logFilePath(options.directory);
  let size = existsSync(file) ? statSync(file).size : 0;
  let closed = false;
  return {
    log(level, scope, message, fields = {}) {
      if (closed || priorities[level] < priorities[options.level]) return;
      const line =
        JSON.stringify({
          ...fields,
          timestamp: (options.now ?? Date.now)(),
          level,
          scope,
          message,
        }) + "\n";
      const bytes = Buffer.byteLength(line);
      if (size > 0 && size + bytes > maxBytes) {
        rmSync(logFilePath(options.directory, backups), { force: true });
        for (let index = backups - 1; index >= 0; index--) {
          const source = logFilePath(options.directory, index);
          if (existsSync(source)) renameSync(source, logFilePath(options.directory, index + 1));
        }
        size = 0;
      }
      appendFileSync(file, line, "utf8");
      size += bytes;
      if (options.development) process.stdout.write(line);
    },
    // Writes are synchronous and each append closes its descriptor; nothing remains buffered.
    close() {
      closed = true;
    },
  };
}
