import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type {
  Contract,
  Json,
  RunHandleDto,
  RunId,
  ScenarioGraphDto,
  ScenarioId,
} from "@zvs/shared";
import type { EventRouter, RoutedEvent } from "../../app/EventRouter";
import {
  edgeStates,
  failedStep,
  finishView,
  latestSteps,
  logFromLine,
  nodeStates,
  phaseOf,
  startView,
  stepFromDto,
  stepFromEvent,
  stepRows,
  upstreamOutput,
  withLog,
  withStep,
  type EdgeRunState,
  type NodeRunState,
  type RunStep,
  type RunTab,
  type RunView,
  type StepRow,
} from "./runModel";
import { RUN_CANCEL_FAILED, RUN_LOAD_FAILED, runErrorCopy } from "./runCopy";

const OUTCOME_PHASES = { ok: "succeeded", failed: "failed", cancelled: "cancelled" } as const;

export const STRIP_HEIGHT_DEFAULT = 186;
export const STRIP_HEIGHT_MIN = 120;
export const STRIP_HEIGHT_COLLAPSED = 38;

export default class ScenarioRunState {
  view: RunView | null = null;
  selectedStepId: string | null = null;
  tab: RunTab = "input";
  stripOpen = true;
  stripHeight = STRIP_HEIGHT_DEFAULT;
  starting = false;
  cancelling = false;
  error: string | null = null;
  loaded = false;

  private detach: (() => void) | null = null;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly scenarioId: ScenarioId,
    private readonly events: EventRouter | null,
    private readonly clock: () => number,
  ) {
    makeAutoObservable<ScenarioRunState, "ipc" | "scenarioId" | "events" | "clock" | "detach">(
      this,
      { ipc: false, scenarioId: false, events: false, clock: false, detach: false },
      { autoBind: true },
    );
  }

  get running(): boolean {
    return this.starting || this.view?.phase === "running";
  }

  get rows(): StepRow[] {
    return stepRows(this.view);
  }

  get selected(): RunStep | null {
    if (this.view === null || this.selectedStepId === null) return null;
    return this.view.steps.find((step) => step.id === this.selectedStepId) ?? null;
  }

  get nodes(): ReadonlyMap<string, NodeRunState> {
    return nodeStates(this.view);
  }

  edges(graph: ScenarioGraphDto): ReadonlyMap<string, EdgeRunState> {
    return edgeStates(graph, this.view);
  }

  upstreamOf(graph: ScenarioGraphDto, nodeId: string): Json | undefined {
    return upstreamOutput(graph, this.view, nodeId);
  }

  latest(nodeId: string): RunStep | null {
    return this.view === null ? null : (latestSteps(this.view).get(nodeId) ?? null);
  }

  select(stepId: string | null): void {
    this.selectedStepId = stepId;
  }

  setTab(tab: RunTab): void {
    this.tab = tab;
  }

  toggleStrip(): void {
    this.stripOpen = !this.stripOpen;
  }

  setStripHeight(height: number, limit: number = Number.POSITIVE_INFINITY): void {
    const ceiling = Math.max(STRIP_HEIGHT_MIN, limit);
    this.stripHeight = Math.round(Math.min(ceiling, Math.max(STRIP_HEIGHT_MIN, height)));
    this.stripOpen = true;
  }

  dismissError(): void {
    this.error = null;
  }

  async loadLast(): Promise<void> {
    try {
      const page = await this.ipc.call("runs.list", {
        kinds: ["scenario"],
        subjectId: this.scenarioId,
        live: false,
        limit: 1,
      });
      const [latest] = page.items;
      if (latest === undefined) {
        runInAction(() => {
          this.loaded = true;
        });
        return;
      }
      const detail = await this.ipc.call("runs.detail", { id: latest.id });
      runInAction(() => {
        if (this.view !== null) return;
        const { run } = detail;
        let view = startView(run.id, run.streamId, run.startedAt ?? run.createdAt);
        for (const step of detail.steps) view = withStep(view, stepFromDto(step));
        for (const line of detail.logs) {
          const log = logFromLine(line);
          if (log !== null) view = withLog(view, log);
        }
        view = { ...view, retryOf: run.retryOfId ?? null };
        const phase = phaseOf(run.status);
        if (phase !== "running") {
          view = { ...view, phase, finishedAt: run.finishedAt ?? null, error: run.error ?? null };
        }
        this.view = view;
        this.loaded = true;
        this.focusFailure();
        if (phase === "running") this.listen(run.streamId);
      });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = runErrorCopy(error, RUN_LOAD_FAILED);
        this.loaded = true;
      });
    }
  }

  async start(graph: ScenarioGraphDto): Promise<boolean> {
    return this.launch(() => this.ipc.call("scenarios.run", { id: this.scenarioId, graph }));
  }

  async runNode(
    graph: ScenarioGraphDto,
    nodeId: string,
    input: Json | undefined,
  ): Promise<boolean> {
    return this.launch(() =>
      this.ipc.call("scenarios.runNode", {
        id: this.scenarioId,
        graph,
        nodeId,
        ...(input === undefined ? {} : { input }),
      }),
    );
  }

  async retryStep(nodeId: string): Promise<boolean> {
    const previous = this.view;
    if (previous === null) return false;
    const kept = previous.steps.filter(
      (step) => step.status === "succeeded" && step.nodeId !== nodeId,
    );
    return this.launch(
      () => this.ipc.call("scenarios.retryStep", { runId: previous.runId as RunId, nodeId }),
      kept,
      previous.runId,
    );
  }

  async cancel(): Promise<void> {
    const view = this.view;
    if (view === null || view.phase !== "running" || this.cancelling) return;
    this.cancelling = true;
    try {
      await this.ipc.call("runs.cancel", { id: view.runId as RunId });
    } catch (error: unknown) {
      runInAction(() => {
        this.error = runErrorCopy(error, RUN_CANCEL_FAILED);
      });
    } finally {
      runInAction(() => {
        this.cancelling = false;
      });
    }
  }

  private async launch(
    call: () => Promise<RunHandleDto>,
    seed: readonly RunStep[] = [],
    retryOf: string | null = null,
  ): Promise<boolean> {
    if (this.running) return false;
    this.starting = true;
    this.error = null;
    try {
      const handle = await call();
      runInAction(() => {
        this.stop();
        let view = { ...startView(handle.id, handle.streamId, this.clock()), retryOf };
        for (const step of seed) view = withStep(view, step);
        this.view = view;
        this.selectedStepId = null;
        this.starting = false;
        this.listen(handle.streamId);
      });
      return true;
    } catch (error: unknown) {
      runInAction(() => {
        this.error = runErrorCopy(error);
        this.starting = false;
      });
      return false;
    }
  }

  private listen(streamId: string): void {
    this.stop();
    if (this.events === null) return;
    this.detach = this.events.subscribe(streamId as RunHandleDto["streamId"], this.apply);
  }

  apply(event: RoutedEvent): void {
    const view = this.view;
    if (view === null || event.streamId !== view.streamId) return;
    if (event.type === "step") {
      const step = stepFromEvent(event.step);
      if (step !== null) this.view = withStep(view, step);
      return;
    }
    if (event.type === "log") {
      const log = logFromLine(event.line);
      if (log !== null) this.view = withLog(view, log);
      return;
    }
    if (event.type === "end") {
      this.view = finishView(
        view,
        OUTCOME_PHASES[event.outcome.status],
        this.clock(),
        event.outcome.message ?? null,
      );
      this.stop();
      this.focusFailure();
    }
  }

  private focusFailure(): void {
    const failed = failedStep(this.view);
    if (failed === null || this.view?.phase !== "failed") return;
    this.selectedStepId = failed.id;
    this.tab = "logs";
    this.stripOpen = true;
  }

  private stop(): void {
    this.detach?.();
    this.detach = null;
  }

  dispose(): void {
    this.stop();
  }
}
