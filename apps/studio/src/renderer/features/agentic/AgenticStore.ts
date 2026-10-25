import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  ConversationId,
  type AgenticPermissionMode,
  type AgenticPlanEditDto,
  type AgenticSessionDto,
  type AgenticSessionId,
  type Contract,
  type HostEvent,
  type ProviderDto,
  type RunId,
  type StreamId,
} from "@zvs/shared";
import type { EventRouter, RoutedEvent } from "../../app/EventRouter";
import {
  agenticErrorCopy,
  CREATE_FAILED,
  DECIDE_FAILED,
  EXTEND_FAILED,
  FOLDER_FAILED,
  INSTRUCT_FAILED,
  LOAD_FAILED,
  MODE_FAILED,
  OPEN_FAILED,
  PLAN_FAILED,
  REMOVE_FAILED,
  STOP_FAILED,
} from "./agenticCopy";
import ExplorerStore from "../chat/explorer/ExplorerStore";
import TerminalStore from "../terminal/TerminalStore";
import SessionDraft from "./SessionDraft";
import {
  applyEvent,
  budgetLines,
  dropEdit,
  EMPTY_STREAM,
  extendedBudget,
  moveEdit,
  newerSession,
  planProgress,
  planRows,
  queuedEdits,
  renameEdit,
  replay,
  runningCall,
  sessionPhase,
  type BudgetLine,
  type PlanProgress,
  type PlanRow,
  type SessionPhase,
  type StreamState,
} from "./sessionModel";

export type ApprovalAnswer = "once" | "always" | "deny";
export type SessionPanel = "session" | "files";

const FINISHED = new Set<AgenticSessionDto["status"]>([
  "completed",
  "stopped",
  "failed",
  "cancelled",
]);

export function isFinished(session: Pick<AgenticSessionDto, "status">): boolean {
  return FINISHED.has(session.status);
}

export default class AgenticStore {
  sessions: AgenticSessionDto[] = [];
  providers: ProviderDto[] = [];
  loaded = false;
  error: string | null = null;
  activeId: AgenticSessionId | null = null;
  stream: StreamState = EMPTY_STREAM;
  streamLoaded = false;
  readonly draft = new SessionDraft();
  formOpen = false;
  formError: string | null = null;
  creating = false;
  picking = false;
  instruction = "";
  instructing = false;
  deciding = false;
  stopping = false;
  extending = false;
  planBusy = false;
  modeBusy = false;
  removingId: AgenticSessionId | null = null;
  sidePanel: SessionPanel = "session";
  panelOpen = true;
  readonly explorer: ExplorerStore;
  readonly terminal: TerminalStore;

  private detach: (() => void) | null = null;
  private listening: StreamId | null = null;
  private syncing = false;
  private held: RoutedEvent[] = [];
  private revision = 0;
  private mounted = false;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly events: EventRouter,
  ) {
    this.explorer = new ExplorerStore(ipc, events);
    this.terminal = new TerminalStore(ipc, events);
    makeAutoObservable<
      AgenticStore,
      "ipc" | "events" | "detach" | "listening" | "syncing" | "held" | "revision" | "mounted"
    >(
      this,
      {
        ipc: false,
        events: false,
        detach: false,
        listening: false,
        syncing: false,
        held: false,
        revision: false,
        mounted: false,
        explorer: false,
        terminal: false,
      },
      { autoBind: true },
    );
  }

  get ordered(): AgenticSessionDto[] {
    return [...this.sessions].sort((left, right) => right.updatedAt - left.updatedAt);
  }

  get session(): AgenticSessionDto | null {
    return this.sessions.find((session) => session.id === this.activeId) ?? null;
  }

  get editable(): boolean {
    return this.session !== null && !isFinished(this.session);
  }

  get phase(): SessionPhase | null {
    return this.session === null ? null : sessionPhase(this.session, this.stream.approval);
  }

  get rows(): PlanRow[] {
    return this.session === null ? [] : planRows(this.session, this.editable);
  }

  get progress(): PlanProgress {
    return planProgress(this.rows);
  }

  get budget(): BudgetLine[] {
    return this.session === null ? [] : budgetLines(this.session.budget, this.session.usage);
  }

  get activeCall() {
    return runningCall(this.stream.entries);
  }

  get provider(): ProviderDto | null {
    const id = this.session?.providerId;
    return this.providers.find((provider) => provider.id === id) ?? null;
  }

  get modelName(): string {
    const session = this.session;
    if (session === null) return "";
    const model = this.provider?.models.find((entry) => entry.externalId === session.modelId);
    return model?.displayName && model.displayName.length > 0 ? model.displayName : session.modelId;
  }

  get explorerOwner(): ConversationId | null {
    const parsed = ConversationId.safeParse(this.activeId);
    return parsed.success ? parsed.data : null;
  }

  get workingFolder(): string | null {
    return this.session?.workspace ?? null;
  }

  get activePanel(): SessionPanel {
    return this.sidePanel === "files" && this.workingFolder !== null ? "files" : "session";
  }

  isPanelShown(panel: SessionPanel): boolean {
    return this.panelOpen && this.activePanel === panel;
  }

  showPanel(panel: SessionPanel): void {
    if (this.activePanel === panel) {
      this.panelOpen = !this.panelOpen;
      return;
    }
    this.sidePanel = panel;
    this.panelOpen = true;
  }

  closePanel(): void {
    this.panelOpen = false;
  }

  async revealInExplorer(path: string): Promise<void> {
    const owner = this.explorerOwner;
    if (owner === null) return;
    this.sidePanel = "files";
    this.panelOpen = true;
    await this.explorer.show(owner);
    const parts = path.split("/").filter((part) => part.length > 0);
    for (let depth = 1; depth < parts.length; depth += 1) {
      const folder = parts.slice(0, depth).join("/");
      if (!this.explorer.expanded.has(folder)) await this.explorer.toggle(folder);
    }
    this.explorer.focus(parts.join("/"));
  }

  get canCreate(): boolean {
    return !this.creating && this.draft.input !== null;
  }

  async mount(): Promise<void> {
    this.mounted = true;
    await Promise.all([this.load(), this.loadProviders()]);
    if (!this.mounted) return;
    if (this.activeId !== null && this.session !== null && this.streamLoaded) return;
    const reopen = this.activeId ?? this.ordered.find((entry) => entry.status === "running")?.id;
    if (reopen !== undefined) await this.open(reopen);
  }

  unmount(): void {
    this.mounted = false;
    this.explorer.dispose();
    this.terminal.dispose();
  }

  async load(): Promise<void> {
    try {
      const sessions = await this.ipc.call("agentic.list", undefined);
      runInAction(() => {
        this.sessions = sessions.map((session) =>
          newerSession(this.sessions.find((entry) => entry.id === session.id) ?? null, session),
        );
        this.loaded = true;
        this.error = null;
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = agenticErrorCopy(error, LOAD_FAILED);
        this.loaded = true;
      });
    }
  }

  async loadProviders(): Promise<void> {
    try {
      const summaries = await this.ipc.call("providers.list", {
        capability: "text",
        enabled: true,
      });
      const providers = await Promise.all(
        summaries.map((provider) =>
          this.ipc.call("providers.get", { id: provider.id, selectedOnly: true }),
        ),
      );
      runInAction(() => {
        this.providers = providers;
        this.draft.adopt(providers);
      });
    } catch {
      runInAction(() => {
        this.providers = [];
      });
    }
  }

  async open(id: AgenticSessionId): Promise<void> {
    const revision = ++this.revision;
    this.stopListening();
    this.held = [];
    this.syncing = false;
    this.activeId = id;
    this.stream = EMPTY_STREAM;
    this.streamLoaded = false;
    this.instruction = "";
    this.error = null;
    try {
      if (this.session === null) {
        const session = await this.ipc.call("agentic.get", { id });
        if (revision !== this.revision) return;
        runInAction(() => {
          this.upsert(session);
        });
      }
      await this.sync(revision);
    } catch (error: unknown) {
      runInAction(() => {
        if (revision !== this.revision) return;
        this.error = agenticErrorCopy(error, OPEN_FAILED);
        this.streamLoaded = true;
      });
    }
  }

  close(): void {
    this.revision += 1;
    this.stopListening();
    this.activeId = null;
    this.stream = EMPTY_STREAM;
    this.streamLoaded = false;
    this.instruction = "";
  }

  private async sync(revision: number): Promise<void> {
    const session = this.session;
    if (session === null) return;
    this.syncing = true;
    if (session.status === "running" && session.streamId !== null) this.listen(session.streamId);
    try {
      const page = await this.ipc.call("runs.list", {
        kinds: ["agentic"],
        subjectId: session.id,
        live: false,
        limit: 200,
      });
      const runs = [...page.items].sort((left, right) => left.createdAt - right.createdAt);
      const details = await Promise.all(
        runs.map((run) => this.ipc.call("runs.detail", { id: run.id })),
      );
      const events: HostEvent[] = details.flatMap((detail) => detail.events);
      const latest = await this.ipc.call("agentic.get", { id: session.id });
      runInAction(() => {
        if (revision !== this.revision) return;
        const rebuilt = replay(EMPTY_STREAM, events);
        const live = latest.status === "running";
        this.stream = live ? rebuilt.state : { ...rebuilt.state, approval: null };
        this.upsert(latest);
        if (rebuilt.session !== null) this.upsert(rebuilt.session);
        this.streamLoaded = true;
        this.drain();
        if (live && latest.streamId !== null) this.listen(latest.streamId);
        else this.stopListening();
      });
    } catch (error: unknown) {
      runInAction(() => {
        if (revision !== this.revision) return;
        this.error = agenticErrorCopy(error, OPEN_FAILED);
        this.streamLoaded = true;
        this.drain();
      });
    }
  }

  private drain(): void {
    this.syncing = false;
    const held = this.held;
    this.held = [];
    for (const event of held) this.absorb(event);
  }

  private listen(streamId: StreamId): void {
    if (this.listening === streamId) return;
    this.stopListening();
    this.listening = streamId;
    this.detach = this.events.subscribe(streamId, this.receive);
  }

  private stopListening(): void {
    this.detach?.();
    this.detach = null;
    this.listening = null;
  }

  receive(event: RoutedEvent): void {
    if (this.syncing) {
      this.held.push(event);
      return;
    }
    if ((event.gap ?? 0) > 0) {
      this.held.push(event);
      void this.sync(this.revision);
      return;
    }
    this.absorb(event);
  }

  private absorb(event: RoutedEvent): void {
    const update = applyEvent(this.stream, event);
    this.stream = update.state;
    if (update.session !== null) this.upsert(update.session);
    if (update.ended) {
      if (this.listening === event.streamId) this.stopListening();
      void this.refresh();
    }
  }

  private async refresh(): Promise<void> {
    const id = this.activeId;
    if (id === null) return;
    try {
      const session = await this.ipc.call("agentic.get", { id });
      runInAction(() => {
        this.upsert(session);
      });
    } catch {
      return;
    }
  }

  private upsert(session: AgenticSessionDto): void {
    const index = this.sessions.findIndex((entry) => entry.id === session.id);
    if (index === -1) {
      this.sessions = [session, ...this.sessions];
      return;
    }
    const current = this.sessions[index] ?? null;
    const next = newerSession(current, session);
    if (next === current) return;
    this.sessions = this.sessions.map((entry, position) => (position === index ? next : entry));
  }

  private adopt(session: AgenticSessionDto): void {
    this.upsert(session);
    if (session.id === this.activeId && session.status === "running" && session.streamId !== null) {
      this.listen(session.streamId);
    }
  }

  openForm(): void {
    this.formOpen = true;
    this.formError = null;
    this.draft.adopt(this.providers);
  }

  closeForm(): void {
    this.formOpen = false;
    this.formError = null;
  }

  async pickFolder(): Promise<void> {
    if (this.picking) return;
    this.picking = true;
    try {
      const picked = await this.ipc.call("chat.pickFolder", undefined);
      runInAction(() => {
        if (picked.path !== null) this.draft.setWorkspace(picked.path);
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.formError = agenticErrorCopy(error, FOLDER_FAILED);
      });
    } finally {
      runInAction(() => {
        this.picking = false;
      });
    }
  }

  async create(): Promise<boolean> {
    const input = this.draft.input;
    if (input === null || this.creating) return false;
    this.creating = true;
    this.formError = null;
    try {
      const session = await this.ipc.call("agentic.create", input);
      runInAction(() => {
        this.upsert(session);
        this.formOpen = false;
        this.draft.clearGoal();
      });
      await this.open(session.id);
      return true;
    } catch (error: unknown) {
      runInAction(() => {
        this.formError = agenticErrorCopy(error, CREATE_FAILED);
      });
      return false;
    } finally {
      runInAction(() => {
        this.creating = false;
      });
    }
  }

  async cancel(): Promise<void> {
    const session = this.session;
    if (session === null || isFinished(session) || this.stopping) return;
    this.stopping = true;
    try {
      await this.ipc.call("agentic.cancel", { id: session.id });
      await this.refresh();
    } catch (error: unknown) {
      runInAction(() => {
        this.error = agenticErrorCopy(error, STOP_FAILED);
      });
    } finally {
      runInAction(() => {
        this.stopping = false;
      });
    }
  }

  async remove(id: AgenticSessionId): Promise<void> {
    if (this.removingId !== null) return;
    this.removingId = id;
    try {
      await this.ipc.call("agentic.remove", { id });
      runInAction(() => {
        this.sessions = this.sessions.filter((session) => session.id !== id);
        if (this.activeId === id) this.close();
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = agenticErrorCopy(error, REMOVE_FAILED);
      });
    } finally {
      runInAction(() => {
        this.removingId = null;
      });
    }
  }

  async setMode(mode: AgenticPermissionMode): Promise<void> {
    const session = this.session;
    if (session === null || !this.editable || this.modeBusy) return;
    if ((session.pendingMode ?? session.mode) === mode) return;
    this.modeBusy = true;
    try {
      const updated = await this.ipc.call("agentic.setMode", { id: session.id, mode });
      runInAction(() => {
        this.upsert(updated);
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = agenticErrorCopy(error, MODE_FAILED);
      });
    } finally {
      runInAction(() => {
        this.modeBusy = false;
      });
    }
  }

  private async commitPlan(
    change: (edits: readonly AgenticPlanEditDto[]) => AgenticPlanEditDto[],
  ): Promise<void> {
    const session = this.session;
    if (session === null || !this.editable || this.planBusy) return;
    const steps = change(queuedEdits(session));
    this.planBusy = true;
    try {
      const updated = await this.ipc.call("agentic.updatePlan", { id: session.id, steps });
      runInAction(() => {
        this.upsert(updated);
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = agenticErrorCopy(error, PLAN_FAILED);
      });
    } finally {
      runInAction(() => {
        this.planBusy = false;
      });
    }
  }

  moveStep(from: number, to: number): Promise<void> {
    return this.commitPlan((edits) => moveEdit(edits, from, to));
  }

  renameStep(index: number, title: string): Promise<void> {
    return this.commitPlan((edits) => renameEdit(edits, index, title));
  }

  deleteStep(index: number): Promise<void> {
    return this.commitPlan((edits) => dropEdit(edits, index));
  }

  setInstruction(text: string): void {
    this.instruction = text;
  }

  async instruct(): Promise<void> {
    const session = this.session;
    const text = this.instruction.trim();
    if (session === null || !this.editable || text.length === 0 || this.instructing) return;
    this.instructing = true;
    try {
      const updated = await this.ipc.call("agentic.instruct", { id: session.id, text });
      runInAction(() => {
        this.upsert(updated);
        this.instruction = "";
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = agenticErrorCopy(error, INSTRUCT_FAILED);
      });
    } finally {
      runInAction(() => {
        this.instructing = false;
      });
    }
  }

  get extension() {
    const session = this.session;
    return session === null ? null : extendedBudget(session.budget, session.usage);
  }

  async extend(): Promise<void> {
    const session = this.session;
    const budget = this.extension;
    if (session === null || budget === null || session.status !== "paused" || this.extending) {
      return;
    }
    this.extending = true;
    try {
      const updated = await this.ipc.call("agentic.extend", { id: session.id, budget });
      runInAction(() => {
        this.adopt(updated);
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = agenticErrorCopy(error, EXTEND_FAILED);
      });
    } finally {
      runInAction(() => {
        this.extending = false;
      });
    }
  }

  async decide(answer: ApprovalAnswer): Promise<void> {
    const approval = this.stream.approval;
    if (approval === null || this.deciding) return;
    this.deciding = true;
    const id = approval.runId as RunId;
    try {
      if (answer === "deny") {
        await this.ipc.call("runs.deny", { id, approvalId: approval.id });
      } else {
        await this.ipc.call("runs.approve", {
          id,
          approvalId: approval.id,
          always: answer === "always",
        });
      }
    } catch (error: unknown) {
      runInAction(() => {
        this.error = agenticErrorCopy(error, DECIDE_FAILED);
      });
    } finally {
      runInAction(() => {
        if (this.stream.approval?.id === approval.id) {
          this.stream = { ...this.stream, approval: null };
        }
        this.deciding = false;
      });
    }
  }

  dismissError(): void {
    this.error = null;
  }

  dispose(): void {
    this.explorer.dispose();
    this.terminal.dispose();
    this.mounted = false;
    this.revision += 1;
    this.stopListening();
  }
}
