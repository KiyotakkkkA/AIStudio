import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  ToolDto,
  toolNeedsConfirmation,
  type Contract,
  type McpServerDto,
  type ToolDefaultsDto,
  type ToolDetailDto,
  type ToolId,
  type ToolInvokeResultDto,
  type ToolTier,
} from "@zvs/shared";
import {
  ALL_SOURCES,
  effectiveTier,
  filterTools,
  sourceCounts,
  sourceGroups,
  tierCounts,
  type SourceGroup,
  type TierFilter,
  type ToolFilter,
} from "./toolFilters";
import { parseInput, seedText } from "./schemaFields";
import { toolErrorCopy, unavailability, type Unavailability } from "./toolPresentation";

export interface ToolStoreOptions {
  readonly pollMs?: number;
  readonly clock?: () => number;
}

export default class ToolStore {
  tools: ToolDto[] = [];
  servers: McpServerDto[] = [];
  source: string = ALL_SOURCES;
  tier: TierFilter = "all";
  query = "";
  selectedId: ToolId | null = null;
  detail: ToolDetailDto | null = null;
  inputText = "{}";
  inputDirty = false;
  running = false;
  confirming = false;
  result: ToolInvokeResultDto | null = null;
  runError: string | null = null;
  defaults: ToolDefaultsDto | null = null;
  defaultsDraft: ToolDefaultsDto | null = null;
  defaultsOpen = false;
  savingDefaults = false;
  busyIds: ToolId[] = [];
  loading = false;
  loaded = false;
  error: string | null = null;
  now: number;

  private timer: ReturnType<typeof setInterval> | undefined;
  private polling = false;
  private readonly pollMs: number;
  private readonly clock: () => number;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    options: ToolStoreOptions = {},
  ) {
    this.pollMs = options.pollMs ?? 5000;
    this.clock = options.clock ?? Date.now;
    this.now = this.clock();
    makeAutoObservable<ToolStore, "ipc" | "timer" | "polling" | "pollMs" | "clock">(
      this,
      { ipc: false, timer: false, polling: false, pollMs: false, clock: false },
      { autoBind: true },
    );
  }

  get filter(): ToolFilter {
    return { source: this.source, tier: this.tier, query: this.query };
  }

  get groups(): SourceGroup[] {
    return sourceGroups(this.tools, this.servers);
  }

  get visible(): ToolDto[] {
    return filterTools(this.tools, this.filter);
  }

  get sourceCounts(): Record<string, number> {
    return sourceCounts(this.tools, this.filter);
  }

  get tierCounts(): Record<ToolTier, number> {
    return tierCounts(this.tools, this.filter);
  }

  get isEmpty(): boolean {
    return this.loaded && this.tools.length === 0;
  }

  get selected(): ToolDto | null {
    return this.tools.find((tool) => tool.id === this.selectedId) ?? null;
  }

  get selectedTier(): ToolTier | null {
    const tool = this.selected;
    return tool === null ? null : effectiveTier(tool);
  }

  get selectedServer(): McpServerDto | null {
    const tool = this.selected;
    if (tool?.source !== "mcp") return null;
    return this.servers.find((server) => server.id === tool.sourceRef) ?? null;
  }

  get selectedUnavailability(): Unavailability | null {
    const tool = this.selected;
    return tool === null ? null : unavailability(tool, this.selectedServer);
  }

  get parsedInput() {
    return parseInput(this.inputText);
  }

  get needsConfirmation(): boolean {
    const tool = this.selected;
    return tool !== null && toolNeedsConfirmation(tool);
  }

  get blockedReason(): string | null {
    const tool = this.selected;
    if (tool === null) return "Инструмент не выбран";
    if (!tool.available) return "Инструмент недоступен";
    if (effectiveTier(tool) === "off") return "Разрешение «off» запрещает вызов";
    return null;
  }

  get canRun(): boolean {
    return !this.running && this.blockedReason === null && this.parsedInput.ok;
  }

  isBusy(id: ToolId): boolean {
    return this.busyIds.includes(id);
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
      const [tools, servers] = await Promise.all([
        this.ipc.call("tools.list", { includeUnavailable: true }),
        this.ipc.call("mcp.list", undefined),
      ]);
      runInAction(() => {
        this.tools = [...tools];
        this.servers = [...servers];
        this.now = this.clock();
        this.loaded = true;
        this.loading = false;
      });
      const keep = this.selectedId !== null && this.selected !== null;
      const first = this.visible[0];
      if (keep && this.selectedId !== null) await this.loadDetail(this.selectedId);
      else if (first !== undefined) await this.select(first.id);
      else
        runInAction(() => {
          this.selectedId = null;
          this.detail = null;
        });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = toolErrorCopy(error);
        this.loaded = true;
        this.loading = false;
      });
    }
  }

  async poll(): Promise<void> {
    if (this.polling || !this.loaded) return;
    this.polling = true;
    try {
      const [tools, servers] = await Promise.all([
        this.ipc.call("tools.list", { includeUnavailable: true }),
        this.ipc.call("mcp.list", undefined),
      ]);
      runInAction(() => {
        this.tools = tools.map((tool) => (this.isBusy(tool.id) ? this.local(tool) : tool));
        this.servers = [...servers];
        this.now = this.clock();
      });
    } catch {
      return;
    } finally {
      runInAction(() => {
        this.polling = false;
      });
    }
  }

  setSource(source: string): void {
    this.source = source;
  }

  setTierFilter(tier: TierFilter): void {
    this.tier = this.tier === tier ? "all" : tier;
  }

  setQuery(query: string): void {
    this.query = query;
  }

  async select(id: ToolId): Promise<void> {
    if (this.selectedId === id && this.detail?.id === id) return;
    this.selectedId = id;
    this.detail = null;
    this.result = null;
    this.runError = null;
    this.confirming = false;
    const tool = this.selected;
    this.inputText = seedText(tool?.inputSchema ?? null);
    this.inputDirty = false;
    await this.loadDetail(id);
  }

  async loadDetail(id: ToolId): Promise<void> {
    try {
      const detail = await this.ipc.call("tools.get", { id });
      runInAction(() => {
        if (this.selectedId !== id) return;
        this.detail = detail;
        this.replace(detail);
        if (!this.inputDirty) this.inputText = seedText(detail.inputSchema);
      });
    } catch (error: unknown) {
      runInAction(() => {
        if (this.selectedId === id) this.error = toolErrorCopy(error);
      });
    }
  }

  async setTier(id: ToolId, tier: ToolTier): Promise<void> {
    const tool = this.tools.find((entry) => entry.id === id);
    if (tool === undefined || this.isBusy(id)) return;
    if (effectiveTier(tool) === tier) return;
    this.busyIds.push(id);
    this.error = null;
    try {
      if (tier !== "off" && !tool.enabled) {
        const enabled = await this.ipc.call("tools.setEnabled", { id, enabled: true });
        runInAction(() => {
          this.replace(enabled);
        });
      }
      if (tool.tier !== tier) {
        const updated = await this.ipc.call("tools.setTier", { id, tier });
        runInAction(() => {
          this.replace(updated);
        });
      }
    } catch (error: unknown) {
      runInAction(() => {
        this.error = toolErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.busyIds = this.busyIds.filter((entry) => entry !== id);
      });
    }
  }

  setInputText(text: string): void {
    this.inputText = text;
    this.inputDirty = true;
  }

  resetInput(): void {
    this.inputText = seedText(this.selected?.inputSchema ?? null);
    this.inputDirty = false;
  }

  requestRun(): void {
    if (!this.canRun) return;
    if (this.needsConfirmation) {
      this.confirming = true;
      return;
    }
    void this.run(false);
  }

  cancelRun(): void {
    this.confirming = false;
  }

  confirmRun(): void {
    this.confirming = false;
    void this.run(true);
  }

  async run(confirmed: boolean): Promise<void> {
    const tool = this.selected;
    const parsed = this.parsedInput;
    if (tool === null || !parsed.ok || this.running) return;
    this.running = true;
    this.result = null;
    this.runError = null;
    try {
      const result = await this.ipc.call("tools.invoke", {
        id: tool.id,
        input: parsed.value,
        confirmed,
      });
      runInAction(() => {
        if (this.selectedId === tool.id) this.result = result;
      });
      await this.refreshTool(tool.id);
    } catch (error: unknown) {
      runInAction(() => {
        if (this.selectedId === tool.id) this.runError = toolErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.running = false;
      });
    }
  }

  async openDefaults(): Promise<void> {
    this.defaultsOpen = true;
    try {
      const defaults = await this.ipc.call("tools.defaults", undefined);
      runInAction(() => {
        this.defaults = defaults;
        this.defaultsDraft = { ...defaults };
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = toolErrorCopy(error);
        this.defaultsOpen = false;
      });
    }
  }

  closeDefaults(): void {
    this.defaultsOpen = false;
    this.defaultsDraft = null;
  }

  setDefault(source: keyof ToolDefaultsDto, tier: ToolDefaultsDto[keyof ToolDefaultsDto]): void {
    if (this.defaultsDraft === null) return;
    this.defaultsDraft = { ...this.defaultsDraft, [source]: tier };
  }

  async saveDefaults(): Promise<void> {
    const draft = this.defaultsDraft;
    if (draft === null || this.savingDefaults) return;
    this.savingDefaults = true;
    try {
      const saved = await this.ipc.call("tools.setDefaults", draft);
      const tools = await this.ipc.call("tools.list", { includeUnavailable: true });
      runInAction(() => {
        this.defaults = saved;
        this.tools = [...tools];
        this.defaultsOpen = false;
        this.defaultsDraft = null;
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = toolErrorCopy(error);
      });
    } finally {
      runInAction(() => {
        this.savingDefaults = false;
      });
    }
  }

  dismissError(): void {
    this.error = null;
  }

  private async refreshTool(id: ToolId): Promise<void> {
    try {
      const detail = await this.ipc.call("tools.get", { id });
      runInAction(() => {
        this.replace(detail);
        if (this.selectedId === id) this.detail = detail;
        this.now = this.clock();
      });
    } catch {
      return;
    }
  }

  private local(tool: ToolDto): ToolDto {
    return this.tools.find((entry) => entry.id === tool.id) ?? tool;
  }

  private replace(tool: ToolDto): void {
    const dto = ToolDto.parse(tool);
    const index = this.tools.findIndex((entry) => entry.id === tool.id);
    if (index === -1) this.tools.push(dto);
    else this.tools[index] = dto;
  }
}
