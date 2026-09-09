import { join } from "node:path";
import { openDatabase, type DatabaseClient } from "../../apps/studio/src/host/data/client.ts";
import { migrate } from "../../apps/studio/src/host/data/migrate.ts";
import { MIGRATIONS_DIR, temporaryDirectory } from "./paths.ts";

export { MIGRATIONS_DIR };

export interface TemporaryDatabase {
  readonly client: DatabaseClient;
  readonly directory: string;
  readonly file: string;
  readonly backupsDir: string;
  dispose(): void;
}

export interface TemporaryDatabaseOptions {
  /** Apply the bundled migrations before handing the client back. Defaults to `true`. */
  migrate?: boolean;
  prefix?: string;
}

/**
 * A migrated SQLite database in a throwaway directory. Every repository test uses this;
 * none may touch the real user-data database.
 */
export function temporaryDatabase(options: TemporaryDatabaseOptions = {}): TemporaryDatabase {
  const directory = temporaryDirectory(options.prefix ?? "studio-db-");
  const file = join(directory.path, "studio.sqlite");
  const backupsDir = join(directory.path, "backups");
  const client = openDatabase({ file });
  if (options.migrate !== false) migrate({ client, migrationsDir: MIGRATIONS_DIR, backupsDir });
  return {
    client,
    directory: directory.path,
    file,
    backupsDir,
    dispose(): void {
      client.close();
      directory.dispose();
    },
  };
}
