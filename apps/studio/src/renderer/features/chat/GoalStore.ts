import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  ChatGoalStepDto,
  type AgenticPlanStepDto,
  type ChatGoalDto,
  type Contract,
  type ConversationDto,
  type ConversationId,
  type RunHandleDto,
} from "@zvs/shared";
import { GOAL_CONTINUE_FAILED, GOAL_STOP_FAILED } from "./goalCopy";

export interface GoalSource {
  readonly conversation: () => ConversationDto | null;
  readonly refresh: (id: ConversationId) => Promise<void>;
  readonly replace: (conversation: ConversationDto) => void;
}

export default class GoalStore {
  live: ChatGoalStepDto | null = null;
  receivedAt = 0;
  busy = false;
  error: string | null = null;
  dismissed: string | null = null;
  private refreshing: ConversationId | null = null;

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly source: GoalSource,
    private readonly clock: () => number = Date.now,
  ) {
    makeAutoObservable<GoalStore, "ipc" | "source" | "clock" | "refreshing">(
      this,
      { ipc: false, source: false, clock: false, refreshing: false },
      { autoBind: true },
    );
  }

  get conversationId(): ConversationId | null {
    return this.source.conversation()?.id ?? null;
  }

  get goal(): ChatGoalDto | null {
    const base = this.source.conversation()?.goal ?? null;
    const live = this.live;
    if (base === null || live === null || live.conversationId !== this.conversationId) return base;
    if (live.round < base.usage.rounds) return base;
    if (base.status !== "active" && live.status !== "active") return base;
    return {
      ...base,
      status: live.status,
      reason: live.reason,
      usage: live.usage,
      budget: live.budget,
      plan: live.plan.length > 0 ? live.plan : base.plan,
    };
  }

  get active(): boolean {
    return this.goal?.status === "active";
  }

  get paused(): boolean {
    return this.goal?.status === "paused";
  }

  get plan(): readonly AgenticPlanStepDto[] {
    return this.goal?.plan ?? [];
  }

  get planDone(): number {
    return this.plan.filter((step) => step.status === "done").length;
  }

  get visible(): boolean {
    const goal = this.goal;
    if (goal === null) return false;
    return goal.status !== "done" || this.dismissed !== this.key(goal);
  }

  get elapsedMs(): number {
    const goal = this.goal;
    if (goal === null) return 0;
    const running = this.active && this.live !== null ? this.clock() - this.receivedAt : 0;
    return goal.usage.elapsedMs + Math.max(0, running);
  }

  apply(step: Record<string, unknown>): boolean {
    const parsed = ChatGoalStepDto.safeParse(step);
    if (!parsed.success) return false;
    const conversationId = this.conversationId;
    if (parsed.data.conversationId !== conversationId) return true;
    this.live = parsed.data;
    this.receivedAt = this.clock();
    const base = this.source.conversation()?.goal ?? null;
    if (parsed.data.status === "active" && (base === null || base.status !== "active"))
      void this.refresh(parsed.data.conversationId);
    return true;
  }

  reset() {
    this.live = null;
    this.error = null;
  }

  dismiss() {
    const goal = this.goal;
    if (goal !== null) this.dismissed = this.key(goal);
  }

  async stop() {
    const id = this.conversationId;
    if (id === null || this.busy) return;
    this.busy = true;
    this.error = null;
    try {
      const conversation = await this.ipc.call("chat.goal.stop", { conversationId: id });
      runInAction(() => {
        this.live = null;
        this.source.replace(conversation);
      });
    } catch {
      runInAction(() => {
        this.error = GOAL_STOP_FAILED;
      });
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }

  async resume(): Promise<RunHandleDto | null> {
    const id = this.conversationId;
    if (id === null || this.busy) return null;
    this.busy = true;
    this.error = null;
    try {
      const run = await this.ipc.call("chat.goal.continue", { conversationId: id });
      runInAction(() => {
        this.live = null;
      });
      await this.source.refresh(id);
      return run;
    } catch {
      runInAction(() => {
        this.error = GOAL_CONTINUE_FAILED;
      });
      return null;
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }

  private async refresh(id: ConversationId) {
    if (this.refreshing === id) return;
    this.refreshing = id;
    try {
      await this.source.refresh(id);
    } finally {
      this.refreshing = null;
    }
  }

  private key(goal: ChatGoalDto): string {
    return `${this.conversationId ?? ""}:${String(goal.startedAt)}`;
  }
}
