import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import SqliteConnection from "better-sqlite3";
import { drizzle } from "drizzle-orm/better-sqlite3";
import * as schema from "./schema/index.ts";
import { createRepositories, type Repositories } from "./repositories/index.ts";
import type { Database } from "./types.ts";
import type { UnitOfWork } from "./UnitOfWork.ts";

export const IN_MEMORY = ":memory:";

export interface DatabaseClient extends UnitOfWork {
  readonly db: Database;
  readonly file: string;
  close(): void;
}

export interface DatabaseClientOptions {
  file: string;
  busyTimeoutMs?: number;
}

export function openDatabase(options: DatabaseClientOptions): DatabaseClient {
  if (options.file !== IN_MEMORY) mkdirSync(dirname(options.file), { recursive: true });
  const connection = new SqliteConnection(options.file);
  let db: Database;
  try {
    connection.pragma("journal_mode = WAL");
    connection.pragma("foreign_keys = ON");
    connection.pragma(`busy_timeout = ${options.busyTimeoutMs ?? 5000}`);
    connection.pragma("synchronous = NORMAL");
    db = drizzle(connection, { schema, casing: "snake_case" });
  } catch (error: unknown) {
    connection.close();
    throw error;
  }
  const repositories: Repositories = createRepositories(db);

  return {
    db,
    file: options.file,
    repositories,
    transaction: (fn) => db.transaction((tx) => fn(createRepositories(tx))),
    close: () => connection.close(),
  };
}
