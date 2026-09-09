import { AppError, AppErrorCode, err, isAppError, ok } from "@zvs/shared";
import type { ChannelName, ContractShape, InputOf, OutputOf } from "@zvs/shared";
import type { IpcBridge } from "@zvs/ipc";

export interface BridgeCall {
  readonly channel: string;
  readonly payload: unknown;
}

export type FakeHandler<C extends ContractShape, K extends ChannelName<C>> = (
  input: InputOf<C, K>,
) => OutputOf<C, K> | Promise<OutputOf<C, K>>;

/**
 * An in-memory stand-in for the preload bridge (`window.zvs`). It speaks the same
 * contract the real one does — `call` resolves a `Result` envelope, `subscribe` hands
 * back an unsubscribe function — so renderer code can be tested without Electron.
 */
export interface FakeBridge<C extends ContractShape> extends IpcBridge {
  subscribe(handler: (event: unknown) => void): () => void;
  /** Registers the response for one channel. Registering again replaces the previous one. */
  handle<K extends ChannelName<C>>(channel: K, handler: FakeHandler<C, K>): FakeBridge<C>;
  /** Registers a channel that always fails, so error paths can be exercised. */
  fail(channel: ChannelName<C>, code: AppErrorCode, message: string): FakeBridge<C>;
  /** Pushes one event to every current subscriber, the way the host does. */
  emit(event: unknown): void;
  /** Every call made through the bridge, in order. */
  readonly calls: readonly BridgeCall[];
  readonly subscribers: number;
}

export function createFakeBridge<C extends ContractShape>(): FakeBridge<C> {
  const handlers = new Map<string, (input: never) => unknown>();
  const listeners = new Set<(event: unknown) => void>();
  const calls: BridgeCall[] = [];

  const bridge: FakeBridge<C> = {
    calls,

    get subscribers(): number {
      return listeners.size;
    },

    async call(channel: string, payload: unknown): Promise<unknown> {
      calls.push({ channel, payload });
      const handler = handlers.get(channel);
      if (!handler) {
        return err(AppErrorCode.NOT_FOUND, `Канал ${channel} не зарегистрирован`);
      }
      try {
        return ok(await (handler as (input: unknown) => unknown)(payload));
      } catch (error: unknown) {
        if (isAppError(error)) return err(error.code, error.message, error.details);
        throw error;
      }
    },

    subscribe(handler: (event: unknown) => void): () => void {
      listeners.add(handler);
      return () => {
        listeners.delete(handler);
      };
    },

    handle(channel, handler) {
      handlers.set(channel, handler as (input: never) => unknown);
      return bridge;
    },

    fail(channel, code, message) {
      handlers.set(channel, () => {
        throw new AppError(code, message);
      });
      return bridge;
    },

    emit(event: unknown): void {
      for (const listener of [...listeners]) listener(event);
    },
  };

  return bridge;
}
