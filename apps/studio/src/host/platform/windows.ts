import { BrowserWindow, session } from "electron";
import type { StudioPaths } from "./paths.ts";
import type { Logger } from "./logger.ts";
import {
  DEFAULT_WINDOW_STATE,
  MIN_WINDOW_HEIGHT,
  MIN_WINDOW_WIDTH,
  type WindowState,
} from "./windowState.ts";

export function installContentSecurityPolicy(developmentUrl?: string): void {
  const connections = developmentUrl
    ? `'self' ${new URL(developmentUrl).origin.replace(/^http/, "ws")}`
    : "'none'";
  const styles = developmentUrl ? "'self' 'unsafe-inline'" : "'self'";
  const policy = `default-src 'none'; script-src 'self'; style-src ${styles}; img-src 'self'; font-src 'self'; connect-src ${connections}; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`;
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
  state: WindowState = DEFAULT_WINDOW_STATE,
): BrowserWindow {
  const url = developmentUrl ?? paths.rendererUrl;
  const window = new BrowserWindow({
    ...state.bounds,
    minWidth: MIN_WINDOW_WIDTH,
    minHeight: MIN_WINDOW_HEIGHT,
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

  if (state.maximized) window.maximize();

  void window.loadURL(url).catch((error: unknown) => {
    logger.log("error", "window", "Failed to load renderer", { error: String(error) });
  });
  return window;
}

export function createRecoveryWindow(details: {
  message: string;
  backupsDir: string;
  onQuit: () => void;
}): BrowserWindow {
  const window = new BrowserWindow({
    width: 720,
    height: 480,
    backgroundColor: "#191b20",
    title: "ZVS AI Studio — восстановление",
    webPreferences: { contextIsolation: true, nodeIntegration: false, sandbox: true },
  });
  window.removeMenu();
  window.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
  window.webContents.on("will-navigate", (event) => event.preventDefault());
  window.on("closed", details.onQuit);
  void window.loadURL(
    `data:text/html;charset=utf-8,${encodeURIComponent(recoveryHtml(details.message, details.backupsDir))}`,
  );
  return window;
}

const escapeHtml = (value: string): string =>
  value.replace(
    /[&<>"']/g,
    (character) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character] ??
      character,
  );

function recoveryHtml(message: string, backupsDir: string): string {
  return `<!doctype html>
<html lang="ru">
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'">
<title>Восстановление</title>
<style>
  body { margin: 0; padding: 32px; background: #191b20; color: #e6e8ee;
         font: 14px/1.6 system-ui, sans-serif; }
  h1 { font-size: 18px; margin: 0 0 12px; }
  p { margin: 0 0 12px; }
  code { display: block; padding: 12px; border-radius: 6px; background: #23262e;
         color: #f2b8b5; white-space: pre-wrap; word-break: break-word; }
  button { margin-top: 24px; padding: 8px 20px; border: 0; border-radius: 6px;
           background: #3b6ef2; color: #fff; font: inherit; cursor: pointer; }
</style>
</head>
<body>
  <h1>Не удалось обновить базу данных</h1>
  <p>Приложение не запущено, чтобы не работать с наполовину применённой схемой.</p>
  <code>${escapeHtml(message)}</code>
  <p>Резервные копии до миграции лежат здесь:</p>
  <code>${escapeHtml(backupsDir)}</code>
  <button type="button" onclick="window.close()">Выйти</button>
</body>
</html>`;
}
