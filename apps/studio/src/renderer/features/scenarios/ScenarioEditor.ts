import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type {
  Contract,
  Json,
  McpServerDto,
  ScenarioDetailDto,
  ScenarioDto,
  ScenarioGraphDto,
  ScenarioId,
  ScenarioIssueDto,
  ScenarioNodeDto,
  ScenarioNodeSettingsDto,
  ScenarioNodeTypeDto,
  ScenarioOverlap,
  ScenarioParams,
  ScenarioPositionDto,
  ScenarioSlot,
  ToolDto,
} from "@zvs/shared";
import {
  applyAction,
  attachRefusal,
  catalogueOf,
  connectRefusal,
  findNode,
  freshNodeId,
  isTrunk,
  sameGraph,
  type Catalogue,
  type GraphAction,
  type NodePatch,
  type Refusal,
} from "./graphModel";
import { canRedo, canUndo, commit, redo, startHistory, undo, type GraphHistory } from "./history";
import type { EventRouter } from "../../app/EventRouter";
import ScenarioRunState from "./ScenarioRunState";
import { nodeSize, slotOffset, slotUnder, tidy, type SlotTarget } from "./layout";
import EntityOptions from "./entityParams";
import { ceilingReason, paletteGroups, typeLabel, type PaletteGroup } from "./palette";
import { TOOL_SET_PARAM, TOOL_SET_TYPE, toolSetProblem } from "./toolSet";
import {
  DRAFT_SAVE_FAILED,
  SCENARIO_LOAD_FAILED,
  issueText,
  refusalIssues,
  refusalText,
  savedAtLabel,
  scenarioErrorCopy,
} from "./scenarioCopy";

export interface Timers {
  readonly set: (callback: () => void, ms: number) => unknown;
  readonly clear: (handle: unknown) => void;
}

export interface ScenarioEditorOptions {
  readonly clock?: () => number;
  readonly timers?: Timers;
  readonly autosaveMs?: number;
  readonly validateMs?: number;
  readonly palettePinned?: boolean;
  readonly events?: EventRouter;
}

export interface Selection {
  readonly nodes: readonly string[];
  readonly links: readonly string[];
}

export type DraftState = "clean" | "pending" | "saving" | "saved" | "failed";

const NO_SELECTION: Selection = { nodes: [], links: [] };
const STEP_GAP = 96;
const SLOT_DROP = 96;

const DEFAULT_TIMERS: Timers = {
  set: (callback, ms) => setTimeout(callback, ms),
  clear: (handle) => {
    clearTimeout(handle as ReturnType<typeof setTimeout>);
  },
};

export default class ScenarioEditor {
  scenario: ScenarioDto | null = null;
  saved: ScenarioGraphDto = { nodes: [], edges: [], attachments: [] };
  history: GraphHistory = startHistory(this.saved);
  types: ScenarioNodeTypeDto[] = [];
  tools: ToolDto[] = [];
  servers: McpServerDto[] = [];
  selection: Selection = NO_SELECTION;
  dragPositions: Record<string, ScenarioPositionDto> = {};
  dropTarget: SlotTarget | null = null;
  issues: ScenarioIssueDto[] = [];
  refusal: string | null = null;
  notice: string | null = null;
  error: string | null = null;
  draftState: DraftState = "clean";
  draftSavedAt: number | null = null;
  loading = false;
  loaded = false;
  saving = false;
  switching = false;
  paletteOpen = false;
  paletteQuery = "";
  palettePinned: boolean;
  overlapSaving = false;
  readonly entities: EntityOptions;
  readonly execution: ScenarioRunState;

  private draftTimer: unknown = undefined;
  private validateTimer: unknown = undefined;
  private validation = 0;
  private draftSequence = 0;
  private readonly clock: () => number;
  private readonly timers: Timers;
  private readonly autosaveMs: number;
  private readonly validateMs: number;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    readonly id: ScenarioId,
    options: ScenarioEditorOptions = {},
  ) {
    this.clock = options.clock ?? Date.now;
    this.timers = options.timers ?? DEFAULT_TIMERS;
    this.autosaveMs = options.autosaveMs ?? 800;
    this.validateMs = options.validateMs ?? 250;
    this.entities = new EntityOptions(ipc);
    this.execution = new ScenarioRunState(ipc, id, options.events ?? null, this.clock);
    this.palettePinned = options.palettePinned ?? false;
    this.paletteOpen = this.palettePinned;
    makeAutoObservable<
      ScenarioEditor,
      | "ipc"
      | "draftTimer"
      | "validateTimer"
      | "validation"
      | "draftSequence"
      | "clock"
      | "timers"
      | "autosaveMs"
      | "validateMs"
    >(
      this,
      {
        ipc: false,
        id: false,
        entities: false,
        execution: false,
        draftTimer: false,
        validateTimer: false,
        validation: false,
        draftSequence: false,
        clock: false,
        timers: false,
        autosaveMs: false,
        validateMs: false,
      },
      { autoBind: true },
    );
  }

  get graph(): ScenarioGraphDto {
    return this.history.present;
  }

  get catalogue(): Catalogue {
    return catalogueOf(this.types);
  }

  get dirty(): boolean {
    return !sameGraph(this.graph, this.saved);
  }

  get canUndo(): boolean {
    return canUndo(this.history);
  }

  get canRedo(): boolean {
    return canRedo(this.history);
  }

  get palette(): PaletteGroup[] {
    return paletteGroups(this.types, this.tools, this.paletteQuery);
  }

  get selectedNode(): ScenarioNodeDto | null {
    if (this.selection.nodes.length !== 1) return null;
    const [id] = this.selection.nodes;
    return id === undefined ? null : (findNode(this.graph, id) ?? null);
  }

  get issuesByNode(): ReadonlyMap<string, ScenarioIssueDto[]> {
    const map = new Map<string, ScenarioIssueDto[]>();
    for (const issue of this.issues) {
      if (issue.nodeId === undefined) continue;
      map.set(issue.nodeId, [...(map.get(issue.nodeId) ?? []), issue]);
    }
    return map;
  }

  specOf(id: string): ScenarioNodeTypeDto | undefined {
    const node = findNode(this.graph, id);
    return node === undefined ? undefined : this.catalogue.get(node.type);
  }

  labelOf(type: string): string {
    return typeLabel(type, this.catalogue.get(type), this.tools);
  }

  nameOf(id: string): string {
    const node = findNode(this.graph, id);
    if (node === undefined) return id;
    return node.name ?? this.labelOf(node.type);
  }

  issueLines(id: string): string[] {
    return (this.issuesByNode.get(id) ?? []).map((issue) => issueText(issue, this.nameOf));
  }

  ceilingOf(id: string): string | null {
    const node = findNode(this.graph, id);
    const spec = this.specOf(id);
    if (node === undefined || spec === undefined) return null;
    if (node.type === TOOL_SET_TYPE) return toolSetProblem(node.params, this.tools);
    return ceilingReason(spec, this.tools);
  }

  positionOf(node: ScenarioNodeDto): ScenarioPositionDto {
    return this.dragPositions[node.id] ?? node.position;
  }

  async load(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    this.error = null;
    try {
      const [detail, draft, types, tools, servers] = await Promise.all([
        this.ipc.call("scenarios.get", { id: this.id }),
        this.ipc.call("scenarios.draft", { id: this.id }),
        this.ipc.call("scenarios.catalogue", undefined),
        this.ipc.call("tools.list", { includeUnavailable: true }),
        this.ipc.call("mcp.list", undefined),
      ]);
      runInAction(() => {
        this.types = [...types];
        this.tools = [...tools];
        this.servers = [...servers];
        this.adopt(detail);
        if (draft !== null && !sameGraph(draft.graph, detail.graph)) {
          this.history = startHistory(draft.graph);
          this.draftState = "saved";
          this.draftSavedAt = draft.savedAt;
          this.notice = `Восстановлен несохранённый черновик от ${savedAtLabel(draft.savedAt)}`;
          this.scheduleValidation();
        }
        this.loaded = true;
        this.loading = false;
      });
      void this.execution.loadLast();
    } catch (error: unknown) {
      runInAction(() => {
        this.error = scenarioErrorCopy(error, SCENARIO_LOAD_FAILED);
        this.loaded = true;
        this.loading = false;
      });
    }
  }

  private adopt(detail: ScenarioDetailDto): void {
    const { graph, issues, ...scenario } = detail;
    this.scenario = scenario;
    this.saved = graph;
    this.history = startHistory(graph);
    this.issues = [...issues];
    this.selection = NO_SELECTION;
    this.draftState = "clean";
  }

  dismissRefusal(): void {
    this.refusal = null;
  }

  dismissNotice(): void {
    this.notice = null;
  }

  dismissError(): void {
    this.error = null;
  }

  private refuse(refusal: Refusal): void {
    this.refusal = refusalText(refusal, this.nameOf, (id) => this.specOf(id)?.fills ?? []);
  }

  private run(actions: readonly GraphAction[]): readonly string[] | null {
    let graph = this.graph;
    const created: string[] = [];
    for (const action of actions) {
      const result = applyAction(graph, action, this.catalogue);
      if (!result.ok) {
        this.refuse(result.refusal);
        return null;
      }
      graph = result.graph;
      created.push(...result.created);
    }
    this.refusal = null;
    this.mutate(graph);
    return created;
  }

  dispatch(action: GraphAction): boolean {
    return this.run([action]) !== null;
  }

  private mutate(graph: ScenarioGraphDto): void {
    if (graph === this.graph) return;
    this.history = commit(this.history, graph);
    this.changed();
  }

  private changed(): void {
    const live = new Set(this.graph.nodes.map((node) => node.id));
    this.selection = {
      nodes: this.selection.nodes.filter((id) => live.has(id)),
      links: this.selection.links,
    };
    this.scheduleDraft();
    this.scheduleValidation();
  }

  undo(): void {
    if (!this.canUndo) return;
    this.history = undo(this.history);
    this.refusal = null;
    this.changed();
  }

  redo(): void {
    if (!this.canRedo) return;
    this.history = redo(this.history);
    this.refusal = null;
    this.changed();
  }

  setSelection(selection: Selection): void {
    this.selection = selection;
  }

  select(id: string, additive = false): void {
    if (additive) {
      const has = this.selection.nodes.includes(id);
      this.selection = {
        nodes: has
          ? this.selection.nodes.filter((entry) => entry !== id)
          : [...this.selection.nodes, id],
        links: this.selection.links,
      };
      return;
    }
    this.selection = { nodes: [id], links: [] };
  }

  selectAll(): void {
    this.selection = { nodes: this.graph.nodes.map((node) => node.id), links: [] };
  }

  clearSelection(): void {
    this.selection = NO_SELECTION;
    this.closePalette();
  }

  deleteSelection(): void {
    if (this.selection.nodes.length === 0 && this.selection.links.length === 0) return;
    this.dispatch({
      type: "remove",
      nodeIds: this.selection.nodes,
      linkIds: this.selection.links,
    });
    this.selection = NO_SELECTION;
  }

  private focus(id: string): void {
    if (!this.selection.nodes.includes(id)) this.selection = { nodes: [id], links: [] };
  }

  duplicateNode(id: string): void {
    this.focus(id);
    this.duplicateSelection();
  }

  deleteNode(id: string): void {
    this.focus(id);
    this.deleteSelection();
  }

  duplicateSelection(): void {
    const created = this.run([{ type: "duplicate", nodeIds: this.selection.nodes }]);
    if (created !== null && created.length > 0) {
      this.selection = { nodes: created, links: [] };
    }
  }

  dragNodes(positions: Readonly<Record<string, ScenarioPositionDto>>): void {
    this.dragPositions = { ...this.dragPositions, ...positions };
    this.dropTarget = this.slotTarget(positions);
  }

  private slotTarget(positions: Readonly<Record<string, ScenarioPositionDto>>): SlotTarget | null {
    const ids = Object.keys(positions);
    const [id] = ids;
    if (ids.length !== 1 || id === undefined) return null;
    const spec = this.specOf(id);
    const position = positions[id];
    if (spec === undefined || spec.fills.length === 0 || position === undefined) return null;
    return slotUnder(this.graph, this.catalogue, id, position);
  }

  endDrag(positions: Readonly<Record<string, ScenarioPositionDto>>): void {
    const target = this.slotTarget(positions);
    const [childId] = Object.keys(positions);
    this.dragPositions = {};
    this.dropTarget = null;
    const moved = Object.entries(positions).some(([id, position]) => {
      const node = findNode(this.graph, id);
      return (
        node !== undefined && (node.position.x !== position.x || node.position.y !== position.y)
      );
    });
    const actions: GraphAction[] = moved ? [{ type: "moveNodes", positions }] : [];
    if (target !== null && childId !== undefined && target.refusal === null) {
      actions.push({ type: "attach", parentId: target.parentId, childId, slot: target.slot });
    }
    if (target !== null && childId !== undefined && target.refusal !== null) {
      this.run(actions);
      this.refuse({
        kind: "attach",
        reason: target.refusal,
        slot: target.slot,
        childId,
        parentId: target.parentId,
      });
      return;
    }
    if (actions.length > 0) this.run(actions);
  }

  connect(
    source: string,
    sourceHandle: string | null,
    target: string,
    targetHandle: string | null,
  ) {
    const sourceSlot = slotOfHandle(sourceHandle);
    if (sourceSlot !== null) {
      return this.dispatch({ type: "attach", parentId: source, childId: target, slot: sourceSlot });
    }
    const targetSlot = slotOfHandle(targetHandle);
    if (targetSlot !== null) {
      return this.dispatch({ type: "attach", parentId: target, childId: source, slot: targetSlot });
    }
    return this.dispatch({ type: "connect", from: source, to: target });
  }

  reroute(linkId: string, source: string, target: string): boolean {
    return this.dispatch({ type: "reroute", linkId, from: source, to: target });
  }

  updateNode(nodeId: string, patch: NodePatch): void {
    this.dispatch({ type: "updateNode", nodeId, patch });
  }

  setSettings(nodeId: string, settings: ScenarioNodeSettingsDto): void {
    this.updateNode(nodeId, { settings });
  }

  setNotes(nodeId: string, notes: string): void {
    this.updateNode(nodeId, { notes });
  }

  pinInput(nodeId: string, input: Json | null): void {
    this.updateNode(nodeId, { pinnedInput: input });
  }

  toggleDisabled(nodeId: string): void {
    const node = findNode(this.graph, nodeId);
    if (node !== undefined) this.updateNode(nodeId, { disabled: node.disabled !== true });
  }

  inputFor(nodeId: string): Json | undefined {
    const node = findNode(this.graph, nodeId);
    if (node?.pinnedInput !== undefined) return node.pinnedInput;
    return this.observedInput(nodeId);
  }

  async testRun(): Promise<void> {
    await this.execution.start(this.graph);
  }

  async runNode(nodeId: string): Promise<void> {
    await this.execution.runNode(this.graph, nodeId, this.inputFor(nodeId));
  }

  async retryStep(nodeId: string): Promise<void> {
    await this.execution.retryStep(nodeId);
  }

  private observedInput(nodeId: string): Json | undefined {
    const upstream = this.execution.upstreamOf(this.graph, nodeId);
    if (upstream !== undefined) return upstream;
    const input = this.execution.latest(nodeId)?.input;
    if (input === undefined) return undefined;
    const event =
      input !== null && typeof input === "object" && !Array.isArray(input)
        ? input.event
        : undefined;
    return event ?? input;
  }

  pinFromRun(nodeId: string): void {
    const observed = this.observedInput(nodeId);
    if (observed !== undefined) this.pinInput(nodeId, observed);
  }

  async setOverlap(overlap: ScenarioOverlap): Promise<void> {
    if (this.scenario === null || this.overlapSaving || this.scenario.overlap === overlap) return;
    this.overlapSaving = true;
    this.error = null;
    try {
      const detail = await this.ipc.call("scenarios.update", { id: this.id, overlap });
      runInAction(() => {
        if (this.scenario !== null) {
          this.scenario = {
            ...this.scenario,
            overlap: detail.overlap,
            updatedAt: detail.updatedAt,
          };
        }
        this.overlapSaving = false;
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = scenarioErrorCopy(error);
        this.overlapSaving = false;
      });
    }
  }

  tidy(): void {
    this.dispatch({ type: "replace", graph: tidy(this.graph, this.catalogue) });
  }

  openPalette(): void {
    this.paletteOpen = true;
    this.paletteQuery = "";
  }

  closePalette(force = false): void {
    if (this.palettePinned && !force) return;
    this.paletteOpen = false;
  }

  togglePalette(): void {
    if (this.paletteOpen) this.closePalette(true);
    else this.openPalette();
  }

  togglePalettePin(): void {
    this.palettePinned = !this.palettePinned;
    if (this.palettePinned) this.paletteOpen = true;
  }

  setPaletteQuery(query: string): void {
    this.paletteQuery = query;
  }

  addStep(type: string, fallback: ScenarioPositionDto): boolean {
    const spec = this.catalogue.get(type);
    if (spec === undefined) return false;
    const blocked = ceilingReason(spec, this.tools);
    if (blocked !== null) {
      this.refusal = blocked;
      return false;
    }
    const id = freshNodeId(this.graph, type, new Set());
    const anchor = this.selectedNode;
    const actions: GraphAction[] = [];
    let position = fallback;
    const anchorSpec = anchor === null ? undefined : this.catalogue.get(anchor.type);
    const params: ScenarioParams = type === TOOL_SET_TYPE ? { [TOOL_SET_PARAM]: [] } : {};
    const node: ScenarioNodeDto = { id, type, position, params };
    const probe: ScenarioGraphDto = { ...this.graph, nodes: [...this.graph.nodes, node] };
    const slot =
      anchor === null || anchorSpec === undefined
        ? undefined
        : anchorSpec.slots.find(
            (entry) => attachRefusal(probe, this.catalogue, anchor.id, id, entry.slot) === null,
          );
    if (anchor !== null && anchorSpec !== undefined && slot !== undefined) {
      const offset = slotOffset(anchorSpec, slot.slot) ?? 0;
      position = {
        x: anchor.position.x + offset - nodeSize(spec).width / 2,
        y: anchor.position.y + nodeSize(anchorSpec).height + SLOT_DROP,
      };
      actions.push(
        { type: "addNode", node: { ...node, position } },
        { type: "attach", parentId: anchor.id, childId: id, slot: slot.slot },
      );
    } else if (
      anchor !== null &&
      spec.role !== "subNode" &&
      isTrunk(this.catalogue, anchor) &&
      connectRefusal(probe, this.catalogue, anchor.id, id) === null
    ) {
      position = {
        x: anchor.position.x + nodeSize(anchorSpec).width + STEP_GAP,
        y: anchor.position.y,
      };
      actions.push(
        { type: "addNode", node: { ...node, position } },
        { type: "connect", from: anchor.id, to: id },
      );
    } else {
      actions.push({ type: "addNode", node });
    }
    const created = this.run(actions);
    if (created === null) return false;
    this.selection = { nodes: [id], links: [] };
    this.closePalette();
    return true;
  }

  private scheduleDraft(): void {
    if (this.draftTimer !== undefined) this.timers.clear(this.draftTimer);
    this.draftState = "pending";
    this.draftTimer = this.timers.set(() => {
      this.draftTimer = undefined;
      void this.flushDraft();
    }, this.autosaveMs);
  }

  async flushDraft(): Promise<void> {
    if (this.draftTimer !== undefined) {
      this.timers.clear(this.draftTimer);
      this.draftTimer = undefined;
    }
    if (this.draftState !== "pending" && this.draftState !== "failed") return;
    const sequence = ++this.draftSequence;
    const graph = this.graph;
    this.draftState = "saving";
    try {
      const draft = await this.ipc.call("scenarios.saveDraft", { id: this.id, graph });
      runInAction(() => {
        if (sequence !== this.draftSequence) return;
        this.draftSavedAt = draft.savedAt;
        this.draftState = sameGraph(graph, this.graph) ? "saved" : "pending";
      });
    } catch {
      runInAction(() => {
        if (sequence !== this.draftSequence) return;
        this.draftState = "failed";
        this.error = DRAFT_SAVE_FAILED;
      });
    }
  }

  private scheduleValidation(): void {
    if (this.validateTimer !== undefined) this.timers.clear(this.validateTimer);
    this.validateTimer = this.timers.set(() => {
      this.validateTimer = undefined;
      void this.validate();
    }, this.validateMs);
  }

  async validate(): Promise<void> {
    const sequence = ++this.validation;
    try {
      const result = await this.ipc.call("scenarios.validate", { graph: this.graph });
      runInAction(() => {
        if (sequence === this.validation) this.issues = [...result.issues];
      });
    } catch {
      return;
    }
  }

  async save(note?: string): Promise<boolean> {
    if (this.saving) return false;
    this.saving = true;
    this.error = null;
    if (this.draftTimer !== undefined) this.timers.clear(this.draftTimer);
    this.draftTimer = undefined;
    this.draftSequence += 1;
    const graph = this.graph;
    const trimmed = note?.trim() ?? "";
    try {
      const detail = await this.ipc.call("scenarios.update", {
        id: this.id,
        graph,
        ...(trimmed.length > 0 ? { note: trimmed } : {}),
      });
      runInAction(() => {
        const { graph: savedGraph, issues, ...scenario } = detail;
        this.scenario = scenario;
        this.saved = savedGraph;
        this.issues = [...issues];
        this.draftState = sameGraph(this.graph, savedGraph) ? "clean" : "pending";
        this.notice = null;
        this.saving = false;
      });
      if (this.draftState === "pending") this.scheduleDraft();
      return true;
    } catch (error: unknown) {
      runInAction(() => {
        this.error = scenarioErrorCopy(error);
        const issues = refusalIssues(error);
        if (issues.length > 0) this.issues = issues;
        this.draftState = "pending";
        this.saving = false;
      });
      this.scheduleDraft();
      return false;
    }
  }

  async setActive(active: boolean): Promise<void> {
    if (this.switching) return;
    this.switching = true;
    this.error = null;
    try {
      const scenario = await this.ipc.call("scenarios.activate", { id: this.id, active });
      runInAction(() => {
        this.scenario = scenario;
        this.switching = false;
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = scenarioErrorCopy(error);
        const issues = refusalIssues(error);
        if (issues.length > 0 && !this.dirty) this.issues = issues;
        this.switching = false;
      });
    }
  }

  async discardDraft(): Promise<void> {
    if (this.draftTimer !== undefined) this.timers.clear(this.draftTimer);
    this.draftTimer = undefined;
    this.draftSequence += 1;
    try {
      await this.ipc.call("scenarios.discardDraft", { id: this.id });
      runInAction(() => {
        this.history = commit(this.history, this.saved);
        this.draftState = "clean";
        this.draftSavedAt = null;
        this.notice = null;
        this.selection = NO_SELECTION;
      });
      this.scheduleValidation();
    } catch (error: unknown) {
      runInAction(() => {
        this.error = scenarioErrorCopy(error);
      });
    }
  }

  dispose(): void {
    this.execution.dispose();
    if (this.validateTimer !== undefined) this.timers.clear(this.validateTimer);
    this.validateTimer = undefined;
    if (this.draftState === "pending") void this.flushDraft();
  }
}

export const SLOT_HANDLE_PREFIX = "slot:";

export function slotHandle(slot: ScenarioSlot): string {
  return `${SLOT_HANDLE_PREFIX}${slot}`;
}

export function slotOfHandle(handle: string | null | undefined): ScenarioSlot | null {
  if (handle?.startsWith(SLOT_HANDLE_PREFIX) !== true) return null;
  const slot = handle.slice(SLOT_HANDLE_PREFIX.length);
  return slot === "model" || slot === "memory" || slot === "tool" || slot === "embedding"
    ? slot
    : null;
}
