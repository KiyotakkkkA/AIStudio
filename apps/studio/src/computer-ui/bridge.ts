import { Channel, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { ComputerCommand, ComputerState } from "@zvs/shared";

export const GLOW_LABEL = "computer-glow";

export function windowLabel(): string {
  return getCurrentWindow().label;
}

export function sendCommand(command: ComputerCommand): Promise<void> {
  return invoke("computer_command", { command });
}

export function subscribeState(handler: (state: ComputerState) => void): void {
  const channel = new Channel<ComputerState>(handler);
  void invoke("computer_subscribe", { channel });
}
