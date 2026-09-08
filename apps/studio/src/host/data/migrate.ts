import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
} from "node:fs";
import { join } from "node:path";
import { migrate as applyMigrations } from "drizzle-orm/better-sqlite3/migrator";
import { openDatabase, type DatabaseClient } from "./client.ts";
import { MigrationFailedError } from "./MigrationFailedError.ts";
import type { Logger } from "../platform/logger.ts";

const BACKUP_PREFIX = "db-";
const BACKUP_SUFFIX = ".sqlite";
export const BACKUPS_KEPT = 3;

export interface MigrateOptions {
  client: DatabaseClient;
  migrationsDir: string;
  backupsDir: string;
  logger?: Logger;
  keep?: number;
  now?: () => number;
}

export interface MigrateReport {
  applied: readonly string[];
  backup?: string;
}

interface JournalEntry {
  when: number;
  tag: string;
}

interface SchemaState {
  pending: readonly string[];
  populated: boolean;
}

export interface PrepareOptions {
  file: string;
  migrationsDir: string;
  backupsDir: string;
  logger?: Logger;
  keep?: number;
  now?: () => number;
}

export interface PreparedDatabase {
  client: DatabaseClient;
  report: MigrateReport;
}

export function prepareDatabase(options: PrepareOptions): PreparedDatabase {
  const { file, backupsDir, logger } = options;
  let client: DatabaseClient;
  try {
    client = openDatabase({ file });
  } catch (error: unknown) {
    logger?.log("error", "data", "Could not open the database", { file, error: describe(error) });
    throw new MigrationFailedError(backupsDir, error);
  }
  try {
    const report = migrate({ ...options, client });
    return { client, report };
  } catch (error: unknown) {
    client.close();
    throw error;
  }
}

export function migrate(options: MigrateOptions): MigrateReport {
  const { client, migrationsDir, backupsDir, logger } = options;

  try {
    const { pending, populated } = inspect(client, migrationsDir);
    if (pending.length === 0) {
      logger?.log("info", "data", "Schema is up to date");
      return { applied: [] };
    }
    const backup = populated
      ? backupDatabase(client.file, backupsDir, options.now ?? Date.now)
      : undefined;
    if (backup !== undefined) {
      pruneBackups(backupsDir, options.keep ?? BACKUPS_KEPT);
      logger?.log("info", "data", "Backed up the database before migrating", { backup });
    }
    applyMigrations(client.db, { migrationsFolder: migrationsDir });
    for (const tag of pending) logger?.log("info", "data", "Applied migration", { migration: tag });
    return { applied: pending, ...(backup === undefined ? {} : { backup }) };
  } catch (error: unknown) {
    logger?.log("error", "data", "Migration failed", { error: describe(error) });
    throw new MigrationFailedError(backupsDir, error);
  }
}

function describe(error: unknown): string {
  return error instanceof Error ? (error.stack ?? error.message) : String(error);
}

function inspect(client: DatabaseClient, migrationsDir: string): SchemaState {
  const journalPath = join(migrationsDir, "meta", "_journal.json");
  if (!existsSync(journalPath)) throw new Error(`Migration journal is missing: ${journalPath}`);
  const journal = JSON.parse(readFileSync(journalPath, "utf8")) as { entries?: JournalEntry[] };
  const entries = journal.entries ?? [];

  const connection = client.db.$client;
  const tables = connection
    .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")
    .all() as { name: string }[];
  const names = new Set(tables.map((table) => table.name));

  const applied = new Set<number>();
  if (names.has("__drizzle_migrations")) {
    const rows = connection.prepare("SELECT created_at FROM __drizzle_migrations").all();
    for (const row of rows as { created_at: number }[]) applied.add(Number(row.created_at));
  }

  return {
    pending: entries.filter((entry) => !applied.has(entry.when)).map((entry) => entry.tag),
    populated: names.size > 0,
  };
}

function backupDatabase(file: string, backupsDir: string, now: () => number): string | undefined {
  if (!existsSync(file)) return undefined;
  mkdirSync(backupsDir, { recursive: true });
  const stamp = new Date(now()).toISOString().replace(/[:.]/g, "-");
  const target = join(backupsDir, `${BACKUP_PREFIX}${stamp}${BACKUP_SUFFIX}`);
  copyFileSync(file, target);
  return target;
}

function pruneBackups(backupsDir: string, keep: number): void {
  if (!existsSync(backupsDir)) return;
  const backups = readdirSync(backupsDir)
    .filter((name) => name.startsWith(BACKUP_PREFIX) && name.endsWith(BACKUP_SUFFIX))
    .map((name) => join(backupsDir, name))
    .sort((left, right) => statSync(right).mtimeMs - statSync(left).mtimeMs);
  for (const stale of backups.slice(keep)) rmSync(stale, { force: true });
}
