import { contextBridge, ipcRenderer } from "electron";
import type { IpcRendererEvent } from "electron";

contextBridge.exposeInMainWorld("zvs", {
  call: (channel: string, payload: unknown): Promise<unknown> =>
    ipcRenderer.invoke(channel, payload),
  subscribe: (handler: (event: unknown) => void): (() => void) => {
    const listener = (_event: IpcRendererEvent, payload: unknown): void => {
      handler(payload);
    };
    ipcRenderer.on("zvs:events", listener);
    return () => {
      ipcRenderer.removeListener("zvs:events", listener);
    };
  },
});
