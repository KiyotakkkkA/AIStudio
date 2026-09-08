import { app, BrowserWindow, ipcMain } from "electron";
import { contract } from "@zvs/shared";
import { createIpcServer } from "@zvs/ipc";
import type { IpcServer } from "@zvs/ipc";
import { createHandlers } from "./ipc";
import { resolvePaths } from "./platform/paths";
import { createLogger } from "./platform/logger";
import type { Logger } from "./platform/logger";
import { createEventBus } from "./platform/events";
import type { EventBus, WindowSender } from "./platform/events";
import { createEventRecorder, recordingEnabled } from "./platform/eventRecorder";
import { createStudioWindow, installContentSecurityPolicy } from "./platform/windows";

app.setName("ZVS AI Studio");
let logger: Logger | undefined;
let window: BrowserWindow | undefined;
let ipcServer: IpcServer | undefined;
let eventBus: EventBus | undefined;

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
    // TASK_006: close the database.
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
      // TASK_006: open database and finish migrations before constructing services.
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
      ipcServer = createIpcServer(contract, createHandlers({ events: eventBus }), {
        ipcMain,
        logger,
        validateOutput: !app.isPackaged,
      });
      logger.log("info", "host", "Registered IPC channels", { count: ipcServer.channels.length });
      const developmentUrl = app.isPackaged ? undefined : process.env.ELECTRON_RENDERER_URL;
      installContentSecurityPolicy(developmentUrl);
      window = createStudioWindow(paths, logger, developmentUrl);
      app.on("activate", () => {
        if (BrowserWindow.getAllWindows().length === 0 && logger) {
          window = createStudioWindow(paths, logger, developmentUrl);
        }
      });
    })
    .catch((error: unknown) => {
      if (logger) logger.log("error", "host", "Startup failed", { error: String(error) });
      else process.stderr.write(`Startup failed: ${String(error)}\n`);
      app.exit(1);
    });
}
