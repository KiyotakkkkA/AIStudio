/// <reference types="vite/client" />
import type { IpcBridge } from "@zvs/ipc";

declare global {
  interface EventBridge {
    subscribe(handler: (event: unknown) => void): () => void;
  }

  interface Window {
    readonly zvs: IpcBridge & EventBridge;
  }

  const __ZVS_VALIDATE_IPC__: boolean;
}

export {};
