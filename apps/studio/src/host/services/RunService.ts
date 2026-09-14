import { ApprovalService } from "./ApprovalService.ts";
import { PermissionService } from "./PermissionService.ts";
import {
  AppError,
  AppErrorCode,
  RunDto,
  RunDetailDto,
  RunId,
  RunKind,
  RunListFilter,
  RunPageDto,
  RunStatus,
  RunSummaryDto,
  StartRunInput,
  StepDto,
  StreamId,
  type RunCountsDto,
  type RunHandleDto,
  type HostEvent,
} from "@zvs/shared";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import type { RunEntity } from "../data/schema/index.ts";
import { encodeRunCursor } from "../data/repositories/RunRepository.ts";
import { NodeRegistry } from "../kernel/NodeRegistry.ts";
import { Scheduler, type SchedulerOptions } from "../kernel/Scheduler.ts";
import type { EventBus, StreamHandle } from "../platform/events.ts";
import { createId } from "../platform/ids.ts";

export interface RunServiceOptions extends Partial<Omit<SchedulerOptions, "data" | "registry">> {
  approvalTimeoutMs?: number;
  attentionWindowMs?: number;
  data: UnitOfWork;
  events: EventBus;
  registry: NodeRegistry;
}

const ATTENTION_WINDOW_MS = 24 * 60 * 60 * 1000;
const UNFINISHED: readonly RunDto["status"][] = ["queued", "running", "blocked"];
export class RunService {
  private readonly observers = new Map<string, Set<(event: HostEvent) => void>>();
  private readonly active = new Map<string, { scheduler: Scheduler; done: Promise<void> }>();
  private readonly options: SchedulerOptions;
  private closed = false;
  readonly permissions: PermissionService;
  private readonly approvals: ApprovalService;
  constructor(private readonly dependencies: RunServiceOptions) {
    const graceMs = dependencies.graceMs ?? 500;
    if (!Number.isInteger(graceMs) || graceMs < 0 || graceMs > 30_000)
      throw new Error("Invalid cancellation grace period");
    this.permissions = new PermissionService(
      dependencies.data,
      dependencies.registry,
      dependencies.clock,
    );
    this.approvals = new ApprovalService(
      dependencies.data,
      this.permissions,
      dependencies.approvalTimeoutMs,
      dependencies.clock,
    );
    this.options = {
      ...dependencies,
      clock: dependencies.clock ?? Date.now,
      graceMs,
      services: dependencies.services ?? {},
      approval:
        dependencies.approval ?? ((request, signal) => this.approvals.wait(request, signal)),
      admit:
        dependencies.admit ??
        (async (requirement, context) => {
          if ("kind" in requirement) return;
          const kind = this.get(context.runId).kind;
          if (
            kind === "chat" &&
            (requirement.tool === "llm.generate" || requirement.tool === "vector.search")
          )
            return;
          if (kind === "indexing" && requirement.tool === "vector.index") return;
          const run = this.get(context.runId);
          const scopes = [
            ...(run.graph.permissionScopes ?? []),
            ...(run.kind === "scenario" && run.subjectId ? [`scenario:${run.subjectId}`] : []),
          ];
          const admission = this.permissions.admitSubject(
            requirement.tool,
            { runId: run.id, scopes },
            requirement.tier,
          );
          if (admission === "deny")
            throw new AppError(AppErrorCode.PERMISSION_DENIED, "Permission denied");
          if (admission === "ask") {
            const request = this.approvals.create(run.id, requirement.tool, scopes[0] ?? "global");
            const decision = await context.requestApproval(request);
            if (decision !== "approved")
              throw new AppError(AppErrorCode.APPROVAL_DENIED, "Approval denied");
            if (
              this.permissions.admitSubject(
                requirement.tool,
                { runId: run.id, scopes },
                requirement.tier,
              ) === "deny"
            )
              throw new AppError(AppErrorCode.PERMISSION_DENIED, "Permission revoked");
          }
        }),
    };
  }
  approve(id: string, approvalId: string, always = false): void {
    this.approvals.decide(id, approvalId, "approved", always);
  }
  deny(id: string, approvalId: string): void {
    this.approvals.decide(id, approvalId, "denied");
  }
  start(raw: StartRunInput): RunHandleDto {
    if (this.closed) throw new AppError(AppErrorCode.CONFLICT, "Сервис выполнения остановлен");
    const input = StartRunInput.parse(raw);
    this.dependencies.registry.validate(input.graph);
    const id = RunId.parse(createId());
    const streamId = StreamId.parse(createId());
    const run = this.dependencies.data.repositories.runs.create({
      ...input,
      id,
      streamId,
      status: "queued",
      createdAt: this.options.clock(),
    });
    const stream = this.openStream(run);
    stream.emit({ type: "log", line: { runId: id, status: "queued" } });
    this.launch(run, stream);
    return { id, streamId };
  }
  private openStream(run: RunEntity): StreamHandle {
    const runs = this.dependencies.data.repositories.runs;
    return this.dependencies.events.openStream({
      id: StreamId.parse(run.streamId),
      nextSeq: runs.nextSequence(run.id),
      record: (event) => {
        runs.appendEvent(run.id, event);
        for (const observer of this.observers.get(run.id) ?? []) observer(event);
        if (event.type === "end") this.observers.delete(run.id);
      },
    });
  }
  subscribe(id: string, observer: (event: HostEvent) => void): () => void {
    this.get(id);
    const observers = this.observers.get(id) ?? new Set<(event: HostEvent) => void>();
    observers.add(observer);
    this.observers.set(id, observers);
    return () => {
      observers.delete(observer);
      if (!observers.size) this.observers.delete(id);
    };
  }
  private launch(run: RunEntity, stream: StreamHandle): void {
    const scheduler = new Scheduler(run, stream, this.options);
    const done = new Promise<void>((resolve) => setImmediate(resolve))
      .then(() => scheduler.execute())
      .catch((error: unknown) => {
        this.options.logger?.log("error", "kernel", "Could not persist run failure", {
          runId: run.id,
          error: String(error),
        });
      })
      .finally(() => this.active.delete(run.id));
    this.active.set(run.id, { scheduler, done });
  }
  async cancel(id: string): Promise<void> {
    this.get(id);
    const active = this.active.get(id);
    active?.scheduler.cancel();
    await active?.done;
  }
  async wait(id: string): Promise<void> {
    await this.active.get(id)?.done;
  }
  get(id: string): RunDto {
    const row = this.dependencies.data.repositories.runs.get(id);
    if (!row) throw new AppError(AppErrorCode.NOT_FOUND, "Запуск не найден");
    return RunDto.parse({
      ...row,
      subjectId: row.subjectId ?? undefined,
      title: row.title ?? undefined,
      retryOfId: row.retryOfId ?? undefined,
      outcome: row.outcome ?? undefined,
      startedAt: row.startedAt ?? undefined,
      finishedAt: row.finishedAt ?? undefined,
      prunedAt: row.prunedAt ?? undefined,
      error: row.error ?? undefined,
    });
  }
  list(filter: RunListFilter = RunListFilter.parse({})): RunPageDto {
    const query = RunListFilter.parse(filter);
    const fetched = this.dependencies.data.repositories.runs.page({
      ...query,
      limit: query.limit + 1,
      attentionSince:
        this.options.clock() - (this.dependencies.attentionWindowMs ?? ATTENTION_WINDOW_MS),
    });
    const rows = fetched.slice(0, query.limit);
    const last = rows.at(-1);
    return RunPageDto.parse({
      items: this.summaries(rows),
      counts: this.counts(),
      ...(fetched.length > query.limit && last
        ? { nextCursor: encodeRunCursor({ createdAt: last.createdAt, id: last.id }) }
        : {}),
    });
  }
  detail(id: string): RunDetailDto {
    const run = this.get(id);
    const row = this.dependencies.data.repositories.runs.get(id)!;
    return RunDetailDto.parse({
      run,
      summary: this.summaries([row])[0],
      steps: this.steps(id),
      logs: this.dependencies.data.repositories.runs.logs(id),
    });
  }
  retry(id: string): RunHandleDto {
    const original = this.get(id);
    if (UNFINISHED.includes(original.status))
      throw new AppError(AppErrorCode.CONFLICT, "Запуск ещё не завершён");
    if (original.kind === "chat")
      throw new AppError(
        AppErrorCode.CONFLICT,
        "Ход чата повторяется со страницы чата: граф ссылается на сообщения исходного хода",
      );
    return this.start({
      kind: original.kind,
      graph: original.graph,
      input: original.input,
      concurrency: original.concurrency,
      retryOfId: RunId.parse(original.id),
      ...(original.subjectId === undefined ? {} : { subjectId: original.subjectId }),
      ...(original.title === undefined ? {} : { title: original.title }),
    });
  }
  clearFinished(): { removed: number } {
    return this.dependencies.data.transaction(({ runs }) => ({
      removed: runs.removeFinished(runs.referenced()),
    }));
  }
  private counts(): RunCountsDto {
    const raw = this.dependencies.data.repositories.runs.counts();
    const byStatus = Object.fromEntries(RunStatus.options.map((status) => [status, 0]));
    const byKind = Object.fromEntries(RunKind.options.map((kind) => [kind, 0]));
    let total = 0;
    for (const row of raw.byStatus) {
      byStatus[row.key] = row.total;
      total += row.total;
    }
    for (const row of raw.byKind) byKind[row.key] = row.total;
    return { total, byStatus, byKind } as RunCountsDto;
  }
  private summaries(rows: readonly RunEntity[]): RunSummaryDto[] {
    const { runs, permissions } = this.dependencies.data.repositories;
    const ids = rows.map((row) => row.id);
    const succeeded = new Map(runs.succeededCounts(ids).map((row) => [row.runId, row.total]));
    const active = new Map<string, string>();
    for (const row of runs.runningNodes(ids))
      if (!active.has(row.runId)) active.set(row.runId, row.nodeId);
    const approvals = new Map<string, RunSummaryDto["approval"]>();
    for (const row of permissions.pendingForRuns(ids))
      if (!approvals.has(row.runId)) approvals.set(row.runId, row);
    return rows.map((row) =>
      RunSummaryDto.parse({
        id: row.id,
        streamId: row.streamId,
        kind: row.kind,
        subjectId: row.subjectId ?? undefined,
        title: row.title ?? row.subjectId ?? row.id,
        status: row.status,
        progress: { done: succeeded.get(row.id) ?? 0, total: row.graph.nodes.length },
        activeNodeId: active.get(row.id),
        approval: approvals.get(row.id),
        outcome: row.outcome ?? undefined,
        error: row.error ?? undefined,
        retryOfId: row.retryOfId ?? undefined,
        createdAt: row.createdAt,
        startedAt: row.startedAt ?? undefined,
        finishedAt: row.finishedAt ?? undefined,
        prunedAt: row.prunedAt ?? undefined,
      }),
    );
  }
  steps(id: string): StepDto[] {
    this.get(id);
    return this.dependencies.data.repositories.runs.steps(id).map((row) =>
      StepDto.parse({
        ...row,
        output: row.status === "succeeded" && row.output !== null ? row.output : undefined,
        error: row.error ?? undefined,
        finishedAt: row.finishedAt ?? undefined,
      }),
    );
  }
  recover(): void {
    if (this.closed) return;
    for (const run of this.dependencies.data.repositories.runs.unfinished()) {
      if (this.active.has(run.id)) continue;
      const stream = this.openStream(run);
      this.dependencies.data.transaction(({ runs }) => {
        for (const step of runs.steps(run.id)) {
          if (step.status !== "running") continue;
          const patch = {
            status: "abandoned" as const,
            finishedAt: this.options.clock(),
            error: "Выполнение прервано завершением приложения",
          };
          runs.updateStep(step.id, patch);
          stream.emit({ type: "step", step: { ...step, ...patch } });
        }
      });
      const pending = this.dependencies.data.repositories.permissions.approvalsForRun(run.id);
      if (
        pending.some((row) => row.decision === null) ||
        (run.status === "blocked" && pending.length > 0)
      ) {
        this.dependencies.data.repositories.permissions.denyUnfinished(run.id);
        new Scheduler(run, stream, this.options).finish(
          "failed",
          "Approval interrupted",
          AppErrorCode.APPROVAL_DENIED,
        );
        continue;
      }
      if (this.dependencies.registry.canResume(run.graph)) {
        stream.emit({
          type: "log",
          line: { runId: run.id, message: "Возобновление с последней контрольной точки" },
        });
        this.launch(run, stream);
      } else
        new Scheduler(run, stream, this.options).finish(
          "interrupted",
          "Автовозобновление недоступно: узел имеет побочные эффекты или отсутствует",
        );
    }
  }
  async dispose(): Promise<void> {
    this.closed = true;
    for (const active of this.active.values()) active.scheduler.suspend();
    await Promise.all([...this.active.values()].map((active) => active.done));
    this.observers.clear();
  }
}
