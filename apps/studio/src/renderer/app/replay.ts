import type { EventRouter } from "./EventRouter";

export interface ReplayOptions {
  delayMs?: number;
  wait?: (ms: number) => Promise<void>;
}

export function parseRecording(text: string): unknown[] {
  const events: unknown[] = [];
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.length > 0) events.push(JSON.parse(trimmed));
  }
  return events;
}

export async function replayRecording(
  router: EventRouter,
  text: string,
  options: ReplayOptions = {},
): Promise<number> {
  const events = parseRecording(text);
  const delayMs = options.delayMs ?? 0;
  const wait =
    options.wait ?? ((ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms)));
  for (const event of events) {
    router.dispatch(event);
    if (delayMs > 0) await wait(delayMs);
  }
  return events.length;
}

export interface ReplayHook {
  fromText(text: string, options?: ReplayOptions): Promise<number>;
  fromUrl(url: string, options?: ReplayOptions): Promise<number>;
}

export function createReplayHook(router: EventRouter): ReplayHook {
  return {
    fromText: (text, options) => replayRecording(router, text, options),
    async fromUrl(url, options) {
      const response = await fetch(url);
      if (!response.ok) throw new Error(`Запись недоступна: ${response.status}`);
      return await replayRecording(router, await response.text(), options);
    },
  };
}
