import { z } from "zod";
import { AppError, AppErrorCode } from "@zvs/shared";
import {
  CallToolResult,
  CLIENT_PROTOCOL_VERSION,
  InitializeResult,
  JsonRpcResponse,
  ListPromptsResult,
  ListResourcesResult,
  ListToolsResult,
  SUPPORTED_PROTOCOL_VERSIONS,
  type JsonRpcOutgoing,
  type McpPromptDescriptor,
  type McpResourceDescriptor,
  type McpToolDescriptor,
} from "./protocol.ts";
import type { McpLogLevel, McpTransport, McpTransportHandlers } from "./transport.ts";

export interface McpClientOptions {
  transport: McpTransport;
  clientName?: string;
  clientVersion?: string;
  requestTimeoutMs?: number;
  /** A line for the health rail. The client never passes a payload here. */
  onLog?: (level: McpLogLevel, line: string) => void;
  /** Host-side only: the raw text of something that did not parse. */
  onMalformed?: (payload: string) => void;
  onClosed?: (reason: string) => void;
  clock?: () => number;
}

export interface McpHandshake {
  protocolVersion: string;
  serverName: string;
  serverVersion: string;
  capabilities: Readonly<Record<string, unknown>>;
  latencyMs: number;
}

export interface McpDiscovery {
  tools: McpToolDescriptor[];
  resources: McpResourceDescriptor[];
  prompts: McpPromptDescriptor[];
}

interface Pending {
  method: string;
  resolve(value: unknown): void;
  reject(error: unknown): void;
  timer: ReturnType<typeof setTimeout>;
}

const MAX_PAGES = 50;

/** One connection's worth of protocol: handshake, discovery, invocation, and nothing else. */
export class McpClient {
  readonly #pending = new Map<string, Pending>();
  #nextId = 1;
  #connected = false;
  #closedReason: string | undefined;

  constructor(private readonly options: McpClientOptions) {}

  get connected(): boolean {
    return this.#connected;
  }

  get timeoutMs(): number {
    return this.options.requestTimeoutMs ?? 30_000;
  }

  async connect(): Promise<McpHandshake> {
    const handlers: McpTransportHandlers = {
      message: (value) => this.#receive(value),
      log: (level, line) => this.options.onLog?.(level, line),
      closed: (reason) => this.#abort(reason),
    };
    this.#closedReason = undefined;
    await this.options.transport.open(handlers);
    const startedAt = this.#now();
    const raw = await this.request("initialize", {
      protocolVersion: CLIENT_PROTOCOL_VERSION,
      capabilities: {},
      clientInfo: {
        name: this.options.clientName ?? "zvs-ai-studio",
        version: this.options.clientVersion ?? "0.0.0",
      },
    });
    const result = this.#parse(InitializeResult, raw, "initialize");
    if (!SUPPORTED_PROTOCOL_VERSIONS.includes(result.protocolVersion)) {
      await this.close();
      throw new AppError(
        AppErrorCode.MCP_UNAVAILABLE,
        `Сервер говорит на неподдерживаемой версии протокола: ${result.protocolVersion}`,
        { details: { protocolVersion: result.protocolVersion } },
      );
    }
    this.options.transport.acceptProtocolVersion?.(result.protocolVersion);
    this.#connected = true;
    await this.notify("notifications/initialized");
    return {
      protocolVersion: result.protocolVersion,
      serverName: result.serverInfo.name,
      serverVersion: result.serverInfo.version,
      capabilities: result.capabilities,
      latencyMs: Math.max(0, Math.round(this.#now() - startedAt)),
    };
  }

  /**
   * Everything the server exposes. A server that never announced a capability is answering
   * honestly about something it lacks, so that list is simply empty.
   */
  async discover(capabilities: Readonly<Record<string, unknown>> = {}): Promise<McpDiscovery> {
    const has = (name: string): boolean =>
      Object.keys(capabilities).length === 0 || capabilities[name] !== undefined;
    return {
      tools: has("tools")
        ? await this.#page("tools/list", ListToolsResult, (page) => page.tools)
        : [],
      resources: has("resources")
        ? await this.#page("resources/list", ListResourcesResult, (page) => page.resources)
        : [],
      prompts: has("prompts")
        ? await this.#page("prompts/list", ListPromptsResult, (page) => page.prompts)
        : [],
    };
  }

  async callTool(
    name: string,
    args: Readonly<Record<string, unknown>>,
    timeoutMs?: number,
  ): Promise<CallToolResult> {
    const raw = await this.request("tools/call", { name, arguments: args }, timeoutMs);
    return this.#parse(CallToolResult, raw, "tools/call");
  }

  async request(
    method: string,
    params?: Record<string, unknown>,
    timeoutMs = this.timeoutMs,
  ): Promise<unknown> {
    if (this.#closedReason !== undefined)
      throw new AppError(AppErrorCode.MCP_UNAVAILABLE, this.#closedReason);
    const id = String(this.#nextId++);
    const answer = new Promise<unknown>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.#pending.delete(id);
        // Tell the server to stop; a hung call must not hold the run or the process.
        void this.notify("notifications/cancelled", { requestId: id, reason: "timeout" }).catch(
          () => undefined,
        );
        reject(
          new AppError(
            AppErrorCode.MCP_TIMEOUT,
            `MCP-сервер не ответил на ${method} за ${timeoutMs} мс`,
            { details: { method, timeoutMs } },
          ),
        );
      }, timeoutMs);
      timer.unref();
      this.#pending.set(id, { method, resolve, reject, timer });
    });
    try {
      await this.#send({ jsonrpc: "2.0", id, method, ...(params === undefined ? {} : { params }) });
    } catch (error) {
      // The caller gets the send failure itself; the pending promise is settled and disowned so
      // a transport that refuses a write does not surface as an unhandled rejection.
      void answer.catch(() => undefined);
      this.#settle(id, (pending) => {
        pending.reject(error);
      });
      throw error;
    }
    return answer;
  }

  async notify(method: string, params?: Record<string, unknown>): Promise<void> {
    await this.#send({ jsonrpc: "2.0", method, ...(params === undefined ? {} : { params }) });
  }

  async close(): Promise<void> {
    this.#connected = false;
    try {
      await this.options.transport.close();
    } finally {
      this.#abort("stopped");
    }
  }

  async #send(message: JsonRpcOutgoing): Promise<void> {
    await this.options.transport.send(message);
  }

  async #page<S extends z.ZodType<{ nextCursor?: string | null | undefined }>, T>(
    method: string,
    schema: S,
    pick: (page: z.infer<S>) => T[],
  ): Promise<T[]> {
    const items: T[] = [];
    let cursor: string | undefined;
    for (let page = 0; page < MAX_PAGES; page++) {
      const raw = await this.request(method, cursor === undefined ? {} : { cursor });
      const parsed = this.#parse(schema, raw, method);
      items.push(...pick(parsed));
      const next = parsed.nextCursor;
      if (next === undefined || next === null || next === "") return items;
      cursor = next;
    }
    this.options.onLog?.("warn", `${method}: список оборван на ${MAX_PAGES} страницах`);
    return items;
  }

  #receive(value: unknown): void {
    const response = JsonRpcResponse.safeParse(value);
    if (!response.success) {
      // A notification or a server-initiated request: nothing in v1 acts on either.
      if (value !== null && typeof value === "object" && "method" in value) return;
      this.#report(value);
      return;
    }
    const id = String(response.data.id);
    if (!this.#pending.has(id)) {
      this.options.onLog?.("debug", `Ответ на неизвестный запрос ${id}`);
      return;
    }
    const failure = response.data.error;
    const result = response.data.result ?? null;
    this.#settle(id, (entry) => {
      if (failure === undefined) entry.resolve(result);
      else
        entry.reject(
          new AppError(
            AppErrorCode.UNKNOWN,
            failure.message.trim() || `MCP-сервер отклонил ${entry.method}`,
            { details: { method: entry.method, code: failure.code } },
          ),
        );
    });
  }

  #parse<S extends z.ZodType>(schema: S, raw: unknown, method: string): z.infer<S> {
    const parsed = schema.safeParse(raw);
    if (parsed.success) return parsed.data;
    this.#report(raw);
    throw new AppError(AppErrorCode.UNKNOWN, `MCP-сервер ответил неожиданным телом на ${method}`, {
      details: { method },
    });
  }

  /** The payload stays host-side: it may quote whatever the server was working on. */
  #report(value: unknown): void {
    let payload: string;
    try {
      payload = JSON.stringify(value) ?? String(value);
    } catch {
      payload = String(value);
    }
    this.options.onMalformed?.(payload.slice(0, 4000));
    this.options.onLog?.("error", "Сервер прислал сообщение, не отвечающее протоколу");
  }

  #settle(id: string, decide: (pending: Pending) => void): void {
    const pending = this.#pending.get(id);
    if (pending === undefined) return;
    this.#pending.delete(id);
    clearTimeout(pending.timer);
    decide(pending);
  }

  #abort(reason: string): void {
    const wasOpen = this.#connected || this.#closedReason === undefined;
    this.#connected = false;
    this.#closedReason = reason;
    for (const id of [...this.#pending.keys()])
      this.#settle(id, (pending) => {
        pending.reject(
          new AppError(AppErrorCode.MCP_UNAVAILABLE, `MCP-сервер прервал ${pending.method}`, {
            details: { reason },
          }),
        );
      });
    if (wasOpen) this.options.onClosed?.(reason);
  }

  #now(): number {
    return (this.options.clock ?? Date.now)();
  }
}
