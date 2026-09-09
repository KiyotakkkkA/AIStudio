import type {
  SessionGateway,
  SessionRequest,
} from "../../apps/studio/src/host/drivers/ai/transport/SessionGateway.ts";

export interface ScriptedSessionReply {
  status?: number;
  headers?: Record<string, string>;
  body?: unknown;
  text?: string;
  error?: Error;
}

export interface RecordedSessionRequest {
  readonly url: string;
  readonly method: string;
  readonly headers: Readonly<Record<string, string>>;
  readonly body: string | undefined;
}

export interface FakeSession extends SessionGateway {
  readonly requests: readonly RecordedSessionRequest[];
  readonly authorizations: readonly (string | undefined)[];
  queue(url: string, ...replies: readonly ScriptedSessionReply[]): FakeSession;
  reset(): void;
}

export const FAKE_USER_AGENT = "Mozilla/5.0 (FakeSession) Chrome/144.0.0.0";

export function createFakeSession(partition = "persist:browser-work"): FakeSession {
  const scripted = new Map<string, ScriptedSessionReply[]>();
  const requests: RecordedSessionRequest[] = [];

  const take = (url: string): ScriptedSessionReply => {
    const queued = scripted.get(url);
    if (queued === undefined || queued.length === 0) {
      throw new Error(`No scripted session reply for ${url}`);
    }
    return queued.length === 1 ? queued[0]! : queued.shift()!;
  };

  return {
    partition,
    userAgent: () => FAKE_USER_AGENT,

    get requests() {
      return requests;
    },

    get authorizations() {
      return requests.map((entry) => entry.headers.authorization);
    },

    queue(url, ...replies) {
      scripted.set(url, [...replies]);
      return this;
    },

    reset() {
      requests.length = 0;
      scripted.clear();
    },

    async fetch(url: string, request: SessionRequest): Promise<Response> {
      requests.push({
        url,
        method: request.method,
        headers: lowercase(request.headers),
        body: request.body,
      });
      const reply = take(url);
      if (reply.error !== undefined) throw reply.error;
      if (request.signal.aborted) throw abortError();
      const headers = new Headers({
        "content-type": "application/json",
        ...lowercase(reply.headers ?? {}),
      });
      const body = reply.text ?? (reply.body === undefined ? "" : JSON.stringify(reply.body));
      return await Promise.resolve(new Response(body, { status: reply.status ?? 200, headers }));
    },
  };
}

function lowercase(headers: Readonly<Record<string, string>>): Record<string, string> {
  return Object.fromEntries(
    Object.entries(headers).map(([key, value]) => [key.toLowerCase(), value]),
  );
}

function abortError(): Error {
  const error = new Error("The operation was aborted");
  error.name = "AbortError";
  return error;
}
