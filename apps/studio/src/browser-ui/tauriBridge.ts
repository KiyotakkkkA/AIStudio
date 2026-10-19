import { Channel, invoke } from "@tauri-apps/api/core";

type StateHandler = (state: unknown) => void;
type BrowserBridge = Window["zvs"];

const COMMAND_CHANNEL = "browser.command";

function createTauriBridge(): BrowserBridge {
  const handlers = new Set<StateHandler>();
  let subscribed = false;

  const connect = (): void => {
    if (subscribed) return;
    subscribed = true;
    const channel = new Channel<unknown>((state) => {
      for (const handler of [...handlers]) handler(state);
    });
    invoke("browser_subscribe", { channel }).catch(() => {
      subscribed = false;
    });
  };

  return Object.freeze({
    call: (channel: string, payload: unknown): Promise<unknown> => {
      if (channel !== COMMAND_CHANNEL) return Promise.reject(new Error("Unknown channel"));
      return invoke("browser_command", { command: payload });
    },
    subscribe: (handler: StateHandler): (() => void) => {
      const listener: StateHandler = (state) => handler(state);
      handlers.add(listener);
      connect();
      return () => {
        handlers.delete(listener);
      };
    },
  });
}

Object.defineProperty(window, "zvs", {
  value: createTauriBridge(),
  enumerable: true,
  configurable: false,
  writable: false,
});
