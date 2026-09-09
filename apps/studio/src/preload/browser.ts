import { contextBridge, ipcRenderer } from "electron";
import type { IpcRendererEvent } from "electron";

contextBridge.exposeInMainWorld("zvs", {
  call: (channel: string, payload: unknown): Promise<unknown> => {
    if (channel !== "browser.command") return Promise.reject(new Error("Unknown channel"));
    return ipcRenderer.invoke(channel, payload);
  },
  subscribe: (handler: (state: unknown) => void): (() => void) => {
    const listener = (_event: IpcRendererEvent, state: unknown) => handler(state);
    ipcRenderer.on("browser.state", listener);
    return () => ipcRenderer.removeListener("browser.state", listener);
  },
});
