import { appendFileSync, existsSync, mkdirSync, renameSync, rmSync, statSync } from "node:fs";
import { logFilePath } from "./paths.ts";

export type LogLevel = "debug" | "info" | "warn" | "error";
const priorities: Record<LogLevel, number> = { debug: 10, info: 20, warn: 30, error: 40 };

export const REDACTED = "[redacted]";

const KEY_SHAPES: readonly RegExp[] = [
  /\bsk-ant-api[0-9]{2}-[A-Za-z0-9_-]{16,}/g,
  /\bsk-or-v1-[A-Za-z0-9_-]{16,}/g,
  /\bsk-[A-Za-z0-9_-]{16,}/g,
  /\bosk_(?:live|test)_[A-Za-z0-9]{8,}/g,
  /\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}/g,
  /\bxox[abposr]-[A-Za-z0-9-]{10,}/g,
  /\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}/g,
  /\bAKIA[0-9A-Z]{16}\b/g,
  /\bBearer\s+[A-Za-z0-9._~+/-]{16,}=*/gi,
];

const MAX_REDACT_DEPTH = 6;

export function redactSecrets<T>(value: T): T {
  return redact(value, 0) as T;
}

function redact(value: unknown, depth: number): unknown {
  if (typeof value === "string") return redactText(value);
  if (depth >= MAX_REDACT_DEPTH || value === null || typeof value !== "object") return value;
  if (Array.isArray(value)) return value.map((entry) => redact(entry, depth + 1));
  if (Buffer.isBuffer(value)) return REDACTED;
  if (value instanceof Error) return redactText(String(value));
  const result: Record<string, unknown> = {};
  for (const [key, entry] of Object.entries(value)) result[key] = redact(entry, depth + 1);
  return result;
}

function redactText(value: string): string {
  let result = value;
  for (const shape of KEY_SHAPES) result = result.replace(shape, REDACTED);
  return result;
}

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
          ...redactSecrets(fields),
          timestamp: (options.now ?? Date.now)(),
          level,
          scope,
          message: redactSecrets(message),
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
