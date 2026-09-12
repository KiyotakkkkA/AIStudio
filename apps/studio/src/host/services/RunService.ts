import {
  AppError,
  AppErrorCode,
  RunDto,
  RunId,
  StartRunInput,
  StepDto,
  StreamId,
  type RunHandleDto,
} from "@zvs/shared";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import type { RunEntity } from "../data/schema/index.ts";
import { NodeRegistry } from "../kernel/NodeRegistry.ts";
import { Scheduler, type SchedulerOptions } from "../kernel/Scheduler.ts";
import type { EventBus, StreamHandle } from "../platform/events.ts";
import { createId } from "../platform/ids.ts";

export interface RunServiceOptions extends Partial<Omit<SchedulerOptions, "data" | "registry">> {
  data: UnitOfWork;
  events: EventBus;
  registry: NodeRegistry;
}
export class RunService {
  private readonly active = new Map<string, { scheduler: Scheduler; done: Promise<void> }>();
  private readonly options: SchedulerOptions;
  private closed = false;
  constructor(private readonly dependencies: RunServiceOptions) {
    const graceMs = dependencies.graceMs ?? 500;
    if (!Number.isInteger(graceMs) || graceMs < 0 || graceMs > 30_000)
      throw new Error("Invalid cancellation grace period");
    this.options = {
      ...dependencies,
      clock: dependencies.clock ?? Date.now,
      graceMs,
      services: dependencies.services ?? {},
    };
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
      record: (event) => runs.appendEvent(run.id, event),
    });
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
      outcome: row.outcome ?? undefined,
      startedAt: row.startedAt ?? undefined,
      finishedAt: row.finishedAt ?? undefined,
      error: row.error ?? undefined,
    });
  }
  list(): RunDto[] {
    return this.dependencies.data.repositories.runs.list().map((row) => this.get(row.id));
  }
  steps(id: string): StepDto[] {
    this.get(id);
    return this.dependencies.data.repositories.runs.steps(id).map((row) =>
      StepDto.parse({
        ...row,
        output: row.status === "succeeded" ? row.output : undefined,
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
  }
}
