import { createChatService } from "../../../test/helpers/chatService.ts";
import { createRunService } from "../../../test/helpers/runService.ts";
import { createSystemService } from "../../../test/helpers/systemService.ts";
import { createProviderService } from "../../../test/helpers/providerService.ts";
import assert from "node:assert/strict";
import { cpSync, existsSync, mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "vitest";
import { AppErrorCode } from "@zvs/shared";
import { openDatabase } from "../src/host/data/client.ts";
import { migrate, prepareDatabase, BACKUPS_KEPT } from "../src/host/data/migrate.ts";
import { isMigrationFailedError } from "../src/host/data/MigrationFailedError.ts";
import { SettingService } from "../src/host/services/SettingService.ts";
import { createHandlers } from "../src/host/ipc/index.ts";
import { createVectorStoreService } from "../../../test/helpers/vectorStoreService.ts";
import { createAccountService } from "../../../test/helpers/accountService.ts";
import {
  captureWindowState,
  DEFAULT_WINDOW_STATE,
  readWindowState,
} from "../src/host/platform/windowState.ts";
import { createFakeClock } from "../../../test/helpers/fakeClock.ts";
import { temporaryDirectory } from "../../../test/helpers/paths.ts";
import { MIGRATIONS_DIR, temporaryDatabase } from "../../../test/helpers/tempDb.ts";
import { createSecretService } from "../../../test/helpers/secretService.ts";

test("the connection is opened with the pragmas the data layer depends on", () => {
  const database = temporaryDatabase({ migrate: false });
  try {
    const connection = database.client.db.$client;
    assert.equal(connection.pragma("journal_mode", { simple: true }), "wal");
    assert.equal(connection.pragma("foreign_keys", { simple: true }), 1);
    assert.equal(connection.pragma("busy_timeout", { simple: true }), 5000);
    assert.equal(connection.pragma("synchronous", { simple: true }), 1);
  } finally {
    database.dispose();
  }
});

test("migrations apply once to an empty file and are a no-op afterwards", () => {
  const database = temporaryDatabase({ migrate: false });
  try {
    const options = {
      client: database.client,
      migrationsDir: MIGRATIONS_DIR,
      backupsDir: database.backupsDir,
    };
    const first = migrate(options);
    assert.deepEqual(first.applied, [
      "0000_setting",
      "0001_secret",
      "0002_provider_model",
      "0003_provider_adapter_auth_mode",
      "0004_account",
      "0005_slimy_tiger_shark",
      "0006_exotic_jane_foster",
      "0007_kernel",
      "0008_permissions",
      "0009_chat",
      "0010_shallow_marvel_boy",
      "0011_run_history",
      "0012_vector_source",
      "0013_download",
      "0014_vector_store_advanced",
    ]);
    const tables = database.client.db.$client
      .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'setting'")
      .all();
    assert.equal(tables.length, 1);

    const second = migrate(options);
    assert.deepEqual(second.applied, []);
    assert.equal(second.backup, undefined);
  } finally {
    database.dispose();
  }
});

test("a backup is written before migrating and only the last three are kept", () => {
  const temp = temporaryDirectory("studio-backup-");
  const directory = temp.path;
  const backupsDir = join(directory, "backups");
  const file = join(directory, "studio.sqlite");
  try {
    const clock = createFakeClock();
    for (let run = 0; run < BACKUPS_KEPT + 2; run++) {
      const client = openDatabase({ file });
      try {
        const report = migrate({
          client,
          migrationsDir: MIGRATIONS_DIR,
          backupsDir,
          now: () => clock.advance(60_000),
        });
        assert.deepEqual(report.applied, [
          "0000_setting",
          "0001_secret",
          "0002_provider_model",
          "0003_provider_adapter_auth_mode",
          "0004_account",
          "0005_slimy_tiger_shark",
          "0006_exotic_jane_foster",
          "0007_kernel",
          "0008_permissions",
          "0009_chat",
          "0010_shallow_marvel_boy",
          "0011_run_history",
          "0012_vector_source",
          "0013_download",
          "0014_vector_store_advanced",
        ]);
        if (run === 0) assert.equal(report.backup, undefined);
        else assert.equal(typeof report.backup, "string");
        client.db.$client.exec("DROP TABLE message");
        client.db.$client.exec("DROP TABLE conversation");
        client.db.$client.exec("DROP TABLE pending_approval");
        client.db.$client.exec("DROP TABLE permission_use");
        client.db.$client.exec("DROP TABLE tool_permission");
        client.db.$client.exec("DROP TABLE run_event");
        client.db.$client.exec("DROP TABLE step");
        client.db.$client.exec("DROP TABLE run");
        client.db.$client.exec("DROP TABLE download");
        client.db.$client.exec("DROP TABLE vector_source");
        client.db.$client.exec("DROP TABLE vector_document");
        client.db.$client.exec("DROP TABLE vector_store");
        client.db.$client.exec("DROP TABLE model");
        client.db.$client.exec("DROP TABLE provider");
        client.db.$client.exec("DROP TABLE account");
        client.db.$client.exec("DROP TABLE secret_usage");
        client.db.$client.exec("DROP TABLE secret");
        client.db.$client.exec("DROP TABLE setting");
        client.db.$client.exec("DROP TABLE __drizzle_migrations");
        client.db.$client.exec("CREATE TABLE IF NOT EXISTS keepalive (id integer primary key)");
      } finally {
        client.close();
      }
    }

    const backups = readdirSync(backupsDir);
    assert.equal(backups.length, BACKUPS_KEPT);
    assert.equal(
      backups.every((name) => name.startsWith("db-") && name.endsWith(".sqlite")),
      true,
    );
  } finally {
    temp.dispose();
  }
});

test("a broken migration blocks boot with MigrationFailedError", () => {
  const temp = temporaryDirectory("studio-broken-");
  const directory = temp.path;
  const migrationsDir = join(directory, "migrations");
  const backupsDir = join(directory, "backups");
  try {
    cpSync(MIGRATIONS_DIR, migrationsDir, { recursive: true });
    writeFileSync(join(migrationsDir, "0000_setting.sql"), "CREATE TABLE (this is not sql;\n");
    const client = openDatabase({ file: join(directory, "studio.sqlite") });
    try {
      assert.throws(
        () => migrate({ client, migrationsDir, backupsDir }),
        (error: unknown) => {
          assert.equal(isMigrationFailedError(error), true);
          assert.equal(isMigrationFailedError(error) && error.code, AppErrorCode.DB_ERROR);
          assert.equal(isMigrationFailedError(error) && error.backupsDir, backupsDir);
          return true;
        },
      );
      const tables = client.db.$client
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'setting'")
        .all();
      assert.equal(tables.length, 0);
    } finally {
      client.close();
    }
  } finally {
    temp.dispose();
  }
});

test("a missing journal is reported as a migration failure, not a crash", () => {
  const temp = temporaryDirectory("studio-nojournal-");
  const directory = temp.path;
  try {
    mkdirSync(join(directory, "migrations"), { recursive: true });
    const client = openDatabase({ file: join(directory, "studio.sqlite") });
    try {
      assert.throws(
        () =>
          migrate({
            client,
            migrationsDir: join(directory, "migrations"),
            backupsDir: join(directory, "backups"),
          }),
        (error: unknown) => isMigrationFailedError(error),
      );
    } finally {
      client.close();
    }
  } finally {
    temp.dispose();
  }
});

test("a corrupt database file is refused at open as a migration failure", () => {
  const temp = temporaryDirectory("studio-corrupt-");
  const directory = temp.path;
  const file = join(directory, "studio.sqlite");
  try {
    writeFileSync(file, "not a database at all, just junk bytes");
    assert.throws(
      () =>
        prepareDatabase({
          file,
          migrationsDir: MIGRATIONS_DIR,
          backupsDir: join(directory, "backups"),
        }),
      (error: unknown) => isMigrationFailedError(error),
    );
  } finally {
    temp.dispose();
  }
});

test("prepareDatabase opens and migrates in one step", () => {
  const temp = temporaryDirectory("studio-prepare-");
  const directory = temp.path;
  try {
    const prepared = prepareDatabase({
      file: join(directory, "studio.sqlite"),
      migrationsDir: MIGRATIONS_DIR,
      backupsDir: join(directory, "backups"),
    });
    try {
      assert.deepEqual(prepared.report.applied, [
        "0000_setting",
        "0001_secret",
        "0002_provider_model",
        "0003_provider_adapter_auth_mode",
        "0004_account",
        "0005_slimy_tiger_shark",
        "0006_exotic_jane_foster",
        "0007_kernel",
        "0008_permissions",
        "0009_chat",
        "0010_shallow_marvel_boy",
        "0011_run_history",
        "0012_vector_source",
        "0013_download",
        "0014_vector_store_advanced",
      ]);
      assert.equal(prepared.client.repositories.settings.all().length, 0);
    } finally {
      prepared.client.close();
    }
  } finally {
    temp.dispose();
  }
});

test("SettingRepository round-trips values and lists them in key order", () => {
  const database = temporaryDatabase();
  try {
    const settings = database.client.repositories.settings;
    assert.equal(settings.get("window.main"), undefined);

    const stored = settings.set("window.main", '{"width":1440}', 1_700_000_000_000);
    assert.deepEqual(stored, {
      key: "window.main",
      value: '{"width":1440}',
      updatedAt: 1_700_000_000_000,
    });
    assert.deepEqual(settings.get("window.main"), stored);

    const updated = settings.set("window.main", '{"width":800}', 1_700_000_001_000);
    assert.equal(updated.value, '{"width":800}');
    assert.equal(settings.all().length, 1);

    settings.set("a.key", '"first"', 1);
    assert.deepEqual(
      settings.all().map((entity) => entity.key),
      ["a.key", "window.main"],
    );

    settings.remove("a.key");
    assert.equal(settings.get("a.key"), undefined);
  } finally {
    database.dispose();
  }
});

test("a transaction rolls the whole unit of work back when it throws", () => {
  const database = temporaryDatabase();
  try {
    const settings = new SettingService({ data: database.client, clock: createFakeClock() });
    settings.set("theme.name", "dark");

    assert.throws(() =>
      database.client.transaction((repositories) => {
        repositories.settings.set("theme.name", '"light"', 2);
        repositories.settings.set("locale.name", '"ru"', 2);
        throw new Error("the unit of work failed halfway");
      }),
    );

    assert.deepEqual(settings.get("theme.name")?.value, "dark");
    assert.equal(settings.get("locale.name"), undefined);

    settings.setMany({ "theme.name": "light", "locale.name": "ru" });
    assert.deepEqual(settings.get("theme.name")?.value, "light");
    assert.deepEqual(settings.get("locale.name")?.value, "ru");
  } finally {
    database.dispose();
  }
});

test("settings channels carry JSON values through the whole chain", async () => {
  const database = temporaryDatabase();
  try {
    const settings = new SettingService({ data: database.client, clock: createFakeClock() });
    const handlers = createHandlers({
      chat: createChatService(database.client),
      runs: createRunService(database.client),
      vectorStores: createVectorStoreService(database.client),
      system: createSystemService(),
      accounts: createAccountService(database.client),
      providers: createProviderService(database.client),
      settings,
      secrets: createSecretService(database.client),
    });
    const geometry = { bounds: { x: 10, y: 20, width: 1600, height: 1000 }, maximized: false };

    const missing = await handlers["settings.get"]({ key: "window.main" });
    assert.deepEqual(missing, { key: "window.main" });

    const written = await handlers["settings.set"]({ key: "window.main", value: geometry });
    assert.deepEqual(written, {
      key: "window.main",
      value: geometry,
      updatedAt: 1_700_000_000_000,
    });
    assert.deepEqual(await handlers["settings.get"]({ key: "window.main" }), written);
  } finally {
    database.dispose();
  }
});

test("window geometry survives a restart and unusable stored state is rejected", () => {
  const database = temporaryDatabase();
  try {
    const settings = new SettingService({ data: database.client });
    const window = {
      isMaximized: () => false,
      isMinimized: () => false,
      isFullScreen: () => false,
      getNormalBounds: () => ({ x: 100, y: 60, width: 1600, height: 1000 }),
    };
    settings.set("window.main", captureWindowState(window));

    assert.deepEqual(readWindowState(settings.get("window.main")?.value), {
      bounds: { x: 100, y: 60, width: 1600, height: 1000 },
      maximized: false,
    });

    assert.deepEqual(readWindowState(undefined), DEFAULT_WINDOW_STATE);
    assert.deepEqual(readWindowState({ bounds: { width: "wide" } }), DEFAULT_WINDOW_STATE);
    assert.deepEqual(readWindowState({ bounds: { width: 10, height: 10 }, maximized: true }), {
      bounds: { width: 1100, height: 700 },
      maximized: true,
    });
  } finally {
    database.dispose();
  }
});

test("the database file and its migration journal both exist on disk", () => {
  const database = temporaryDatabase();
  try {
    assert.equal(existsSync(database.file), true);
    assert.equal(existsSync(join(MIGRATIONS_DIR, "meta", "_journal.json")), true);
  } finally {
    database.dispose();
  }
});
