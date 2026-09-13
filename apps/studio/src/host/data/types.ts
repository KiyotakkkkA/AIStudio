import type SqliteConnection from "better-sqlite3";
import type { BetterSQLite3Database } from "drizzle-orm/better-sqlite3";
import type * as schema from "./schema/index.ts";

type Schema = typeof schema;
export type Database = BetterSQLite3Database<Schema> & { $client: SqliteConnection.Database };
type Transaction = Parameters<Parameters<Database["transaction"]>[0]>[0];
export type DatabaseHandle = Database | Transaction;
