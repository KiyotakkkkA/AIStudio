import { setMaxListeners } from "node:events";
import {
  AppError,
  AppErrorCode,
  HostEvent,
  RunId,
  type ApprovalRequestDto,
  type GraphNode,
  type HostEventDraft,
  type RunDto,
  type RunOutcomeDto,
} from "@zvs/shared";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import type { RunEntity, StepEntity } from "../data/schema/index.ts";
import type { StreamHandle } from "../platform/events.ts";
import type { Logger } from "../platform/logger.ts";
import { createId } from "../platform/ids.ts";
import type { NodeRegistry } from "./NodeRegistry.ts";
import type {
  ApprovalDecision,
  KernelServices,
  PermissionRequirement,
  StepContext,
} from "./types.ts";

export interface SchedulerOptions {
  data: UnitOfWork;
  registry: NodeRegistry;
  services: KernelServices;
  clock: () => number;
  graceMs: number;
  logger?: Logger;
  approval?: (request: ApprovalRequestDto, signal: AbortSignal) => Promise<ApprovalDecision>;
  admit?: (permission: PermissionRequirement, context: StepContext) => Promise<void>;
}
const failureText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);
const aborted = Symbol("aborted");
const abandoned = Symbol("abandoned");

export class Scheduler {
  readonly controller = new AbortController();
  private stopping = false;
  private pendingApprovals = 0;
  private terminal = false;
  private readonly active = new Map<string, Promise<void>>();
  private readonly completed = new Map<string, StepEntity>();
  private readonly attempts = new Map<string, number>();
  private readonly latest = new Map<string, StepEntity>();
  private readonly readyAt = new Map<string, number>();
  private failure: string | undefined;
  private failureCode: AppErrorCode = AppErrorCode.RUN_FAILED;

  constructor(
    private readonly run: RunEntity,
    private readonly stream: StreamHandle,
    private readonly options: SchedulerOptions,
  ) {
    setMaxListeners(run.concurrency * 3 + 10, this.controller.signal);
  }

  cancel(): void {
    this.controller.abort();
  }
  suspend(): void {
    this.stopping = true;
    this.controller.abort();
  }

  private emit(event: HostEventDraft): void {
    if (!this.terminal) this.stream.emit(event);
  }
  private status(status: RunDto["status"]): void {
    this.options.data.transaction(({ runs }) => {
      runs.update(this.run.id, { status });
      this.run.status = status;
      this.emit({ type: "log", line: { runId: this.run.id, status } });
    });
  }
  private progress(): void {
    this.emit({ type: "progress", done: this.completed.size, total: this.run.graph.nodes.length });
  }

  finish(
    status: "succeeded" | "failed" | "cancelled" | "interrupted",
    error?: string,
    code = this.failureCode,
  ): void {
    if (this.terminal) return;
    const outcome: RunOutcomeDto =
      status === "succeeded"
        ? { status: "ok" }
        : {
            status: status === "cancelled" ? "cancelled" : "failed",
            code: status === "cancelled" ? AppErrorCode.RUN_CANCELLED : code,
            message:
              error ?? (status === "interrupted" ? "Выполнение прервано" : "Выполнение отменено"),
          };
    this.options.data.transaction(({ runs }) => {
      runs.update(this.run.id, {
        status,
        outcome,
        error: error ?? null,
        finishedAt: this.options.clock(),
      });
      this.emit({ type: "log", line: { runId: this.run.id, status } });
      this.stream.end(outcome);
    });
    this.terminal = true;
  }

  async execute(): Promise<void> {
    try {
      for (const step of this.options.data.repositories.runs.steps(this.run.id)) {
        if (step.attempt > (this.latest.get(step.nodeId)?.attempt ?? 0))
          this.latest.set(step.nodeId, step);
        this.attempts.set(step.nodeId, Math.max(this.attempts.get(step.nodeId) ?? 0, step.attempt));
        if (step.status === "succeeded") this.completed.set(step.nodeId, step);
        if (step.status === "failed") {
          const node = this.run.graph.nodes.find((node) => node.id === step.nodeId);
          if (node)
            this.readyAt.set(
              step.nodeId,
              (step.finishedAt ?? 0) + node.retry.backoffMs * step.attempt,
            );
        }
      }
      this.options.registry.validate(this.run.graph);
      this.options.data.repositories.runs.update(this.run.id, {
        startedAt: this.run.startedAt ?? this.options.clock(),
      });
      this.status("running");
      this.progress();
      const nodes = [...this.run.graph.nodes].sort((a, b) =>
        a.id < b.id ? -1 : a.id > b.id ? 1 : 0,
      );
      while (!this.controller.signal.aborted && this.completed.size < nodes.length) {
        let next = Infinity;
        for (const node of nodes) {
          if (this.pendingApprovals) break;
          if (this.active.size >= this.run.concurrency) break;
          if (
            this.completed.has(node.id) ||
            this.active.has(node.id) ||
            !node.dependencies.every((id) => this.completed.has(id))
          )
            continue;
          const previous = this.latest.get(node.id);
          if (previous?.status === "failed" && previous.attempt >= node.retry.maxAttempts) {
            this.failure = previous.error ?? "Попытки исчерпаны";
            this.controller.abort();
            break;
          }
          const due = this.readyAt.get(node.id) ?? 0;
          if (due > this.options.clock()) {
            next = Math.min(next, due);
            continue;
          }
          const work = this.executeNode(node).finally(() => this.active.delete(node.id));
          this.active.set(node.id, work);
        }
        if (this.controller.signal.aborted) break;
        if (this.active.size) {
          if (Number.isFinite(next)) {
            const timer = this.timer(Math.max(0, next - this.options.clock()));
            await Promise.race([...this.active.values(), timer.promise]);
            timer.dispose();
          } else await Promise.race(this.active.values());
        } else if (Number.isFinite(next)) {
          const timer = this.timer(Math.max(0, next - this.options.clock()));
          await timer.promise;
          timer.dispose();
        } else if (this.completed.size < nodes.length) throw new Error("Нет доступных узлов");
      }
      await Promise.all(this.active.values());
      if (this.stopping) return;
      if (this.failure) this.finish("failed", this.failure);
      else if (this.controller.signal.aborted) this.finish("cancelled");
      else this.finish("succeeded");
    } catch (error) {
      this.controller.abort();
      await Promise.allSettled(this.active.values());
      this.options.logger?.log("error", "kernel", "Run failed", {
        runId: this.run.id,
        error: failureText(error),
      });
      if (!this.stopping) this.finish("failed", failureText(error));
    }
  }

  private timer(ms: number) {
    let resolve!: () => void;
    const promise = new Promise<void>((done) => {
      resolve = done;
    });
    const timer = setTimeout(() => resolve(), ms);
    const signal = this.controller.signal;
    signal.addEventListener("abort", resolve, { once: true });
    if (signal.aborted) resolve();
    return {
      promise,
      dispose: () => {
        clearTimeout(timer);
        signal.removeEventListener("abort", resolve);
      },
    };
  }

  private async executeNode(node: GraphNode): Promise<void> {
    const { data, clock, registry } = this.options;
    const signal = this.controller.signal;
    const attempt = (this.attempts.get(node.id) ?? 0) + 1;
    this.attempts.set(node.id, attempt);
    const input = Object.keys(node.bindings).length
      ? Object.fromEntries([
          ...Object.entries(node.input as Record<string, RunDto["input"]>),
          ...Object.entries(node.bindings).map(([key, binding]) => [
            key,
            binding.source === "run" ? this.run.input : this.completed.get(binding.nodeId)!.output,
          ]),
        ])
      : node.input;
    const step: StepEntity = {
      id: createId(),
      runId: this.run.id,
      nodeId: node.id,
      type: node.type,
      status: "running",
      input,
      output: null,
      error: null,
      startedAt: clock(),
      finishedAt: null,
      attempt,
    };
    data.transaction(({ runs }) => {
      runs.addStep(step);
      this.emit({ type: "step", step: { ...step } });
    });
    let live = true;
    const work = Promise.resolve()
      .then(async () => {
        signal.throwIfAborted();
        const context: StepContext = {
          runId: RunId.parse(this.run.id),
          signal,
          services: this.options.services,
          emit: (event) => {
            if (!live || signal.aborted) return;
            const parsed = HostEvent.parse({
              ...event,
              streamId: this.run.streamId,
              seq: 0,
              ts: clock(),
            });
            if (parsed.type === "end") throw new Error("Only the kernel can end a run");
            this.emit(parsed);
          },
          requestApproval: async (request) => {
            if (!live || signal.aborted)
              throw new AppError(AppErrorCode.RUN_CANCELLED, "Выполнение отменено");
            if (!this.options.approval)
              throw new AppError(AppErrorCode.APPROVAL_DENIED, "Сервис согласования недоступен");
            this.pendingApprovals++;
            this.status("blocked");
            this.emit({ type: "approval", request });
            try {
              return await this.options.approval(request, signal);
            } finally {
              this.pendingApprovals--;
              if (live && !signal.aborted && !this.pendingApprovals) this.status("running");
            }
          },
        };
        const definition = registry.get(node.type);
        if (this.options.admit) await this.options.admit(definition.permission, context);
        else if (!("kind" in definition.permission))
          throw new AppError(AppErrorCode.PERMISSION_DENIED, "Сервис разрешений недоступен");
        signal.throwIfAborted();
        return definition.execute(context, input);
      })
      .then(
        (output) => ({ output }),
        (error: unknown) => ({ error }),
      );
    let onAbort!: () => void;
    const cancellation = new Promise<typeof aborted>((resolve) => {
      onAbort = () => resolve(aborted);
    });
    signal.addEventListener("abort", onAbort, { once: true });
    if (signal.aborted) onAbort();
    try {
      let result: Awaited<typeof work> | typeof aborted | typeof abandoned = await Promise.race([
        work,
        cancellation,
      ]);
      if (result === aborted) {
        let timer!: ReturnType<typeof setTimeout>;
        result = await Promise.race([
          work,
          new Promise<typeof abandoned>((resolve) => {
            timer = setTimeout(() => resolve(abandoned), this.options.graceMs);
          }),
        ]);
        clearTimeout(timer);
      }
      live = false;
      step.finishedAt = clock();
      this.latest.set(node.id, step);
      if (result === abandoned) {
        step.status = "abandoned";
        step.error = "Узел не остановился после отмены";
        this.options.logger?.log("error", "kernel", "Abandoned node ignored cancellation", {
          runId: this.run.id,
          nodeId: node.id,
          attempt,
        });
        this.emit({ type: "log", line: { level: "error", message: step.error, nodeId: node.id } });
      } else if (signal.aborted) {
        step.status = "cancelled";
        step.error = "Выполнение отменено";
      } else if ("error" in result) {
        step.status = "failed";
        step.error = failureText(result.error);
        this.readyAt.set(node.id, clock() + node.retry.backoffMs * attempt);
        const permissionFailure =
          result.error instanceof AppError &&
          (result.error.code === AppErrorCode.APPROVAL_DENIED ||
            result.error.code === AppErrorCode.PERMISSION_DENIED);
        if (permissionFailure)
          this.failureCode =
            result.error instanceof AppError ? result.error.code : AppErrorCode.RUN_FAILED;
        if (permissionFailure || attempt >= node.retry.maxAttempts) {
          this.failure = step.error;
          this.controller.abort();
        }
      } else {
        step.status = "succeeded";
        step.output = result.output;
        this.completed.set(node.id, step);
      }
      data.transaction(({ runs }) => {
        runs.updateStep(step.id, step);
        this.emit({ type: "step", step: { ...step } });
        this.progress();
      });
    } finally {
      live = false;
      signal.removeEventListener("abort", onAbort);
    }
  }
}
