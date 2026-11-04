import { Channel, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { ComputerCommand, ComputerPointer, ComputerState } from "@zvs/shared";

export const GLOW_LABEL = "computer-glow";
const RETRY_MS = 500;
const RETRIES_MAX = 20;

export function windowLabel(): string {
  return getCurrentWindow().label;
}

export function sendCommand(command: ComputerCommand): Promise<void> {
  return invoke("computer_command", { command });
}

function follow<T>(command: string, handler: (message: T) => void): () => void {
  let active = true;
  const connect = (attempt: number) => {
    const channel = new Channel<T>((message) => {
      if (active) handler(message);
    });
    invoke(command, { channel }).catch((error: unknown) => {
      console.error(`[computer-ui] ${command} failed`, error);
      if (active && attempt < RETRIES_MAX) {
        window.setTimeout(() => connect(attempt + 1), RETRY_MS);
      }
    });
  };
  connect(0);
  return () => {
    active = false;
  };
}

export function subscribeState(handler: (state: ComputerState) => void): () => void {
  return follow("computer_subscribe", handler);
}

export function subscribePointer(handler: (pointer: ComputerPointer) => void): () => void {
  return follow("computer_pointer", handler);
}
