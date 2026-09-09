import type { Logger } from "../../../platform/logger.ts";
import {
  cancelled,
  httpFailure,
  isSignedOutResponse,
  networkFailure,
  sessionExpired,
  timedOut,
} from "../errors.ts";
import { DEFAULT_TIMEOUT_SECONDS } from "./ApiTransport.ts";
import type { SessionGateway } from "./SessionGateway.ts";
import {
  joinUrl,
  transportResponse,
  type RequestSpec,
  type Transport,
  type TransportResponse,
} from "./Transport.ts";

export const DEFAULT_TOKEN_TYPE = "Bearer";

export interface AccountToken {
  readonly token: string;
  readonly tokenType: string;
}

export interface AccountCredentials {
  current(signal: AbortSignal): Promise<AccountToken | null>;
  refresh(signal: AbortSignal): Promise<AccountToken | null>;
}

export interface AccountTransportOptions {
  baseUrl: string;
  session: SessionGateway;
  credentials?: AccountCredentials | undefined;
  timeoutSeconds?: number | undefined;
  headers?: Readonly<Record<string, string>>;
  logger?: Logger;
}

interface ReadChunk {
  readonly done: boolean;
  readonly value?: Uint8Array | undefined;
}

interface Attempt {
  readonly response: Response;
  readonly timeout: AbortSignal;
  readonly authorised: boolean;
}

export class AccountTransport implements Transport {
  readonly baseUrl: string;
  readonly #session: SessionGateway;
  readonly #credentials: AccountCredentials | undefined;
  readonly #timeoutSeconds: number;
  readonly #headers: Readonly<Record<string, string>>;
  readonly #origin: string;
  readonly #logger: Logger | undefined;

  constructor(options: AccountTransportOptions) {
    this.baseUrl = options.baseUrl;
    this.#session = options.session;
    this.#credentials = options.credentials;
    this.#timeoutSeconds = normaliseTimeout(options.timeoutSeconds);
    this.#headers = options.headers ?? {};
    this.#origin = new URL(options.baseUrl).origin;
    this.#logger = options.logger;
  }

  async request(spec: RequestSpec, signal: AbortSignal): Promise<TransportResponse> {
    const attempt = await this.#attempt(spec, signal);
    const headers = collect(attempt.response.headers);
    const text = await this.#read(attempt, signal);
    this.#guard(attempt.response.status, headers);
    if (!attempt.response.ok) {
      throw httpFailure({
        status: attempt.response.status,
        headers,
        body: text,
        authMode: "account",
      });
    }
    return transportResponse(attempt.response.status, headers, text);
  }

  stream(spec: RequestSpec, signal: AbortSignal): AsyncIterable<Uint8Array> {
    return this.#pump({ accept: "text/event-stream", ...spec }, signal);
  }

  async *#pump(spec: RequestSpec, signal: AbortSignal): AsyncIterable<Uint8Array> {
    const attempt = await this.#attempt(spec, signal);
    const { response, timeout } = attempt;
    const headers = collect(response.headers);
    if (!response.ok) {
      const body = await this.#read(attempt, signal);
      this.#guard(response.status, headers);
      throw httpFailure({ status: response.status, headers, body, authMode: "account" });
    }
    this.#guard(response.status, headers);
    if (response.body === null) return;
    const reader = response.body.getReader();
    try {
      for (;;) {
        const chunk = await this.#next(reader, signal, timeout);
        if (chunk.done) return;
        if (chunk.value !== undefined) yield chunk.value;
      }
    } finally {
      void reader.cancel().catch(() => undefined);
    }
  }

  async #attempt(spec: RequestSpec, signal: AbortSignal): Promise<Attempt> {
    const first = await this.#send(spec, signal, await this.#token(signal, "current"));
    if (!first.authorised || !isRejected(first.response.status)) return first;
    const refreshed = await this.#token(signal, "refresh");
    if (refreshed === null) throw sessionExpired({ status: first.response.status });
    return await this.#send(spec, signal, refreshed);
  }

  async #token(signal: AbortSignal, mode: "current" | "refresh"): Promise<AccountToken | null> {
    if (this.#credentials === undefined) return null;
    return mode === "current"
      ? await this.#credentials.current(signal)
      : await this.#credentials.refresh(signal);
  }

  async #next(
    reader: ReadableStreamDefaultReader<Uint8Array>,
    signal: AbortSignal,
    timeout: AbortSignal,
  ): Promise<ReadChunk> {
    const combined = AbortSignal.any([signal, timeout]);
    if (combined.aborted) throw this.#abortReason(signal);
    return await new Promise<ReadChunk>((resolve, reject) => {
      const onAbort = (): void => reject(this.#abortReason(signal));
      combined.addEventListener("abort", onAbort, { once: true });
      reader.read().then(
        (chunk) => {
          combined.removeEventListener("abort", onAbort);
          resolve(chunk);
        },
        (error: unknown) => {
          combined.removeEventListener("abort", onAbort);
          reject(signal.aborted ? cancelled() : networkFailure(error));
        },
      );
    });
  }

  async #send(
    spec: RequestSpec,
    signal: AbortSignal,
    token: AccountToken | null,
  ): Promise<Attempt> {
    const url = joinUrl(this.baseUrl, spec.path, spec.query);
    const timeout = AbortSignal.timeout(this.#timeoutSeconds * 1000);
    const combined = AbortSignal.any([signal, timeout]);
    const headers: Record<string, string> = {
      accept: spec.accept ?? "application/json",
      "user-agent": this.#session.userAgent(),
      origin: this.#origin,
      referer: `${this.#origin}/`,
      ...this.#headers,
      ...spec.headers,
    };
    if (token !== null) headers.authorization = `${token.tokenType} ${token.token}`;
    if (spec.body !== undefined) headers["content-type"] = "application/json";

    try {
      const response = await this.#session.fetch(url, {
        method: spec.method,
        headers,
        signal: combined,
        ...(spec.body === undefined ? {} : { body: JSON.stringify(spec.body) }),
      });
      return { response, timeout, authorised: token !== null };
    } catch (error: unknown) {
      if (combined.aborted) throw this.#abortReason(signal);
      throw networkFailure(error);
    }
  }

  async #read(attempt: Attempt, signal: AbortSignal): Promise<string> {
    try {
      return await attempt.response.text();
    } catch (error: unknown) {
      if (signal.aborted || attempt.timeout.aborted) throw this.#abortReason(signal);
      this.#logger?.log("debug", "ai", "Could not read the account response body", {
        status: attempt.response.status,
      });
      throw networkFailure(error);
    }
  }

  #guard(status: number, headers: Readonly<Record<string, string>>): void {
    if (!isSignedOutResponse(status, headers)) return;
    throw sessionExpired({ status, partition: this.#session.partition });
  }

  #abortReason(signal: AbortSignal): Error {
    return signal.aborted ? cancelled() : timedOut(this.#timeoutSeconds);
  }
}

function isRejected(status: number): boolean {
  return status === 401 || status === 403;
}

function normaliseTimeout(seconds: number | undefined): number {
  if (seconds === undefined || !Number.isFinite(seconds) || seconds <= 0) {
    return DEFAULT_TIMEOUT_SECONDS;
  }
  return seconds;
}

function collect(headers: Headers): Record<string, string> {
  const result: Record<string, string> = {};
  headers.forEach((value, key) => {
    result[key.toLowerCase()] = value;
  });
  return result;
}
