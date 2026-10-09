import {
  AppError,
  AppErrorCode,
  CreateMcpServerInput,
  McpCapabilityDto,
  McpCallResultDto,
  McpHealthDto,
  McpNamespace,
  McpServerDetailDto,
  McpServerDto,
  McpTestResultDto,
  timestampNow,
  UpdateMcpServerInput,
  type CreateMcpServerRequest,
  type Json,
  type McpCapabilityKind,
  type McpLogLineDto,
  type McpServerStatus,
  type McpValue,
} from "@zvs/shared";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import type { McpServerPatch } from "../data/repositories/index.ts";
import type { McpCapabilityEntity, McpServerEntity } from "../data/schema/index.ts";
import {
  HttpTransport,
  isolatedToolchainEnv,
  inferDestructive,
  inferReadOnly,
  McpClient,
  StdioTransport,
  type CallToolResult,
  type McpDiscovery,
  type McpFetch,
  type McpLogLevel,
  type McpTransport,
  type SpawnServer,
} from "../drivers/mcp/index.ts";
import { registerMcpToolNode, type McpNodeTarget } from "../kernel/mcpNodes.ts";
import type { NodeRegistry } from "../kernel/NodeRegistry.ts";
import type { Logger } from "../platform/logger.ts";
import { createId } from "../platform/ids.ts";
import type { SecretService } from "./SecretService.ts";
import type { ToolRegistry } from "../tools/ToolRegistry.ts";

export interface McpServiceOptions {
  data: UnitOfWork;
  tools: ToolRegistry;
  nodes: NodeRegistry;
  secrets: Pick<SecretService, "resolve" | "addUsage" | "removeUsage">;
  logger?: Logger;
  clock?: () => number;
  /** Injected so a test drives a fake server without a real process or socket. */
  spawn?: SpawnServer;
  fetch?: McpFetch;
  transport?: (server: McpServerEntity, env: Record<string, string>) => McpTransport;
  requestTimeoutMs?: number;
  callTimeoutMs?: number;
  backoffMs?: number;
  maxBackoffMs?: number;
  maxRestarts?: number;
  clientVersion?: string;
  toolchainDir?: string;
}

interface Supervisor {
  client?: McpClient;
  starting?: Promise<void>;
  startedAt?: number;
  restarts: number;
  failures: number;
  calls: { at: number; ms: number; failed: boolean }[];
  log: McpLogLineDto[];
  retry?: ReturnType<typeof setTimeout>;
  /** Set when a stop was asked for, so an exit is not treated as a death to recover from. */
  intentional: boolean;
}

const DAY_MS = 24 * 60 * 60 * 1000;
const LOG_LINES = 50;
const CALL_SAMPLES = 500;

export class McpService {
  readonly #supervisors = new Map<string, Supervisor>();
  #closed = false;

  constructor(private readonly options: McpServiceOptions) {}

  // ── configuration ───────────────────────────────────────────────────────────

  list(): McpServerDto[] {
    const rows = this.#repository.list();
    const counts = this.#repository.countsForServers(rows.map((row) => row.id));
    return rows.map((row) =>
      this.#toDto(row, counts.get(row.id) ?? { tool: 0, resource: 0, prompt: 0 }),
    );
  }

  get(id: string): McpServerDetailDto {
    const row = this.#require(id);
    return McpServerDetailDto.parse({
      ...this.#toDto(row, this.#repository.capabilityCounts(row.id)),
      capabilities: this.capabilities(row.id),
    });
  }

  capabilities(id: string, kind?: McpCapabilityKind): McpCapabilityDto[] {
    const rows =
      kind === undefined
        ? this.#repository.capabilities(this.#require(id).id)
        : this.#repository.capabilitiesOfKind(this.#require(id).id, kind);
    return rows.map((row) => toCapabilityDto(row));
  }

  create(input: CreateMcpServerRequest): McpServerDto {
    const draft = CreateMcpServerInput.parse(input);
    this.#validateShape(draft.transport, draft.command, draft.url);
    const now = timestampNow(this.#clock);
    const id = createId();
    const namespace = this.#uniqueNamespace(draft.namespace ?? deriveNamespace(draft.name), null);
    const row = this.#repository.insert({
      id,
      name: draft.name,
      namespace,
      transport: draft.transport,
      command: draft.command,
      args: [...draft.args],
      env: draft.env,
      url: draft.url,
      headers: draft.headers,
      enabled: draft.enabled,
      startup: draft.startup,
      status: draft.enabled ? "stopped" : "disabled",
      protocolVersion: null,
      serverName: null,
      serverVersion: null,
      lastError: null,
      lastConnectedAt: null,
      createdAt: now,
      updatedAt: now,
    });
    this.#syncSecretUsage(row.id, [], secretIds(row));
    this.#log("info", "Added an MCP server", { serverId: row.id, transport: row.transport });
    return this.#toDto(row, { tool: 0, resource: 0, prompt: 0 });
  }

  /** Changing what a server *is* stops it: the running process no longer matches the config. */
  async update(input: UpdateMcpServerInput): Promise<McpServerDto> {
    const patch = UpdateMcpServerInput.parse(input);
    const existing = this.#require(patch.id);
    const transport = patch.transport ?? existing.transport;
    const command = patch.command === undefined ? existing.command : patch.command;
    const url = patch.url === undefined ? existing.url : patch.url;
    this.#validateShape(transport, command, url);
    const next: McpServerPatch = { updatedAt: timestampNow(this.#clock) };
    if (patch.name !== undefined) next.name = patch.name;
    if (patch.namespace !== undefined)
      next.namespace = this.#uniqueNamespace(patch.namespace, existing.id);
    if (patch.transport !== undefined) next.transport = patch.transport;
    if (patch.command !== undefined) next.command = patch.command;
    if (patch.args !== undefined) next.args = [...patch.args];
    if (patch.env !== undefined) next.env = patch.env;
    if (patch.url !== undefined) next.url = patch.url;
    if (patch.headers !== undefined) next.headers = patch.headers;
    if (patch.startup !== undefined) next.startup = patch.startup;
    if (patch.enabled !== undefined) next.enabled = patch.enabled;
    await this.stop(existing.id);
    const row = this.#repository.update(existing.id, {
      ...next,
      status: (patch.enabled ?? existing.enabled) ? "stopped" : "disabled",
      lastError: null,
    });
    this.#syncSecretUsage(row.id, secretIds(existing), secretIds(row));
    if (!row.enabled) this.#withdraw(row);
    return this.#toDto(row, this.#repository.capabilityCounts(row.id));
  }

  async remove(id: string): Promise<void> {
    const row = this.#require(id);
    await this.stop(row.id);
    this.#withdraw(row);
    this.#syncSecretUsage(row.id, secretIds(row), []);
    this.#repository.remove(row.id);
    this.#supervisors.delete(row.id);
    this.#log("info", "Removed an MCP server", { serverId: row.id });
  }

  // ── lifecycle ───────────────────────────────────────────────────────────────

  /** Brings up everything configured to start with the app. One bad server never blocks boot. */
  async startAutoStart(): Promise<void> {
    await Promise.all(
      this.#repository.listAutoStart().map(async (row) => {
        try {
          await this.start(row.id);
        } catch (error: unknown) {
          this.#log("warn", "An MCP server configured for launch did not start", {
            serverId: row.id,
            raw: String(error),
          });
        }
      }),
    );
  }

  async start(id: string): Promise<void> {
    const row = this.#require(id);
    if (!row.enabled)
      throw new AppError(AppErrorCode.CONFLICT, "MCP-сервер выключен", {
        details: { serverId: row.id },
      });
    const supervisor = this.#supervisor(row.id);
    supervisor.intentional = false;
    clearTimeout(supervisor.retry);
    supervisor.retry = undefined;
    if (supervisor.client?.connected === true) return;
    supervisor.starting ??= this.#launch(row.id).finally(() => {
      this.#supervisor(row.id).starting = undefined;
    });
    await supervisor.starting;
  }

  async stop(id: string): Promise<void> {
    const supervisor = this.#supervisors.get(id);
    if (!supervisor) return;
    supervisor.intentional = true;
    clearTimeout(supervisor.retry);
    supervisor.retry = undefined;
    const client = supervisor.client;
    supervisor.client = undefined;
    supervisor.startedAt = undefined;
    try {
      await supervisor.starting;
    } catch {
      // A start that was already failing is not a reason for a stop to fail.
    }
    await client?.close();
    const row = this.#repository.findById(id);
    if (row) this.#setStatus(row.id, row.enabled ? "stopped" : "disabled");
  }

  async restart(id: string): Promise<void> {
    await this.stop(id);
    await this.start(id);
  }

  /**
   * The handshake the mockup's test line reports. It reconnects on purpose so the latency is a
   * real handshake rather than a round trip over a connection that was already warm.
   */
  async test(id: string): Promise<McpTestResultDto> {
    const row = this.#require(id);
    await this.stop(row.id);
    try {
      await this.start(row.id);
    } catch (error: unknown) {
      return McpTestResultDto.parse({
        ok: false,
        latencyMs: 0,
        protocolVersion: null,
        serverName: null,
        serverVersion: null,
        toolCount: 0,
        resourceCount: 0,
        promptCount: 0,
        error: messageOf(error),
      });
    }
    const connected = this.#require(row.id);
    const counts = this.#repository.capabilityCounts(row.id);
    return McpTestResultDto.parse({
      ok: true,
      latencyMs: this.#supervisor(row.id).calls.at(-1)?.ms ?? 0,
      protocolVersion: connected.protocolVersion,
      serverName: connected.serverName,
      serverVersion: connected.serverVersion,
      toolCount: counts.tool,
      resourceCount: counts.resource,
      promptCount: counts.prompt,
      error: null,
    });
  }

  async dispose(): Promise<void> {
    this.#closed = true;
    await Promise.all([...this.#supervisors.keys()].map((id) => this.stop(id)));
    this.#supervisors.clear();
  }

  // ── invocation ──────────────────────────────────────────────────────────────

  /** What a kernel node resolves to. Permissions were already applied at step admission. */
  async call(
    target: Pick<McpNodeTarget, "serverId" | "tool" | "timeoutMs">,
    args: Readonly<Record<string, Json>>,
    signal?: AbortSignal,
  ): Promise<McpCallResultDto> {
    if (this.#closed) throw new AppError(AppErrorCode.CONFLICT, "Сервис MCP остановлен");
    signal?.throwIfAborted();
    await this.start(target.serverId);
    const supervisor = this.#supervisor(target.serverId);
    const client = supervisor.client;
    if (!client?.connected)
      throw new AppError(AppErrorCode.MCP_UNAVAILABLE, "MCP-сервер не подключён", {
        details: { serverId: target.serverId },
      });
    const startedAt = this.#now();
    const abort = new Promise<never>((_resolve, reject) => {
      signal?.addEventListener(
        "abort",
        () => reject(new AppError(AppErrorCode.RUN_CANCELLED, "Операция отменена")),
        { once: true },
      );
    });
    try {
      const result = await (signal === undefined
        ? client.callTool(target.tool, args, target.timeoutMs ?? this.#callTimeoutMs)
        : Promise.race([
            client.callTool(target.tool, args, target.timeoutMs ?? this.#callTimeoutMs),
            abort,
          ]));
      const dto = toCallResult(result);
      this.#recordCall(target.serverId, this.#now() - startedAt, dto.isError);
      this.#append(
        target.serverId,
        dto.isError ? "warn" : "info",
        `tools/call ${target.tool} ${dto.isError ? "error" : "ok"} ${Math.round(this.#now() - startedAt)}ms`,
      );
      return dto;
    } catch (error: unknown) {
      this.#recordCall(target.serverId, this.#now() - startedAt, true);
      this.#append(target.serverId, "error", `tools/call ${target.tool} — ${messageOf(error)}`);
      throw error;
    }
  }

  async callByToolKey(
    key: string,
    args: Readonly<Record<string, Json>>,
    signal?: AbortSignal,
  ): Promise<McpCallResultDto> {
    const capability = this.#repository.findCapabilityByToolKey(key);
    if (!capability)
      throw new AppError(AppErrorCode.NOT_FOUND, "Инструмент MCP не найден", {
        details: { key },
      });
    return this.call({ serverId: capability.serverId, tool: capability.name }, args, signal);
  }

  // ── health ──────────────────────────────────────────────────────────────────

  /**
   * The rail's numbers. Uptime and restarts are counted for this app session — a server the
   * app has not started this run has nothing honest to say about them.
   */
  health(id: string): McpHealthDto {
    const row = this.#require(id);
    const supervisor = this.#supervisor(row.id);
    const window = this.#now() - DAY_MS;
    const recent = supervisor.calls.filter((call) => call.at >= window);
    const durations = recent.map((call) => call.ms).sort((left, right) => left - right);
    return McpHealthDto.parse({
      serverId: row.id,
      status: row.status,
      uptimeMs:
        supervisor.startedAt === undefined
          ? null
          : Math.max(0, Math.round(this.#now() - supervisor.startedAt)),
      restarts: supervisor.restarts,
      restarting: supervisor.retry !== undefined,
      calls: recent.length,
      errors: recent.filter((call) => call.failed).length,
      medianLatencyMs: durations.length === 0 ? null : Math.round(median(durations)),
      recentLog: [...supervisor.log],
    });
  }

  // ── internals ───────────────────────────────────────────────────────────────

  async #launch(id: string): Promise<void> {
    const row = this.#require(id);
    this.#setStatus(id, "starting");
    const env = await this.#resolve(row.env);
    const headers = await this.#resolve(row.headers);
    const supervisor = this.#supervisor(id);
    const client = new McpClient({
      transport: this.#transport(row, env, headers),
      clientVersion: this.options.clientVersion ?? "0.0.0",
      requestTimeoutMs: this.options.requestTimeoutMs ?? 30_000,
      clock: this.#clock,
      onLog: (level, line) => this.#append(id, level, line),
      onMalformed: (payload) =>
        // Host-side only: the payload may quote whatever the server was handling.
        this.#log("error", "An MCP server sent a malformed message", {
          serverId: id,
          raw: payload,
        }),
      onClosed: (reason) => this.#died(id, reason),
    });
    supervisor.client = client;
    let discovery: McpDiscovery;
    let handshake;
    try {
      handshake = await client.connect();
      discovery = await client.discover(handshake.capabilities);
    } catch (error: unknown) {
      supervisor.client = undefined;
      await client.close().catch(() => undefined);
      supervisor.failures += 1;
      const message = messageOf(error);
      this.#repository.update(id, {
        status: "failed",
        lastError: message.slice(0, 2000),
        updatedAt: timestampNow(this.#clock),
      });
      this.#withdraw(this.#require(id));
      this.#append(id, "error", message);
      this.#log("warn", "An MCP server did not come up", { serverId: id, raw: message });
      throw error;
    }
    supervisor.failures = 0;
    supervisor.startedAt = this.#now();
    this.#recordCall(id, handshake.latencyMs, false);
    this.#append(id, "info", `initialize protocol ${handshake.protocolVersion}`);
    this.#append(
      id,
      "info",
      `tools/list ${discovery.tools.length} tools · ${discovery.resources.length} resources`,
    );
    this.#repository.update(id, {
      status: "connected",
      protocolVersion: handshake.protocolVersion,
      serverName: handshake.serverName || null,
      serverVersion: handshake.serverVersion || null,
      lastError: null,
      lastConnectedAt: timestampNow(this.#clock),
      updatedAt: timestampNow(this.#clock),
    });
    this.#adopt(this.#require(id), discovery);
    this.#log("info", "An MCP server is connected", {
      serverId: id,
      protocolVersion: handshake.protocolVersion,
      tools: discovery.tools.length,
    });
  }

  #transport(
    row: McpServerEntity,
    env: Record<string, string>,
    headers: Record<string, string>,
  ): McpTransport {
    if (this.options.transport) return this.options.transport(row, env);
    if (row.transport === "stdio") {
      if (!row.command)
        throw new AppError(AppErrorCode.VALIDATION_FAILED, "У stdio-сервера не задана команда");
      return new StdioTransport({
        command: row.command,
        args: row.args,
        env:
          this.options.toolchainDir === undefined
            ? env
            : { ...isolatedToolchainEnv(this.options.toolchainDir), ...env },
        ...(this.options.spawn === undefined ? {} : { spawn: this.options.spawn }),
      });
    }
    if (!row.url)
      throw new AppError(AppErrorCode.VALIDATION_FAILED, "У http-сервера не задан адрес");
    return new HttpTransport({
      url: row.url,
      headers,
      ...(this.options.fetch === undefined ? {} : { fetch: this.options.fetch }),
    });
  }

  /**
   * Secret references become plaintext here and nowhere else. The result is handed straight to
   * the transport; it is never written back to the row, never put in a DTO and never logged.
   */
  async #resolve(values: Readonly<Record<string, McpValue>>): Promise<Record<string, string>> {
    const resolved: Record<string, string> = {};
    for (const [name, value] of Object.entries(values)) {
      if (value.kind === "literal") {
        resolved[name] = value.value;
        continue;
      }
      try {
        resolved[name] = await this.options.secrets.resolve(value.secretId);
      } catch (cause: unknown) {
        throw new AppError(
          AppErrorCode.SECRET_MISSING,
          `Не удалось получить значение для ${name}`,
          { cause, details: { name } },
        );
      }
    }
    return resolved;
  }

  /** Discovery result → the TASK_031 registry and a kernel node per tool. */
  #adopt(row: McpServerEntity, discovery: McpDiscovery): void {
    const now = timestampNow(this.#clock);
    const kept: string[] = [];
    const live = new Set<string>();
    for (const tool of discovery.tools) {
      const key = toolKey(row.namespace, tool.name);
      const readOnly = inferReadOnly(tool);
      const destructive = inferDestructive(tool);
      kept.push(
        this.#repository.upsertCapability({
          id: createId(),
          serverId: row.id,
          kind: "tool",
          name: tool.name,
          title: tool.title ?? null,
          description: tool.description,
          toolKey: key,
          uri: null,
          mimeType: null,
          schema: (tool.inputSchema ?? null) as Json,
          readOnly,
          destructive,
          discoveredAt: now,
        }).id,
      );
      live.add(key);
      this.options.tools.register({
        key,
        source: "mcp",
        sourceRef: row.id,
        displayName: tool.title ?? key,
        description: tool.description,
        inputSchema: (tool.inputSchema ?? null) as Json,
        readOnly,
        destructive,
      });
      registerMcpToolNode(
        this.options.nodes,
        { key, serverId: row.id, tool: tool.name, readOnly },
        (target, args, signal) => this.call(target, args, signal),
      );
    }
    for (const resource of discovery.resources)
      kept.push(
        this.#repository.upsertCapability({
          id: createId(),
          serverId: row.id,
          kind: "resource",
          name: resource.name || resource.uri,
          title: resource.title ?? null,
          description: resource.description,
          toolKey: null,
          uri: resource.uri,
          mimeType: resource.mimeType ?? null,
          schema: null,
          readOnly: true,
          destructive: false,
          discoveredAt: now,
        }).id,
      );
    // Prompts are stored and left alone: surfacing them is a later task.
    for (const prompt of discovery.prompts)
      kept.push(
        this.#repository.upsertCapability({
          id: createId(),
          serverId: row.id,
          kind: "prompt",
          name: prompt.name,
          title: prompt.title ?? null,
          description: prompt.description,
          toolKey: null,
          uri: null,
          mimeType: null,
          schema: prompt.arguments as Json,
          readOnly: true,
          destructive: false,
          discoveredAt: now,
        }).id,
      );
    const stale = this.#repository
      .capabilitiesOfKind(row.id, "tool")
      .filter((capability) => capability.toolKey !== null && !live.has(capability.toolKey));
    for (const capability of stale) {
      this.options.tools.deregister(capability.toolKey!);
      this.options.nodes.unregister(capability.toolKey!);
    }
    this.#repository.pruneCapabilities(row.id, kept);
  }

  /** A server that is gone must not leave callable tools behind. */
  #withdraw(row: McpServerEntity): void {
    for (const dto of this.options.tools.deregisterSource("mcp", row.id))
      this.options.nodes.unregister(dto.key);
  }

  #died(id: string, reason: string): void {
    const supervisor = this.#supervisors.get(id);
    if (!supervisor || supervisor.starting !== undefined) return;
    supervisor.client = undefined;
    supervisor.startedAt = undefined;
    if (supervisor.intentional || this.#closed) return;
    const row = this.#repository.findById(id);
    if (!row || !row.enabled) return;
    this.#repository.update(id, {
      status: "failed",
      lastError: reason.slice(0, 2000),
      updatedAt: timestampNow(this.#clock),
    });
    this.#withdraw(row);
    this.#append(id, "error", reason);
    this.#log("warn", "An MCP server died", { serverId: id, raw: reason });
    supervisor.failures += 1;
    if (supervisor.failures > this.#maxRestarts) {
      this.#append(id, "error", `Перезапуск прекращён после ${this.#maxRestarts} попыток`);
      return;
    }
    const delay = Math.min(
      this.#maxBackoffMs,
      this.#backoffMs * 2 ** Math.max(0, supervisor.failures - 1),
    );
    supervisor.restarts += 1;
    this.#append(id, "info", `перезапуск через ${delay} мс`);
    supervisor.retry = setTimeout(() => {
      supervisor.retry = undefined;
      void this.start(id).catch(() => undefined);
    }, delay);
    supervisor.retry.unref();
  }

  #supervisor(id: string): Supervisor {
    let supervisor = this.#supervisors.get(id);
    if (!supervisor) {
      supervisor = { restarts: 0, failures: 0, calls: [], log: [], intentional: false };
      this.#supervisors.set(id, supervisor);
    }
    return supervisor;
  }

  #recordCall(id: string, ms: number, failed: boolean): void {
    const supervisor = this.#supervisor(id);
    supervisor.calls.push({ at: this.#now(), ms: Math.max(0, Math.round(ms)), failed });
    if (supervisor.calls.length > CALL_SAMPLES) supervisor.calls.splice(0, 1);
  }

  #append(id: string, level: McpLogLevel, line: string): void {
    const supervisor = this.#supervisor(id);
    supervisor.log.unshift({ at: timestampNow(this.#clock), level, line: line.slice(0, 1000) });
    if (supervisor.log.length > LOG_LINES) supervisor.log.length = LOG_LINES;
  }

  #setStatus(id: string, status: McpServerStatus): void {
    const row = this.#repository.findById(id);
    if (!row || row.status === status) return;
    this.#repository.update(id, { status, updatedAt: timestampNow(this.#clock) });
  }

  #validateShape(transport: string, command: string | null, url: string | null): void {
    if (transport === "stdio" && !command)
      throw new AppError(AppErrorCode.VALIDATION_FAILED, "Укажите команду запуска", {
        details: { field: "command" },
      });
    if (transport === "http" && !url)
      throw new AppError(AppErrorCode.VALIDATION_FAILED, "Укажите адрес сервера", {
        details: { field: "url" },
      });
  }

  #uniqueNamespace(candidate: string, ownId: string | null): string {
    const base = McpNamespace.parse(candidate);
    for (let suffix = 0; suffix < 100; suffix++) {
      const namespace = suffix === 0 ? base : `${base}${suffix + 1}`;
      const taken = this.#repository.findByNamespace(namespace);
      if (!taken || taken.id === ownId) return namespace;
    }
    throw new AppError(AppErrorCode.CONFLICT, "Не удалось подобрать свободное пространство имён");
  }

  #syncSecretUsage(id: string, before: string[], after: string[]): void {
    for (const secretId of before.filter((entry) => !after.includes(entry)))
      this.options.secrets.removeUsage(secretId, "mcp_server", id);
    for (const secretId of after.filter((entry) => !before.includes(entry)))
      this.options.secrets.addUsage(secretId, "mcp_server", id);
  }

  #toDto(row: McpServerEntity, counts: Record<McpCapabilityKind, number>): McpServerDto {
    return McpServerDto.parse({
      id: row.id,
      name: row.name,
      namespace: row.namespace,
      transport: row.transport,
      command: row.command,
      args: row.args,
      env: row.env,
      url: row.url,
      headers: row.headers,
      enabled: row.enabled,
      startup: row.startup,
      status: row.status,
      protocolVersion: row.protocolVersion,
      serverName: row.serverName,
      serverVersion: row.serverVersion,
      lastError: row.lastError,
      lastConnectedAt: row.lastConnectedAt,
      toolCount: counts.tool,
      resourceCount: counts.resource,
      promptCount: counts.prompt,
      createdAt: row.createdAt,
      updatedAt: row.updatedAt,
    });
  }

  #require(id: string): McpServerEntity {
    const row = this.#repository.findById(id);
    if (!row)
      throw new AppError(AppErrorCode.NOT_FOUND, "MCP-сервер не найден", {
        details: { serverId: id },
      });
    return row;
  }

  #log(level: McpLogLevel, message: string, fields: Record<string, unknown> = {}): void {
    this.options.logger?.log(level, "mcp", message, fields);
  }

  get #repository() {
    return this.options.data.repositories.mcpServers;
  }

  get #clock(): () => number {
    return this.options.clock ?? Date.now;
  }

  get #callTimeoutMs(): number {
    return this.options.callTimeoutMs ?? 60_000;
  }

  get #backoffMs(): number {
    return this.options.backoffMs ?? 1_000;
  }

  get #maxBackoffMs(): number {
    return this.options.maxBackoffMs ?? 60_000;
  }

  get #maxRestarts(): number {
    return this.options.maxRestarts ?? 5;
  }

  #now(): number {
    return this.#clock();
  }
}

/** `qdrant-local` → `qdrantLocal`, so the key's first segment matches §17's `namespace.verb`. */
export function deriveNamespace(name: string): string {
  const parts = name
    .split(/[^A-Za-z0-9]+/)
    .filter((part) => part.length > 0)
    .map((part) => part.toLowerCase());
  const joined = parts
    .map((part, index) => (index === 0 ? part : part[0]!.toUpperCase() + part.slice(1)))
    .join("");
  // A name that opens with digits still has to produce a key whose first segment starts with a
  // letter, so everything before the first letter goes.
  const trimmed = joined.replace(/^[^A-Za-z]+/, "");
  if (trimmed.length === 0) return "server";
  return (trimmed[0]!.toLowerCase() + trimmed.slice(1)).slice(0, 48);
}

export function toolKey(namespace: string, tool: string): string {
  const suffix = tool.replace(/[^A-Za-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "");
  return `${namespace}.${suffix.length > 0 ? suffix.slice(0, 64) : "call"}`;
}

function toCapabilityDto(row: McpCapabilityEntity): McpCapabilityDto {
  return McpCapabilityDto.parse({
    id: row.id,
    serverId: row.serverId,
    kind: row.kind,
    name: row.name,
    title: row.title,
    description: row.description,
    toolKey: row.toolKey,
    uri: row.uri,
    mimeType: row.mimeType,
    schema: row.schema ?? null,
    readOnly: row.readOnly,
    destructive: row.destructive,
    discoveredAt: row.discoveredAt,
  });
}

function toCallResult(result: CallToolResult): McpCallResultDto {
  const content = result.content.map((entry) => {
    const { type, text, ...rest } = entry;
    return {
      type,
      text: typeof text === "string" ? text : null,
      data: (Object.keys(rest).length > 0 ? rest : null) as Json,
    };
  });
  return McpCallResultDto.parse({
    content,
    text: content
      .map((entry) => entry.text)
      .filter((entry): entry is string => entry !== null)
      .join("\n"),
    structured: (result.structuredContent ?? null) as Json,
    isError: result.isError,
  });
}

function secretIds(row: McpServerEntity): string[] {
  return [...Object.values(row.env), ...Object.values(row.headers)]
    .filter((value) => value.kind === "secret")
    .map((value) => value.secretId);
}

function median(sorted: readonly number[]): number {
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 1 ? sorted[middle]! : (sorted[middle - 1]! + sorted[middle]!) / 2;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
