import { BrowserWindow, session } from "electron";
import type { StudioPaths } from "./paths";
import type { Logger } from "./logger";

export function installContentSecurityPolicy(developmentUrl?: string): void {
  const connections = developmentUrl
    ? `'self' ${new URL(developmentUrl).origin.replace(/^http/, "ws")}`
    : "'none'";
  const policy = `default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; font-src 'self'; connect-src ${connections}; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`;
  session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
    const headers = { ...details.responseHeaders };
    for (const name of Object.keys(headers)) {
      if (name.toLowerCase() === "content-security-policy") delete headers[name];
    }
    callback({ responseHeaders: { ...headers, "Content-Security-Policy": [policy] } });
  });
}

export function createStudioWindow(
  paths: StudioPaths,
  logger: Logger,
  developmentUrl?: string,
): BrowserWindow {
  const url = developmentUrl ?? paths.rendererUrl;
  const window = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 1100,
    backgroundColor: "#191b20",
    show: false,
    title: "ZVS AI Studio",
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      preload: paths.preloadPath,
    },
  });
  window.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
  const blockExternalNavigation = (event: Electron.Event, destination: string) => {
    if (destination !== url) event.preventDefault();
  };
  window.webContents.on("will-navigate", blockExternalNavigation);
  window.webContents.on("will-redirect", blockExternalNavigation);
  window.once("ready-to-show", () => window.show());

  window.maximize();

  void window.loadURL(url).catch((error: unknown) => {
    logger.log("error", "window", "Failed to load renderer", { error: String(error) });
  });
  return window;
}
