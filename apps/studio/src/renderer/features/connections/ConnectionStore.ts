import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type {
  Contract,
  McpCapabilityDto,
  McpHealthDto,
  McpServerDetailDto,
  McpServerDto,
  McpServerId,
  McpTestResultDto,
  SecretSummaryDto,
  ToolDto,
  ToolId,
  ToolReferenceDto,
  ToolTier,
} from "@zvs/shared";
import { connectionErrorCopy, connectionFieldErrors } from "./connectionErrors";
import { serverState, type ServerState } from "./connectionPresentation";
import type { ServerPreset } from "./serverRegistry";
import { ServerFormVm } from "./ServerFormVm";

export type PendingIntent =
  | { readonly kind: "create"; readonly preset: ServerPreset | null }
  | { readonly kind: "select"; readonly id: McpServerId };

export interface ToolRow {
  readonly capability: McpCapabilityDto;
  readonly tool: ToolDto | null;
}

export interface UsedBy {
  readonly scenarios: number;
  readonly skills: number;
}

export interface ConnectionStoreOptions {
  readonly pollMs?: number;
}

export default class ConnectionStore {
  servers: McpServerDto[] = [];
  health: Record<string, McpHealthDto> = {};
  selectedId: McpServerId | null = null;
  detail: McpServerDetailDto | null = null;
  form: ServerFormVm | null = null;
  tools: ToolDto[] = [];
  references: ToolReferenceDto[] = [];
  secrets: SecretSummaryDto[] = [];
  testResult: McpTestResultDto | null = null;
  pending: PendingIntent | null = null;
  registryOpen = false;
  loading = false;
  loaded = false;
  saving = false;
  removing = false;
  testing = false;
  switching = false;
  busyToolIds: ToolId[] = [];
  error: string | null = null;

  private timer: ReturnType<typeof setInterval> | undefined;
  private polling = false;
  private readonly pollMs: number;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    options: ConnectionStoreOptions = {},
  ) {
    this.pollMs = options.pollMs ?? 3000;
    makeAutoObservable<ConnectionStore, "ipc" | "timer" | "polling" | "pollMs">(
      this,
      { ipc: false, timer: false, polling: false, pollMs: false },
      { autoBind: true },
    );
  }

  get selectedServer(): McpServerDto | null {
    return this.servers.find((server) => server.id === this.selectedId) ?? null;
  }

  get selectedHealth(): McpHealthDto | null {
    return this.selectedId === null ? null : (this.health[this.selectedId] ?? null);
  }

  get selectedState(): ServerState | null {
    const server = this.selectedServer;
    return server === null ? null : serverState(server, this.selectedHealth);
  }

  stateOf(server: McpServerDto): ServerState {
    return serverState(server, this.health[server.id] ?? null);
  }

  get toolRows(): ToolRow[] {
    const capabilities = this.detail?.capabilities ?? [];
    return capabilities
      .filter((capability) => capability.kind === "tool")
      .map((capability) => ({
        capability,
        tool: this.tools.find((tool) => tool.key === capability.toolKey) ?? null,
      }));
  }

  get enabledToolCount(): number {
    return this.toolRows.filter((row) => row.tool?.enabled === true).length;
  }

  get exposedToolCount(): number {
    return this.servers
      .filter((server) => server.enabled)
      .reduce((total, server) => total + server.toolCount, 0);
  }

  get usedBy(): UsedBy {
    const scenarios = new Set<string>();
    const skills = new Set<string>();
    for (const reference of this.references) {
      if (reference.kind === "scenario") scenarios.add(reference.refId);
      else skills.add(reference.refId);
    }
    return { scenarios: scenarios.size, skills: skills.size };
  }

  get isEmpty(): boolean {
    return this.loaded && this.servers.length === 0;
  }

  isToolBusy(id: ToolId): boolean {
    return this.busyToolIds.includes(id);
  }

  mount(): void {
    void this.load();
    if (this.timer !== undefined || this.pollMs <= 0) return;
    this.timer = setInterval(() => {
      void this.poll();
    }, this.pollMs);
  }

  dispose(): void {
    if (this.timer !== undefined) clearInterval(this.timer);
    this.timer = undefined;
  }

  async load(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    this.error = null;
    try {
      const [servers, secrets] = await Promise.all([
        this.ipc.call("mcp.list", undefined),
        this.ipc.call("secrets.list", {}),
      ]);
      runInAction(() => {
        this.servers = [...servers];
        this.secrets = [...secrets];
        this.loaded = true;
        this.loading = false;
      });
      await this.refreshHealth();
      if (this.selectedId !== null) await this.refreshSelected(this.selectedId);
      else if (this.form === null && this.servers[0] !== undefined)
        await this.select(this.servers[0].id);
    } catch (error: unknown) {
      runInAction(() => {
        this.error = connectionErrorCopy(error);
        this.loaded = true;
        this.loading = false;
      });
    }
  }

  async poll(): Promise<void> {
    if (this.polling || !this.loaded) return;
    this.polling = true;
    try {
      const servers = await this.ipc.call("mcp.list", undefined);
      runInAction(() => {
        this.servers = [...servers];
      });
      await this.refreshHealth();
      const id = this.selectedId;
      const server = this.selectedServer;
      const detail = this.detail;
      if (
        id !== null &&
        server !== null &&
        detail !== null &&
        (server.updatedAt !== detail.updatedAt ||
          server.status !== detail.status ||
          server.toolCount !== detail.toolCount)
      ) {
        await this.fetchDetail(id, { keepForm: true });
        await this.loadTools(id);
      }
    } catch {
      return;
    } finally {
      this.polling = false;
    }
  }

  async select(id: McpServerId): Promise<void> {
    this.selectedId = id;
    this.detail = null;
    this.form = null;
    this.tools = [];
    this.references = [];
    this.testResult = null;
    await this.fetchDetail(id, { keepForm: false });
    await Promise.all([this.loadTools(id), this.refreshHealth([id])]);
  }

  requestSelect(id: McpServerId): void {
    if (id === this.selectedId && this.form !== null) return;
    if (this.form?.dirty === true) {
      this.pending = { kind: "select", id };
      return;
    }
    void this.select(id);
  }

  startCreate(preset: ServerPreset | null = null): void {
    this.selectedId = null;
    this.detail = null;
    this.tools = [];
    this.references = [];
    this.testResult = null;
    this.form = new ServerFormVm({ preset });
  }

  requestCreate(preset: ServerPreset | null = null): void {
    this.registryOpen = false;
    if (this.form?.dirty === true) {
      this.pending = { kind: "create", preset };
      return;
    }
    this.startCreate(preset);
  }

  confirmPending(): void {
    const intent = this.pending;
    this.pending = null;
    if (intent === null) return;
    if (intent.kind === "create") this.startCreate(intent.preset);
    else void this.select(intent.id);
  }

  cancelPending(): void {
    this.pending = null;
  }

  openRegistry(): void {
    this.registryOpen = true;
  }

  closeRegistry(): void {
    this.registryOpen = false;
  }

  dismissError(): void {
    this.error = null;
  }

  resetForm(): void {
    const server = this.selectedServer;
    if (server === null) {
      this.form = null;
      if (this.servers[0] !== undefined) void this.select(this.servers[0].id);
      return;
    }
    this.form = new ServerFormVm({ server: this.detail ?? server });
  }

  async submit(): Promise<boolean> {
    const form = this.form;
    if (form === null || this.saving) return false;
    form.setBanner(null);
    if (!form.validate()) return false;
    this.saving = true;
    this.error = null;
    try {
      const server = form.isNew
        ? await this.ipc.call("mcp.create", form.toCreateInput())
        : await this.ipc.call("mcp.update", form.toUpdateInput());
      runInAction(() => {
        this.saving = false;
        this.mergeServer(server);
        this.selectedId = server.id;
      });
      await this.fetchDetail(server.id, { keepForm: false });
      await Promise.all([this.loadTools(server.id), this.refreshHealth([server.id])]);
      return true;
    } catch (error: unknown) {
      const fieldErrors = connectionFieldErrors(error);
      runInAction(() => {
        this.saving = false;
        if (this.form === null) {
          this.error = connectionErrorCopy(error);
          return;
        }
        this.form.setErrors(fieldErrors);
        this.form.setBanner(
          Object.keys(fieldErrors).length > 0 ? null : connectionErrorCopy(error),
        );
      });
      return false;
    }
  }

  async removeSelected(): Promise<boolean> {
    const id = this.selectedId;
    if (id === null || this.removing) return false;
    this.removing = true;
    this.error = null;
    try {
      await this.ipc.call("mcp.remove", { id });
      runInAction(() => {
        this.removing = false;
        this.servers = this.servers.filter((server) => server.id !== id);
        this.health = Object.fromEntries(Object.entries(this.health).filter(([key]) => key !== id));
        this.selectedId = null;
        this.detail = null;
        this.form = null;
        this.tools = [];
        this.references = [];
        this.testResult = null;
      });
      if (this.servers[0] !== undefined) await this.select(this.servers[0].id);
      return true;
    } catch (error: unknown) {
      runInAction(() => {
        this.removing = false;
        this.error = connectionErrorCopy(error);
      });
      return false;
    }
  }

  async setServerEnabled(enabled: boolean): Promise<void> {
    const form = this.form;
    if (form === null) return;
    form.setEnabled(enabled);
    const id = this.selectedId;
    if (form.isNew || id === null || this.switching) return;
    this.switching = true;
    try {
      const server = await this.ipc.call("mcp.update", { id, enabled });
      runInAction(() => {
        this.switching = false;
        this.mergeServer(server);
      });
      await this.refreshSelected(id);
    } catch (error: unknown) {
      runInAction(() => {
        this.switching = false;
        this.form?.setEnabled(!enabled);
        this.error = connectionErrorCopy(error);
      });
    }
  }

  async startSelected(): Promise<void> {
    await this.lifecycle("mcp.start");
  }

  async stopSelected(): Promise<void> {
    await this.lifecycle("mcp.stop");
  }

  async test(): Promise<void> {
    const form = this.form;
    if (form === null || this.testing) return;
    if (form.dirty) {
      const saved = await this.submit();
      if (!saved) return;
    }
    const id = this.selectedId;
    if (id === null) return;
    this.testing = true;
    this.testResult = null;
    try {
      const result = await this.ipc.call("mcp.test", { id });
      runInAction(() => {
        this.testing = false;
        if (this.selectedId === id) this.testResult = result;
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.testing = false;
        this.error = connectionErrorCopy(error);
      });
    }
    await this.refreshSelected(id);
  }

  async setToolEnabled(id: ToolId, enabled: boolean): Promise<void> {
    await this.writeTool(id, () => this.ipc.call("tools.setEnabled", { id, enabled }));
  }

  async setToolTier(id: ToolId, tier: ToolTier): Promise<void> {
    await this.writeTool(id, () => this.ipc.call("tools.setTier", { id, tier }));
  }

  private async writeTool(id: ToolId, write: () => Promise<ToolDto>): Promise<void> {
    if (this.isToolBusy(id)) return;
    this.busyToolIds = [...this.busyToolIds, id];
    try {
      const tool = await write();
      runInAction(() => {
        this.tools = this.tools.map((entry) => (entry.id === tool.id ? tool : entry));
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = connectionErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.busyToolIds = this.busyToolIds.filter((entry) => entry !== id);
      });
    }
  }

  private async lifecycle(channel: "mcp.start" | "mcp.stop"): Promise<void> {
    const id = this.selectedId;
    if (id === null || this.switching) return;
    this.switching = true;
    this.error = null;
    try {
      const server = await this.ipc.call(channel, { id });
      runInAction(() => {
        this.switching = false;
        this.mergeServer(server);
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.switching = false;
        this.error = connectionErrorCopy(error);
      });
    }
    await this.refreshSelected(id);
  }

  private async refreshSelected(id: McpServerId): Promise<void> {
    try {
      const servers = await this.ipc.call("mcp.list", undefined);
      runInAction(() => {
        this.servers = [...servers];
      });
    } catch {
      return;
    }
    if (this.selectedId !== id) return;
    await this.fetchDetail(id, { keepForm: true });
    await Promise.all([this.loadTools(id), this.refreshHealth([id])]);
  }

  private async fetchDetail(
    id: McpServerId,
    options: { readonly keepForm: boolean },
  ): Promise<void> {
    try {
      const detail = await this.ipc.call("mcp.get", { id });
      runInAction(() => {
        if (this.selectedId !== id) return;
        this.detail = detail;
        this.mergeServer(detail);
        const keep = options.keepForm && this.form !== null && this.form.dirty;
        if (keep) return;
        this.form = new ServerFormVm({ server: detail });
      });
    } catch (error: unknown) {
      runInAction(() => {
        if (this.selectedId !== id) return;
        this.error = connectionErrorCopy(error);
      });
    }
  }

  private async loadTools(id: McpServerId): Promise<void> {
    try {
      const all = await this.ipc.call("tools.list", { source: "mcp", includeUnavailable: true });
      const tools = all.filter((tool) => tool.sourceRef === id);
      const usages = await Promise.all(
        tools.map(async (tool) => this.ipc.call("tools.usage", { id: tool.id })),
      );
      const seen = new Set<string>();
      const references: ToolReferenceDto[] = [];
      for (const usage of usages) {
        for (const reference of usage.references) {
          const key = `${reference.kind}:${reference.refId}`;
          if (seen.has(key)) continue;
          seen.add(key);
          references.push(reference);
        }
      }
      runInAction(() => {
        if (this.selectedId !== id) return;
        this.tools = [...tools];
        this.references = references;
      });
    } catch (error: unknown) {
      runInAction(() => {
        if (this.selectedId !== id) return;
        this.error = connectionErrorCopy(error);
      });
    }
  }

  private async refreshHealth(ids?: readonly McpServerId[]): Promise<void> {
    const targets = ids ?? this.servers.map((server) => server.id);
    const results = await Promise.all(
      targets.map(async (id) => {
        try {
          return await this.ipc.call("mcp.health", { id });
        } catch {
          return null;
        }
      }),
    );
    runInAction(() => {
      const next = ids === undefined ? {} : { ...this.health };
      for (const health of results) {
        if (health !== null) next[health.serverId] = health;
      }
      this.health = next;
    });
  }

  private mergeServer(server: McpServerDto): void {
    const summary: McpServerDto = {
      id: server.id,
      name: server.name,
      namespace: server.namespace,
      transport: server.transport,
      command: server.command,
      args: [...server.args],
      env: { ...server.env },
      url: server.url,
      headers: { ...server.headers },
      enabled: server.enabled,
      startup: server.startup,
      status: server.status,
      protocolVersion: server.protocolVersion,
      serverName: server.serverName,
      serverVersion: server.serverVersion,
      lastError: server.lastError,
      lastConnectedAt: server.lastConnectedAt,
      toolCount: server.toolCount,
      resourceCount: server.resourceCount,
      promptCount: server.promptCount,
      createdAt: server.createdAt,
      updatedAt: server.updatedAt,
    };
    const index = this.servers.findIndex((entry) => entry.id === server.id);
    this.servers =
      index === -1
        ? [...this.servers, summary]
        : this.servers.map((entry, at) => (at === index ? summary : entry));
  }
}
