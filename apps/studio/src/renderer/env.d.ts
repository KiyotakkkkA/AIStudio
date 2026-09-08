/// <reference types="vite/client" />
import type { IpcBridge } from "@zvs/ipc";

declare global {
  interface Window {
    readonly zvs: IpcBridge;
  }

  const __ZVS_VALIDATE_IPC__: boolean;
}

export {};
