import { app, BrowserWindow, ipcMain, safeStorage } from "electron";
import { contract } from "@zvs/shared";
import { createIpcServer } from "@zvs/ipc";
import type { IpcServer } from "@zvs/ipc";
import { createHandlers } from "./ipc";
import { resolvePaths } from "./platform/paths";
import type { StudioPaths } from "./platform/paths";
import { createLogger } from "./platform/logger";
import type { Logger } from "./platform/logger";
import { createEventBus } from "./platform/events";
import type { EventBus, WindowSender } from "./platform/events";
import { createEventRecorder, recordingEnabled } from "./platform/eventRecorder";
import {
  createRecoveryWindow,
  createStudioWindow,
  installContentSecurityPolicy,
} from "./platform/windows";
import { captureWindowState, readWindowState, WINDOW_STATE_KEY } from "./platform/windowState";
import type { DatabaseClient } from "./data/client";
import { prepareDatabase } from "./data/migrate";
import { isMigrationFailedError } from "./data/MigrationFailedError";
import { CryptoService } from "./services/CryptoService";
import { SecretService } from "./services/SecretService";
import { SettingService } from "./services/SettingService";

app.setName("ZVS AI Studio");
let logger: Logger | undefined;
let window: BrowserWindow | undefined;
let ipcServer: IpcServer | undefined;
let eventBus: EventBus | undefined;
let database: DatabaseClient | undefined;
let settings: SettingService | undefined;
let secrets: SecretService | undefined;

if (!app.requestSingleInstanceLock()) {
  app.quit();
} else {
  app.on("second-instance", () => {
    if (!window || window.isDestroyed()) return;
    if (window.isMinimized()) window.restore();
    window.show();
    window.focus();
    logger?.log("info", "host", "Focused existing instance");
  });
  app.on("window-all-closed", () => {
    if (process.platform !== "darwin") app.quit();
  });
  app.on("will-quit", () => {
    logger?.log("info", "host", "Shutdown");
    ipcServer?.dispose();
    eventBus?.dispose();
    database?.close();
    logger?.close();
  });

  void app
    .whenReady()
    .then(() => {
      const paths = resolvePaths({
        userData: app.getPath("userData"),
        resources: process.resourcesPath,
        appRoot: app.getAppPath(),
        packaged: app.isPackaged,
      });
      logger = createLogger({
        directory: paths.logsDir,
        level: "info",
        development: !app.isPackaged,
      });
      logger.log("info", "host", "Startup", { version: app.getVersion() });

      try {
        const prepared = prepareDatabase({
          file: paths.dbPath,
          migrationsDir: paths.migrationsDir,
          backupsDir: paths.backupsDir,
          logger,
        });
        database = prepared.client;
        logger.log("info", "host", "Database ready", { applied: prepared.report.applied.length });
      } catch (error: unknown) {
        if (!isMigrationFailedError(error)) throw error;
        window = createRecoveryWindow({
          message: error.message,
          backupsDir: error.backupsDir,
          onQuit: () => app.quit(),
        });
        return;
      }

      settings = new SettingService({ data: database });
      const crypto = new CryptoService(safeStorage);
      secrets = new SecretService({ data: database, crypto, logger });
      logger.log("info", "host", "Secret store ready", {
        encryption: crypto.isAvailable(),
        count: secrets.list().length,
      });
      const recording = recordingEnabled();
      eventBus = createEventBus({
        logger,
        senders: (): readonly WindowSender[] =>
          BrowserWindow.getAllWindows()
            .filter((open) => !open.isDestroyed())
            .map((open) => open.webContents),
        recorder: recording
          ? createEventRecorder({ directory: paths.streamsDir, logger })
          : undefined,
      });
      logger.log("info", "host", "Opened the event channel", { recording });
      ipcServer = createIpcServer(
        contract,
        createHandlers({ events: eventBus, settings, secrets }),
        {
          ipcMain,
          logger,
          validateOutput: !app.isPackaged,
        },
      );
      logger.log("info", "host", "Registered IPC channels", { count: ipcServer.channels.length });
      const developmentUrl = app.isPackaged ? undefined : process.env.ELECTRON_RENDERER_URL;
      installContentSecurityPolicy(developmentUrl);
      window = openStudioWindow(paths, logger, settings, developmentUrl);
      app.on("activate", () => {
        if (BrowserWindow.getAllWindows().length === 0 && logger && settings) {
          window = openStudioWindow(paths, logger, settings, developmentUrl);
        }
      });
    })
    .catch((error: unknown) => {
      if (logger) logger.log("error", "host", "Startup failed", { error: String(error) });
      else process.stderr.write(`Startup failed: ${String(error)}\n`);
      app.exit(1);
    });
}

function openStudioWindow(
  paths: StudioPaths,
  hostLogger: Logger,
  settingService: SettingService,
  developmentUrl?: string,
): BrowserWindow {
  const stored = settingService.get(WINDOW_STATE_KEY);
  const created = createStudioWindow(
    paths,
    hostLogger,
    developmentUrl,
    readWindowState(stored?.value),
  );
  created.on("close", () => {
    try {
      settingService.set(WINDOW_STATE_KEY, captureWindowState(created));
    } catch (error: unknown) {
      hostLogger.log("warn", "window", "Could not store the window geometry", {
        error: String(error),
      });
    }
  });
  return created;
}
