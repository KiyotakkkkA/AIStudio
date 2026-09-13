type RequestMethod = "GET" | "POST" | "DELETE";

export interface RequestSpec {
  readonly method: RequestMethod;
  readonly path: string;
  readonly query?: Readonly<Record<string, string>>;
  readonly headers?: Readonly<Record<string, string>>;
  readonly body?: unknown;
  readonly accept?: string;
}

export interface TransportResponse {
  readonly status: number;
  readonly headers: Readonly<Record<string, string>>;
  readonly text: string;
  json<T>(): T;
}

export interface Transport {
  readonly baseUrl: string;
  request(spec: RequestSpec, signal: AbortSignal): Promise<TransportResponse>;
  stream(spec: RequestSpec, signal: AbortSignal): AsyncIterable<Uint8Array>;
}

export function transportResponse(
  status: number,
  headers: Readonly<Record<string, string>>,
  text: string,
): TransportResponse {
  return {
    status,
    headers,
    text,
    json<T>(): T {
      return JSON.parse(text) as T;
    },
  };
}

export function joinUrl(baseUrl: string, path: string, query?: Readonly<Record<string, string>>) {
  const base = baseUrl.replace(/\/+$/, "");
  const suffix = path.length === 0 ? "" : `/${path.replace(/^\/+/, "")}`;
  const url = new URL(`${base}${suffix}`);
  for (const [key, value] of Object.entries(query ?? {})) url.searchParams.set(key, value);
  return url.toString();
}
