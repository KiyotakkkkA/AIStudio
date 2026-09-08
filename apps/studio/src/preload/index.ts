import { contextBridge, ipcRenderer } from "electron";

contextBridge.exposeInMainWorld("zvs", {
  call: (channel: string, payload: unknown): Promise<unknown> =>
    ipcRenderer.invoke(channel, payload),
});
