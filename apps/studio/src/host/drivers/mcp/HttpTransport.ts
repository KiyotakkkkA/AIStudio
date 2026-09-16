import { AppError, AppErrorCode } from "@zvs/shared";
import { CLIENT_PROTOCOL_VERSION, type JsonRpcOutgoing } from "./protocol.ts";
import type { McpTransport, McpTransportHandlers } from "./transport.ts";

export type McpFetch = (url: string, init: RequestInit) => Promise<Response>;

export interface HttpTransportOptions {
  url: string;
  /**
   * Already resolved, exactly as in the stdio case: an `Authorization` value that came from a
   * secret reference is plaintext only between the launch and this request.
   */
  headers?: Readonly<Record<string, string>>;
  fetch?: McpFetch;
}

const MAX_BODY_BYTES = 16 * 1024 * 1024;

/**
 * Streamable HTTP: every message is a POST, and the answer arrives either as one JSON body or
 * as an SSE stream the server closes when it is done. v1 authenticates with static headers —
 * an OAuth flow is its own task.
 */
export class HttpTransport implements McpTransport {
  readonly kind = "http" as const;
  #handlers: McpTransportHandlers | undefined;
  #session: string | undefined;
  #protocolVersion: string | undefined;
  #closed = false;

  constructor(private readonly options: HttpTransportOptions) {}

  async open(handlers: McpTransportHandlers): Promise<void> {
    this.#handlers = handlers;
    this.#closed = false;
    await Promise.resolve();
  }

  acceptProtocolVersion(version: string): void {
    this.#protocolVersion = version;
  }

  async send(message: JsonRpcOutgoing): Promise<void> {
    if (this.#closed) throw new AppError(AppErrorCode.MCP_UNAVAILABLE, "MCP-сервер не запущен");
    let response: Response;
    try {
      response = await (this.options.fetch ?? globalThis.fetch)(this.options.url, {
        method: "POST",
        headers: {
          ...this.options.headers,
          "content-type": "application/json",
          accept: "application/json, text/event-stream",
          "mcp-protocol-version": this.#protocolVersion ?? CLIENT_PROTOCOL_VERSION,
          ...(this.#session === undefined ? {} : { "mcp-session-id": this.#session }),
        },
        body: JSON.stringify(message),
      });
    } catch (cause) {
      const reason = `${this.options.url}: ${String(cause)}`;
      this.#handlers?.closed(reason);
      throw new AppError(AppErrorCode.MCP_UNAVAILABLE, `MCP-сервер недоступен: ${reason}`, {
        cause,
      });
    }
    const session = response.headers.get("mcp-session-id");
    if (session) this.#session = session;
    if (response.status === 401 || response.status === 403) {
      throw new AppError(
        AppErrorCode.MCP_UNAVAILABLE,
        `MCP-сервер отклонил учётные данные (${response.status})`,
      );
    }
    if (!response.ok) {
      throw new AppError(
        AppErrorCode.MCP_UNAVAILABLE,
        `MCP-сервер ответил ошибкой ${response.status}`,
      );
    }
    if (response.status === 202) return;
    const type = response.headers.get("content-type") ?? "";
    if (type.includes("text/event-stream")) await this.#readEvents(response);
    else await this.#readJson(response);
  }

  async close(): Promise<void> {
    this.#closed = true;
    const session = this.#session;
    this.#session = undefined;
    const handlers = this.#handlers;
    this.#handlers = undefined;
    if (session !== undefined) {
      try {
        await (this.options.fetch ?? globalThis.fetch)(this.options.url, {
          method: "DELETE",
          headers: { ...this.options.headers, "mcp-session-id": session },
        });
      } catch {
        // A server that will not end its own session is not a reason to fail a shutdown.
      }
    }
    handlers?.closed("stopped");
  }

  async #readJson(response: Response): Promise<void> {
    const body = await response.text();
    if (!body.trim()) return;
    if (body.length > MAX_BODY_BYTES) {
      this.#handlers?.log("error", "Сервер прислал тело сверх допустимого размера");
      return;
    }
    this.#decode(body);
  }

  async #readEvents(response: Response): Promise<void> {
    const body = await response.text();
    for (const frame of body.split(/\r?\n\r?\n/)) {
      const data = frame
        .split(/\r?\n/)
        .filter((line) => line.startsWith("data:"))
        .map((line) => line.slice(5).trim())
        .join("\n");
      if (data) this.#decode(data);
    }
  }

  #decode(payload: string): void {
    try {
      this.#handlers?.message(JSON.parse(payload));
    } catch {
      this.#handlers?.log("error", `Нечитаемый ответ: ${payload.slice(0, 500)}`);
    }
  }
}
