import { HostEvent, type StreamId } from "@zvs/shared";

export type RoutedEvent = HostEvent & { readonly gap?: number };
export type EventHandler = (event: RoutedEvent) => void;
export type EventSource = (handler: (payload: unknown) => void) => () => void;

export interface RouterLogger {
  log(
    level: "debug" | "warn",
    scope: string,
    message: string,
    fields?: Record<string, unknown>,
  ): void;
}

export interface EventRouterOptions {
  buffer?: number;
  logger?: RouterLogger;
}

export interface EventRouter {
  subscribe(streamId: StreamId, handler: EventHandler): () => void;
  observe(handler: EventHandler): () => void;
  dispatch(payload: unknown): void;
  connect(source: EventSource): () => void;
  dispose(): void;
}

const consoleLogger: RouterLogger = {
  log(level, scope, message, fields = {}) {
    const line = `[${scope}] ${message}`;
    if (level === "warn") console.warn(line, fields);
    else console.debug(line, fields);
  },
};

export function createEventRouter(options: EventRouterOptions = {}): EventRouter {
  const logger = options.logger ?? consoleLogger;
  const capacity = options.buffer ?? 256;
  const handlers = new Map<StreamId, Set<EventHandler>>();
  const observers = new Set<EventHandler>();
  const lastSeq = new Map<StreamId, number>();
  let pending: RoutedEvent[] = [];
  let detach: (() => void) | undefined;

  const hold = (event: RoutedEvent): void => {
    pending.push(event);
    while (pending.length > capacity) {
      const dropped = pending.shift();
      logger.log("warn", "events", "Dropped a held event, nothing subscribed in time", {
        streamId: dropped?.streamId,
        seq: dropped?.seq,
      });
    }
  };

  const route = (event: RoutedEvent): void => {
    for (const observer of [...observers]) observer(event);
    const targets = handlers.get(event.streamId);
    if (!targets || targets.size === 0) {
      hold(event);
      return;
    }
    for (const handler of [...targets]) handler(event);
    if (event.type === "end") lastSeq.delete(event.streamId);
  };

  const dispatch = (payload: unknown): void => {
    const parsed = HostEvent.safeParse(payload);
    if (!parsed.success) {
      logger.log("warn", "events", "Discarded an event that does not match the contract", {
        issues: parsed.error.issues.length,
      });
      return;
    }
    const event = parsed.data;
    const previous = lastSeq.get(event.streamId);
    let routed: RoutedEvent = event;
    if (previous !== undefined && event.seq > previous + 1) {
      const gap = event.seq - previous - 1;
      logger.log("warn", "events", "Missed events in a stream", {
        streamId: event.streamId,
        gap,
        expected: previous + 1,
        received: event.seq,
      });
      routed = Object.assign({}, event, { gap });
    } else if (previous !== undefined && event.seq <= previous) {
      logger.log("warn", "events", "Received an out of order event", {
        streamId: event.streamId,
        expected: previous + 1,
        received: event.seq,
      });
    }
    lastSeq.set(event.streamId, previous === undefined ? event.seq : Math.max(previous, event.seq));
    route(routed);
  };

  return {
    dispatch,

    subscribe(streamId, handler) {
      let targets = handlers.get(streamId);
      if (!targets) {
        targets = new Set();
        handlers.set(streamId, targets);
      }
      targets.add(handler);
      const held = pending.filter((event) => event.streamId === streamId);
      if (held.length > 0) {
        pending = pending.filter((event) => event.streamId !== streamId);
        for (const event of held) handler(event);
      }
      let active = true;
      return () => {
        if (!active) return;
        active = false;
        const registered = handlers.get(streamId);
        if (!registered) return;
        registered.delete(handler);
        if (registered.size === 0) handlers.delete(streamId);
      };
    },

    observe(handler) {
      observers.add(handler);
      let active = true;
      return () => {
        if (!active) return;
        active = false;
        observers.delete(handler);
      };
    },

    connect(source) {
      detach?.();
      const stop = source(dispatch);
      detach = stop;
      return () => {
        if (detach === stop) detach = undefined;
        stop();
      };
    },

    dispose() {
      detach?.();
      detach = undefined;
      handlers.clear();
      observers.clear();
      lastSeq.clear();
      pending = [];
    },
  };
}
