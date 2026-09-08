import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { openDatabase, type DatabaseClient } from "../src/host/data/client.ts";
import { migrate } from "../src/host/data/migrate.ts";

export const MIGRATIONS_DIR = join(
  dirname(fileURLToPath(import.meta.url)),
  "../src/host/data/migrations",
);

export interface TemporaryDatabase {
  client: DatabaseClient;
  directory: string;
  file: string;
  backupsDir: string;
  dispose(): void;
}

export function temporaryDatabase(options: { migrate?: boolean } = {}): TemporaryDatabase {
  const directory = mkdtempSync(join(tmpdir(), "studio-db-"));
  const file = join(directory, "studio.sqlite");
  const backupsDir = join(directory, "backups");
  const client = openDatabase({ file });
  if (options.migrate !== false) migrate({ client, migrationsDir: MIGRATIONS_DIR, backupsDir });
  return {
    client,
    directory,
    file,
    backupsDir,
    dispose() {
      client.close();
      rmSync(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 50 });
    },
  };
}
