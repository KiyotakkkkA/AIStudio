import { BrowserWindow, WebContentsView, ipcMain, session } from "electron";
import type { Cookie, IpcMainInvokeEvent, WebContents, WebPreferences } from "electron";
import { BrowserBounds, BrowserCommand, type BrowserCookie, type BrowserState } from "@zvs/shared";
import type { StudioPaths } from "../platform/paths.ts";
import { createId } from "../platform/ids.ts";
import type { BrowserLifecycle } from "./lifecycle.ts";
import {
  BROWSER_PARTITION,
  CHROME_HEIGHT,
  STATUS_HEIGHT,
  isWebUrl,
  navigationUrl,
} from "./policy.ts";

interface Tab {
  id: string;
  view: WebContentsView;
  error?: string;
}

export class BrowserViewManager {
  private window?: BrowserWindow;
  private chrome?: WebContentsView;
  private visible = false;
  private bounds = { x: 0, y: 0, width: 0, height: 0 };
  private tabs: Tab[] = [];
  private activeId: string | null = null;
  private attached?: WebContentsView;
  private sitesVisible = false;
  private readonly linkTabs = new Map<WebContents, string>();
  private readonly stopLifecycle?: () => void;
  private readonly profile = session.fromPartition(BROWSER_PARTITION);

  constructor(
    private readonly paths: StudioPaths,
    private readonly studio: () => BrowserWindow | undefined,
    private readonly developmentUrl?: string,
    private readonly lifecycle?: BrowserLifecycle,
    private readonly navigateWorkspace?: (path: "/browser" | "/providers") => void,
  ) {
    this.stopLifecycle = lifecycle?.subscribe((event) => {
      if (event.type !== "link-finished") return;
      for (const [contents, url] of this.linkTabs) {
        if (url === event.url) this.linkTabs.delete(contents);
      }
    });
    this.profile.setPermissionRequestHandler((_contents, _permission, callback) => callback(false));
    this.profile.setPermissionCheckHandler(() => false);
    this.profile.setDevicePermissionHandler(() => false);
    this.profile.on("will-download", (event) => event.preventDefault());
    // Also reject non-web main/subframe requests initiated outside will-navigate (e.g. redirects).
    this.profile.webRequest.onBeforeRequest((details, callback) => {
      const frame = details.resourceType === "mainFrame" || details.resourceType === "subFrame";
      callback({ cancel: frame && !isWebUrl(details.url) && details.url !== "about:blank" });
    });
    ipcMain.handle("browser.mount", (event, input: unknown) => {
      if (!this.isSender(event, this.studio())) throw new Error("Forbidden");
      const parsed = BrowserBounds.safeParse(input);
      if (!parsed.success) throw new Error("Invalid browser bounds");
      this.bounds = parsed.data;
      this.open();
    });
    ipcMain.handle("browser.hide", (event) => {
      if (!this.isSender(event, this.studio())) throw new Error("Forbidden");
      this.visible = false;
      this.layout();
    });
    ipcMain.handle("browser.command", async (event, input: unknown) => {
      if (
        !this.chrome ||
        event.sender !== this.chrome.webContents ||
        event.senderFrame !== this.chrome.webContents.mainFrame
      )
        throw new Error("Forbidden");
      const parsed = BrowserCommand.safeParse(input);
      if (!parsed.success) throw new Error("Invalid browser command");
      return this.command(parsed.data);
    });
  }

  private isSender(event: IpcMainInvokeEvent, window?: BrowserWindow): boolean {
    return (
      !!window &&
      !window.isDestroyed() &&
      event.sender === window.webContents &&
      event.senderFrame === window.webContents.mainFrame
    );
  }

  open(): void {
    this.visible = true;
    if (this.chrome && !this.chrome.webContents.isDestroyed()) {
      this.layout();
      return;
    }
    const window = this.studio();
    if (!window || window.isDestroyed()) throw new Error("Studio window is unavailable");
    const chrome = new WebContentsView({
      webPreferences: {
        preload: this.paths.browserPreloadPath,
        sandbox: true,
        contextIsolation: true,
        nodeIntegration: false,
      },
    });
    this.window = window;
    this.chrome = chrome;
    chrome.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
    chrome.webContents.on("will-navigate", (event) => event.preventDefault());
    chrome.webContents.on("will-redirect", (event) => event.preventDefault());
    window.on("resize", () => this.layout());
    chrome.webContents.on("did-finish-load", () => this.publish());
    window.once("closed", () => {
      this.lifecycle?.emit({ type: "workspace-closed" });
      this.window = undefined;
      this.visible = false;
      this.attached = undefined;
      chrome.webContents.close({ waitForBeforeUnload: false });
      this.chrome = undefined;
      for (const tab of this.tabs) tab.view.webContents.close({ waitForBeforeUnload: false });
      this.tabs = [];
      this.activeId = null;
      this.sitesVisible = false;
      this.profile.flushStorageData();
      void this.profile.cookies.flushStore().catch(() => undefined);
    });
    const url = this.developmentUrl
      ? new URL("browser-ui/index.html", this.developmentUrl).href
      : this.paths.browserRendererUrl;
    void chrome.webContents.loadURL(url).catch(() => undefined);
    this.newTab();
  }

  openTab(url: string): void {
    this.navigateWorkspace?.("/browser");
    this.open();
    const contents = this.newTab(url);
    this.linkTabs.set(contents, url);
    const closed = () => {
      if (!this.linkTabs.delete(contents)) return;
      this.lifecycle?.emit({ type: "link-tab-closed", url });
    };
    contents.once("destroyed", closed);
    contents.on("render-process-gone", closed);
  }

  private active(): Tab | undefined {
    return this.tabs.find((tab) => tab.id === this.activeId);
  }

  private sitePreferences(): WebPreferences {
    return {
      partition: BROWSER_PARTITION,
      preload: this.paths.sitePreloadPath,
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webSecurity: true,
      allowRunningInsecureContent: false,
      webviewTag: false,
    };
  }

  private newTab(url?: string, webContents?: WebContents): WebContents {
    if (this.tabs.length >= 30) throw new Error("Close a tab before opening another (limit: 30)");
    const destination = url === undefined ? undefined : navigationUrl(url);
    const view = new WebContentsView({
      ...(webContents ? { webContents } : {}),
      webPreferences: this.sitePreferences(),
    });
    const tab: Tab = { id: createId(), view };
    const contents = view.webContents;
    this.tabs.push(tab);
    this.activeId = tab.id;
    this.sitesVisible = false;
    const block = (event: Electron.Event, url: string) => {
      if (!isWebUrl(url)) {
        event.preventDefault();
        tab.error = "Navigation blocked: only HTTP/HTTPS is allowed";
        this.publish();
      }
    };
    contents.on("will-navigate", block);
    contents.on("will-redirect", block);
    contents.on("will-frame-navigate", (event) => block(event, event.url));
    contents.on("will-attach-webview", (event) => event.preventDefault());
    contents.setWindowOpenHandler((details) => {
      if ((!isWebUrl(details.url) && details.url !== "about:blank") || this.tabs.length >= 30)
        return { action: "deny" };
      return {
        action: "allow",
        overrideBrowserWindowOptions: { webPreferences: this.sitePreferences() },
        // Adopt Chromium's popup contents instead of replaying the URL as a GET.
        // This preserves POST bodies, referrers, window.opener and OAuth postMessage flows.
        // No native BrowserWindow is created; the popup is another isolated tab.
        createWindow: (options) => {
          // Electron documents this field for createWindow but omits it from the public
          // BrowserWindowConstructorOptions type. Background tabs can omit it at runtime.
          const { webContents } = options as typeof options & { webContents?: WebContents };
          return this.newTab(
            !webContents && details.url !== "about:blank" ? details.url : undefined,
            webContents,
          );
        },
      };
    });
    contents.on("did-start-navigation", (_event, _url, inPlace, mainFrame) => {
      if (mainFrame && !inPlace) tab.error = undefined;
      this.publish();
    });
    contents.on("did-start-loading", () => this.publish());
    contents.on("did-stop-loading", () => this.publish());
    contents.on("did-navigate", () => this.publish());
    contents.on("did-navigate-in-page", () => this.publish());
    contents.on("page-title-updated", () => this.publish());
    contents.on("did-fail-load", (_event, code, _description, _url, mainFrame) => {
      if (mainFrame && code !== -3) {
        tab.error =
          code === -400
            ? "This page needs a form submission. Go back to the sign-in page and try again."
            : `Page could not load (${code})`;
        this.publish();
      }
    });
    contents.on("render-process-gone", () => {
      tab.error = "Page process stopped. Reload to retry.";
      this.publish();
    });
    contents.on("destroyed", () => {
      if (!this.window) return;
      const index = this.tabs.indexOf(tab);
      if (index < 0) return;
      if (this.attached === view) {
        this.window.contentView.removeChildView(view);
        this.attached = undefined;
      }
      this.tabs.splice(index, 1);
      if (this.activeId === tab.id) this.activeId = this.tabs[Math.max(0, index - 1)]?.id ?? null;
      if (!this.tabs.length) this.newTab();
      this.publish();
    });
    this.layout();
    this.publish();
    if (destination) void contents.loadURL(destination).catch(() => undefined);
    return contents;
  }

  private layout(): void {
    if (!this.window || this.window.isDestroyed() || !this.chrome) return;
    const children = this.window.contentView.children;
    const shown = children.includes(this.chrome);
    if (this.visible && !shown) this.window.contentView.addChildView(this.chrome);
    if (!this.visible && shown) this.window.contentView.removeChildView(this.chrome);
    // Only trusted studio layout can provide the panel rectangle. Page/chrome content cannot
    // set geometry. Clamp to the native window and keep the page below the host-owned toolbar.
    const [windowWidth = 0, windowHeight = 0] = this.window.getContentSize();
    const zoom = this.window.webContents.getZoomFactor();
    const x = Math.min(windowWidth, Math.round(this.bounds.x * zoom));
    const y = Math.min(windowHeight, Math.round(this.bounds.y * zoom));
    const width = Math.min(windowWidth - x, Math.round(this.bounds.width * zoom));
    const height = Math.min(windowHeight - y, Math.round(this.bounds.height * zoom));
    this.chrome.setBounds({ x, y, width, height });
    const tab = this.active();
    const next =
      this.visible && tab && !this.sitesVisible && tab.view.webContents.getURL()
        ? tab.view
        : undefined;
    if (this.attached !== next) {
      if (this.attached) this.window.contentView.removeChildView(this.attached);
      this.attached = next;
      if (next) this.window.contentView.addChildView(next);
    }
    if (!next || !tab) return;
    tab.view.setBounds({
      x,
      y: y + CHROME_HEIGHT,
      width,
      height: Math.max(0, height - CHROME_HEIGHT - STATUS_HEIGHT),
    });
  }

  private state(): BrowserState {
    return {
      activeId: this.activeId,
      sitesVisible: this.sitesVisible,
      tabs: this.tabs.map((tab) => {
        const contents = tab.view.webContents;
        const url = contents.getURL();
        return {
          id: tab.id,
          title: contents.getTitle() || "Новая вкладка",
          url,
          loading: contents.isLoading(),
          canGoBack: contents.navigationHistory.canGoBack(),
          canGoForward: contents.navigationHistory.canGoForward(),
          error: tab.error,
          security: tab.error
            ? "error"
            : contents.isLoading()
              ? "none"
              : url.startsWith("https:")
                ? "https"
                : url.startsWith("http:")
                  ? "http"
                  : "none",
        };
      }),
    };
  }

  private publish(): void {
    if (!this.window || this.window.isDestroyed()) return;
    this.layout();
    this.chrome?.webContents.send("browser.state", this.state());
  }

  private cookieId(cookie: Cookie): string {
    return JSON.stringify([cookie.domain, cookie.path, cookie.name, cookie.secure]);
  }

  private async cookies(): Promise<BrowserCookie[]> {
    return (await this.profile.cookies.get({})).map((cookie) => ({
      id: this.cookieId(cookie),
      domain: cookie.domain ?? "",
      name: cookie.name,
      path: cookie.path ?? "/",
      secure: cookie.secure === true,
      httpOnly: cookie.httpOnly === true,
      session: cookie.session === true,
      expirationDate: cookie.expirationDate,
    }));
  }

  private async removeCookies(cookies: Cookie[]): Promise<void> {
    for (const cookie of cookies) {
      const url = new URL(
        `${cookie.secure ? "https" : "http"}://${cookie.domain?.replace(/^\./, "")}`,
      );
      url.pathname = cookie.path ?? "/";
      await this.profile.cookies.remove(url.href, cookie.name);
      if (cookie.domain) this.lifecycle?.emit({ type: "cookies-cleared", domain: cookie.domain });
    }
    await this.profile.cookies.flushStore();
  }

  private async command(command: BrowserCommand): Promise<BrowserState | BrowserCookie[]> {
    const tab = this.active();
    switch (command.action) {
      case "state":
        break;
      case "new":
        this.newTab(command.url);
        break;
      case "navigate": {
        if (!tab) throw new Error("No active tab");
        const url = navigationUrl(command.url);
        const linkUrl = this.linkTabs.get(tab.view.webContents);
        if (linkUrl) {
          this.linkTabs.delete(tab.view.webContents);
          this.lifecycle?.emit({ type: "link-tab-closed", url: linkUrl });
        }
        this.sitesVisible = false;
        void tab.view.webContents.loadURL(url).catch(() => undefined);
        break;
      }
      case "switch":
        if (!this.tabs.some((item) => item.id === command.id)) throw new Error("Unknown tab");
        this.activeId = command.id;
        this.sitesVisible = false;
        break;
      case "close": {
        const index = this.tabs.findIndex((item) => item.id === command.id);
        if (index < 0) throw new Error("Unknown tab");
        const removed = this.tabs.splice(index, 1)[0]!;
        if (this.attached === removed.view) {
          this.window?.contentView.removeChildView(removed.view);
          this.attached = undefined;
        }
        removed.view.webContents.close({ waitForBeforeUnload: false });
        if (this.activeId === command.id)
          this.activeId = this.tabs[Math.min(index, this.tabs.length - 1)]?.id ?? null;
        if (!this.tabs.length) this.newTab();
        break;
      }
      case "reorder": {
        const index = this.tabs.findIndex((item) => item.id === command.id);
        if (index < 0 || command.index >= this.tabs.length) throw new Error("Invalid tab order");
        this.tabs.splice(command.index, 0, this.tabs.splice(index, 1)[0]!);
        break;
      }
      case "back":
        if (tab?.view.webContents.navigationHistory.canGoBack())
          tab.view.webContents.navigationHistory.goBack();
        break;
      case "forward":
        if (tab?.view.webContents.navigationHistory.canGoForward())
          tab.view.webContents.navigationHistory.goForward();
        break;
      case "reload":
        tab?.view.webContents.reload();
        break;
      case "sites":
        this.sitesVisible = command.visible;
        break;
      case "cookies":
        return this.cookies();
      case "removeCookie": {
        const cookies = (await this.profile.cookies.get({})).filter(
          (cookie) => this.cookieId(cookie) === command.id,
        );
        await this.removeCookies(cookies);
        return this.cookies();
      }
      case "clearSite": {
        const cookies = (await this.profile.cookies.get({})).filter(
          (cookie) => cookie.domain === command.domain,
        );
        await this.removeCookies(cookies);
        this.lifecycle?.emit({ type: "cookies-cleared", domain: command.domain });
        return this.cookies();
      }
    }
    this.publish();
    return this.state();
  }

  dispose(): void {
    this.lifecycle?.emit({ type: "workspace-closed" });
    this.stopLifecycle?.();
    this.linkTabs.clear();
    ipcMain.removeHandler("browser.mount");
    ipcMain.removeHandler("browser.hide");
    ipcMain.removeHandler("browser.command");
  }
}
