import { app, BrowserWindow } from "electron";
import { resolvePaths } from "./platform/paths";
import { createLogger } from "./platform/logger";
import type { Logger } from "./platform/logger";
import { createStudioWindow, installContentSecurityPolicy } from "./platform/windows";

app.setName("ZVS AI Studio");
let logger: Logger | undefined;
let window: BrowserWindow | undefined;

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
    // TASK_004: dispose IPC registrations; TASK_006: close the database.
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
      // Construct services here with explicit dependencies.
      // TASK_004: register the typed IPC handlers before creating the window.
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
