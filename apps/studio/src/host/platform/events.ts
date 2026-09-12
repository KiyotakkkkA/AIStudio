import {
  createBrandedId,
  EVENT_CHANNEL,
  StreamId,
  timestampNow,
  type HostEvent,
  type HostEventDraft,
  type RunOutcomeDto,
} from "@zvs/shared";
import { createId } from "./ids.ts";
import type { Logger } from "./logger.ts";

export interface WindowSender {
  isDestroyed(): boolean;
  send(channel: string, payload: unknown): void;
}

export interface EventRecorder {
  append(event: HostEvent): void;
  close(): void;
}

export interface StreamHandle {
  readonly id: StreamId;
  readonly closed: boolean;
  emit(draft: HostEventDraft): void;
  end(outcome: RunOutcomeDto): void;
}

export interface EventBus {
  readonly open: number;
  openStream(options?: {
    id?: StreamId;
    nextSeq?: number;
    record?: (event: HostEvent) => void;
  }): StreamHandle;
  dispose(): void;
}

export interface EventBusOptions {
  logger?: Logger;
  senders?: () => readonly WindowSender[];
  recorder?: EventRecorder;
  newId?: () => string;
  clock?: () => number;
}

const silentLogger: Logger = {
  log() {},
  close() {},
};

export function createEventBus(options: EventBusOptions = {}): EventBus {
  const logger = options.logger ?? silentLogger;
  const senders = options.senders ?? ((): readonly WindowSender[] => []);
  const clock = options.clock ?? Date.now;
  const newId = options.newId ?? createId;
  const live = new Set<StreamId>();
  let disposed = false;

  const deliver = (event: HostEvent): void => {
    try {
      options.recorder?.append(event);
    } catch (error) {
      logger.log("error", "events", "Event recorder failed", { error: String(error) });
    }
    let targets: readonly WindowSender[];
    try {
      targets = senders().filter((sender) => !sender.isDestroyed());
    } catch (error) {
      logger.log("error", "events", "Could not enumerate event recipients", {
        error: String(error),
      });
      return;
    }
    if (targets.length === 0) {
      logger.log("debug", "events", "Dropped an event, no window is open", {
        streamId: event.streamId,
        type: event.type,
        seq: event.seq,
      });
      return;
    }
    for (const target of targets) {
      try {
        target.send(EVENT_CHANNEL, event);
      } catch (error) {
        logger.log("error", "events", "Could not deliver event", {
          streamId: event.streamId,
          error: String(error),
        });
      }
    }
  };

  return {
    get open() {
      return live.size;
    },

    openStream(streamOptions = {}) {
      if (disposed) throw new Error("The event bus is disposed");
      const id = streamOptions.id ?? createBrandedId(StreamId, newId());
      if (live.has(id)) throw new Error("The stream is already open");
      live.add(id);
      let seq = streamOptions.nextSeq ?? 0;
      let closed = false;

      const stamp = (draft: HostEventDraft): HostEvent =>
        ({ ...draft, streamId: id, seq: seq++, ts: timestampNow(clock) }) as HostEvent;

      const refuse = (type: string): void => {
        logger.log("error", "events", "Emitted on a stream that already ended", {
          streamId: id,
          type,
        });
      };

      return {
        id,
        get closed() {
          return closed;
        },
        emit(draft) {
          if (closed) {
            refuse(draft.type);
            return;
          }
          const event = stamp(draft);
          streamOptions.record?.(event);
          deliver(event);
        },
        end(outcome) {
          if (closed) {
            refuse("end");
            return;
          }
          const event = stamp({ type: "end", outcome });
          streamOptions.record?.(event);
          closed = true;
          live.delete(id);
          deliver(event);
        },
      };
    },

    dispose() {
      disposed = true;
      live.clear();
      options.recorder?.close();
    },
  };
}
