import { Channel, invoke } from "@tauri-apps/api/core";
import { restoreEnvelope } from "./tauriEnvelope";

type EventHandler = (event: unknown) => void;
type ZvsBridge = Window["zvs"];

function createTauriBridge(): ZvsBridge {
  const handlers = new Set<EventHandler>();
  let subscribed = false;

  const connect = (): void => {
    if (subscribed) return;
    subscribed = true;
    const channel = new Channel<unknown>((event) => {
      for (const handler of [...handlers]) handler(event);
    });
    invoke("events_subscribe", { channel }).catch(() => {
      subscribed = false;
    });
  };

  return Object.freeze({
    call: async (channel: string, payload: unknown): Promise<unknown> =>
      restoreEnvelope(await invoke("ipc_call", { channel, payload })),
    subscribe: (handler: EventHandler): (() => void) => {
      const listener: EventHandler = (event) => handler(event);
      handlers.add(listener);
      connect();
      return () => {
        handlers.delete(listener);
      };
    },
  });
}

if ("__TAURI_INTERNALS__" in window && !("zvs" in window)) {
  Object.defineProperty(window, "zvs", {
    value: createTauriBridge(),
    enumerable: true,
    configurable: false,
    writable: false,
  });
}
