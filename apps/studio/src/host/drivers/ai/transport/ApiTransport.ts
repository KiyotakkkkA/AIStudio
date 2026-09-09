import type { Logger } from "../../../platform/logger.ts";
import { cancelled, httpFailure, networkFailure, timedOut } from "../errors.ts";
import {
  joinUrl,
  transportResponse,
  type RequestSpec,
  type Transport,
  type TransportResponse,
} from "./Transport.ts";

export type FetchLike = (input: string, init: RequestInit) => Promise<Response>;

export const DEFAULT_TIMEOUT_SECONDS = 60;

export interface ApiTransportOptions {
  baseUrl: string;
  apiKey?: string | undefined;
  timeoutSeconds?: number | undefined;
  headers?: Readonly<Record<string, string>>;
  fetch?: FetchLike;
  logger?: Logger;
}

interface ReadChunk {
  readonly done: boolean;
  readonly value?: Uint8Array | undefined;
}

interface Attempt {
  readonly response: Response;
  readonly timeout: AbortSignal;
}

export class ApiTransport implements Transport {
  readonly baseUrl: string;
  readonly #authorization: string | undefined;
  readonly #timeoutSeconds: number;
  readonly #headers: Readonly<Record<string, string>>;
  readonly #fetch: FetchLike;
  readonly #logger: Logger | undefined;

  constructor(options: ApiTransportOptions) {
    const key = options.apiKey?.trim();
    this.baseUrl = options.baseUrl;
    this.#authorization = key === undefined || key.length === 0 ? undefined : `Bearer ${key}`;
    this.#timeoutSeconds = normaliseTimeout(options.timeoutSeconds);
    this.#headers = options.headers ?? {};
    this.#fetch = options.fetch ?? ((input, init) => fetch(input, init));
    this.#logger = options.logger;
  }

  async request(spec: RequestSpec, signal: AbortSignal): Promise<TransportResponse> {
    const attempt = await this.#send(spec, signal);
    const text = await this.#read(attempt, signal);
    const headers = collect(attempt.response.headers);
    if (!attempt.response.ok) {
      throw httpFailure({ status: attempt.response.status, headers, body: text });
    }
    return transportResponse(attempt.response.status, headers, text);
  }

  stream(spec: RequestSpec, signal: AbortSignal): AsyncIterable<Uint8Array> {
    return this.#pump({ accept: "text/event-stream", ...spec }, signal);
  }

  async *#pump(spec: RequestSpec, signal: AbortSignal): AsyncIterable<Uint8Array> {
    const attempt = await this.#send(spec, signal);
    const { response, timeout } = attempt;
    if (!response.ok) {
      throw httpFailure({
        status: response.status,
        headers: collect(response.headers),
        body: await this.#read(attempt, signal),
      });
    }
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

  async #send(spec: RequestSpec, signal: AbortSignal): Promise<Attempt> {
    const url = joinUrl(this.baseUrl, spec.path, spec.query);
    const timeout = AbortSignal.timeout(this.#timeoutSeconds * 1000);
    const combined = AbortSignal.any([signal, timeout]);
    const headers: Record<string, string> = {
      accept: spec.accept ?? "application/json",
      ...this.#headers,
      ...spec.headers,
    };
    if (this.#authorization !== undefined) headers.authorization = this.#authorization;
    if (spec.body !== undefined) headers["content-type"] = "application/json";

    try {
      const response = await this.#fetch(url, {
        method: spec.method,
        headers,
        signal: combined,
        ...(spec.body === undefined ? {} : { body: JSON.stringify(spec.body) }),
      });
      return { response, timeout };
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
      this.#logger?.log("debug", "ai", "Could not read the provider response body", {
        status: attempt.response.status,
      });
      throw networkFailure(error);
    }
  }

  #abortReason(signal: AbortSignal): Error {
    return signal.aborted ? cancelled() : timedOut(this.#timeoutSeconds);
  }
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
