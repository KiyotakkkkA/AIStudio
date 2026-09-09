import { httpFailure } from "../../apps/studio/src/host/drivers/ai/errors.ts";
import {
  joinUrl,
  transportResponse,
  type RequestSpec,
  type Transport,
  type TransportResponse,
} from "../../apps/studio/src/host/drivers/ai/transport/Transport.ts";

export interface ScriptedReply {
  status?: number;
  headers?: Record<string, string>;
  body?: unknown;
  text?: string;
  chunks?: readonly string[];
  delayMs?: number;
  error?: Error;
}

export interface RecordedCall {
  readonly method: string;
  readonly path: string;
  readonly url: string;
  readonly body: unknown;
  readonly streamed: boolean;
}

export interface FakeTransport extends Transport {
  readonly calls: readonly RecordedCall[];
  readonly aborts: number;
  reply(path: string, reply: ScriptedReply): FakeTransport;
  lastBody<T>(): T;
  reset(): void;
}

const DEFAULT_BASE_URL = "https://fake.invalid/v1";

export function createFakeTransport(baseUrl = DEFAULT_BASE_URL): FakeTransport {
  const scripted = new Map<string, ScriptedReply>();
  const calls: RecordedCall[] = [];
  let aborts = 0;

  const record = (spec: RequestSpec, streamed: boolean): void => {
    calls.push({
      method: spec.method,
      path: spec.path,
      url: joinUrl(baseUrl, spec.path, spec.query),
      body: spec.body,
      streamed,
    });
  };

  const scriptFor = (spec: RequestSpec): ScriptedReply => {
    const reply = scripted.get(spec.path);
    if (reply === undefined) throw new Error(`No scripted reply for ${spec.method} ${spec.path}`);
    return reply;
  };

  const bodyText = (reply: ScriptedReply): string => {
    if (reply.text !== undefined) return reply.text;
    return reply.body === undefined ? "" : JSON.stringify(reply.body);
  };

  const wait = async (ms: number, signal: AbortSignal): Promise<void> => {
    if (ms <= 0) return;
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => {
        signal.removeEventListener("abort", onAbort);
        resolve();
      }, ms);
      const onAbort = (): void => {
        clearTimeout(timer);
        aborts += 1;
        reject(abortError());
      };
      if (signal.aborted) {
        onAbort();
        return;
      }
      signal.addEventListener("abort", onAbort, { once: true });
    });
  };

  return {
    baseUrl,

    get calls() {
      return calls;
    },

    get aborts() {
      return aborts;
    },

    reply(path, reply) {
      scripted.set(path, reply);
      return this;
    },

    lastBody<T>(): T {
      const call = calls.at(-1);
      if (call === undefined) throw new Error("No call has been recorded");
      return call.body as T;
    },

    reset() {
      calls.length = 0;
      aborts = 0;
      scripted.clear();
    },

    async request(spec: RequestSpec, signal: AbortSignal): Promise<TransportResponse> {
      record(spec, false);
      const reply = scriptFor(spec);
      await wait(reply.delayMs ?? 0, signal);
      if (signal.aborted) {
        aborts += 1;
        throw abortError();
      }
      if (reply.error !== undefined) throw reply.error;
      const status = reply.status ?? 200;
      const headers = lowercase(reply.headers ?? {});
      const text = bodyText(reply);
      if (status < 200 || status >= 300) throw httpFailure({ status, headers, body: text });
      return transportResponse(status, headers, text);
    },

    stream(spec: RequestSpec, signal: AbortSignal): AsyncIterable<Uint8Array> {
      record(spec, true);
      const reply = scriptFor(spec);
      const encoder = new TextEncoder();
      return {
        async *[Symbol.asyncIterator](): AsyncIterator<Uint8Array> {
          const status = reply.status ?? 200;
          if (status < 200 || status >= 300) {
            throw httpFailure({
              status,
              headers: lowercase(reply.headers ?? {}),
              body: bodyText(reply),
            });
          }
          if (reply.error !== undefined) throw reply.error;
          for (const chunk of reply.chunks ?? []) {
            await wait(reply.delayMs ?? 0, signal);
            if (signal.aborted) {
              aborts += 1;
              throw abortError();
            }
            yield encoder.encode(chunk);
          }
        },
      };
    },
  };
}

export function sseFrames(deltas: readonly string[], finishReason = "stop"): string[] {
  const frames = deltas.map(
    (delta) =>
      `data: ${JSON.stringify({ choices: [{ index: 0, delta: { content: delta } }] })}\n\n`,
  );
  frames.push(
    `data: ${JSON.stringify({ choices: [{ index: 0, delta: {}, finish_reason: finishReason }] })}\n\n`,
  );
  frames.push("data: [DONE]\n\n");
  return frames;
}

function lowercase(headers: Record<string, string>): Record<string, string> {
  return Object.fromEntries(
    Object.entries(headers).map(([key, value]) => [key.toLowerCase(), value]),
  );
}

function abortError(): Error {
  const error = new Error("The operation was aborted");
  error.name = "AbortError";
  return error;
}
