import { EventEmitter } from "node:events";
import { expect, test, vi } from "vitest";
import { BrowserViewManager } from "../src/host/browser/BrowserViewManager.ts";
import { BrowserLifecycle } from "../src/host/browser/lifecycle.ts";
import type { StudioPaths } from "../src/host/platform/paths.ts";

const views = vi.hoisted(() => [] as { webContents: EventEmitter }[]);
const ipcMain = vi.hoisted(() => ({ handle: vi.fn(), removeHandler: vi.fn() }));
vi.mock("electron", () => ({
  BrowserWindow: class {},
  WebContentsView: class {
    webContents: Electron.WebContents = Object.assign(new EventEmitter(), {
      isDestroyed: () => false,
      setWindowOpenHandler: vi.fn(),
      loadURL: vi.fn(async () => undefined),
      getURL: () => "https://chat.qwen.ai/",
      getTitle: () => "Qwen",
      isLoading: () => false,
      send: vi.fn(),
      navigationHistory: { canGoBack: () => false, canGoForward: () => false },
    }) as unknown as Electron.WebContents;
    setBounds = vi.fn();
    constructor(options?: { webContents?: Electron.WebContents }) {
      if (options?.webContents) this.webContents = options.webContents;
      views.push(this);
    }
  },
  ipcMain,
  session: {
    fromPartition: () => ({
      setPermissionRequestHandler: vi.fn(),
      setPermissionCheckHandler: vi.fn(),
      setDevicePermissionHandler: vi.fn(),
      on: vi.fn(),
      webRequest: { onBeforeRequest: vi.fn() },
    }),
  },
}));

test("OAuth navigation preserves linking until the tab actually closes", () => {
  const lifecycle = new BrowserLifecycle();
  const events: unknown[] = [];
  lifecycle.subscribe((event) => events.push(event));
  const window = Object.assign(new EventEmitter(), {
    isDestroyed: () => false,
    getContentSize: () => [1000, 800],
    webContents: { getZoomFactor: () => 1, mainFrame: {} },
    contentView: { children: [], addChildView: vi.fn(), removeChildView: vi.fn() },
  });
  const browser = new BrowserViewManager(
    { browserRendererUrl: "file:///browser-ui/index.html" } as StudioPaths,
    () => window as unknown as Electron.BrowserWindow,
    undefined,
    lifecycle,
  );
  browser.openTab("https://chat.qwen.ai/");
  const page = views.at(-1)!.webContents;
  const event = { preventDefault: vi.fn() };
  page.emit("will-navigate", event, "https://accounts.google.com/signin");
  page.emit("will-navigate", event, "https://chat.qwen.ai/");
  expect(event.preventDefault).not.toHaveBeenCalled();
  expect(events).toEqual([]);
  const hide = vi
    .mocked(ipcMain.handle)
    .mock.calls.find(([channel]) => channel === "browser.hide")![1];
  hide({
    sender: window.webContents,
    senderFrame: window.webContents.mainFrame,
  } as Electron.IpcMainInvokeEvent);
  expect(events).toEqual([]);

  const handler = vi.mocked((page as unknown as Electron.WebContents).setWindowOpenHandler).mock
    .calls[0]![0];
  const popup = handler({ url: "https://accounts.google.com/signin" } as Electron.HandlerDetails);
  expect(popup.action).toBe("allow");
  expect(popup.overrideBrowserWindowOptions?.webPreferences).toMatchObject({
    partition: "persist:browser-work",
    sandbox: true,
    nodeIntegration: false,
  });
  // Chromium's popup contents carry the OAuth POST and opener relationship.
  const original = page as unknown as Electron.WebContents;
  const guest = Object.assign(new EventEmitter(), {
    isDestroyed: () => false,
    setWindowOpenHandler: vi.fn(),
    loadURL: vi.fn(),
    getURL: () => "https://accounts.google.com/signin",
    getTitle: () => "Sign in",
    isLoading: () => false,
    navigationHistory: original.navigationHistory,
  });
  const adopted = popup.createWindow!({
    webContents: guest,
  } as unknown as Electron.BrowserWindowConstructorOptions);
  expect(adopted).toBe(guest);
  expect(adopted.loadURL).not.toHaveBeenCalled();
  expect(handler({ url: "file:///private" } as Electron.HandlerDetails).action).toBe("deny");
  page.emit("render-process-gone");
  expect(events).toEqual([{ type: "link-tab-closed", url: "https://chat.qwen.ai/" }]);
});
