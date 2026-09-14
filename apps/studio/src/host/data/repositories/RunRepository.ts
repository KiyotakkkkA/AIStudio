import {
  and,
  asc,
  count,
  desc,
  eq,
  exists,
  gte,
  inArray,
  isNull,
  like,
  lt,
  lte,
  or,
  sql,
} from "drizzle-orm";
import type { HostEvent, LogLineDto, RunDto } from "@zvs/shared";
import { message } from "../schema/message.ts";
import { run, step, runEvent, type RunInsert, type StepInsert } from "../schema/index.ts";
import { Repository } from "./Repository.ts";

type RunStatus = RunDto["status"];
type RunKind = RunDto["kind"];

const ATTENTION_STATUSES: RunStatus[] = ["failed", "interrupted"];
const UNFINISHED_STATUSES: RunStatus[] = ["queued", "running", "blocked"];
const FINISHED_STATUSES: RunStatus[] = ["succeeded", "failed", "cancelled", "interrupted"];

export interface RunPageQuery {
  readonly statuses?: readonly RunStatus[];
  readonly kinds?: readonly RunKind[];
  readonly query?: string;
  readonly from?: number;
  readonly to?: number;
  readonly live?: boolean;
  readonly attentionSince?: number;
  readonly limit: number;
  readonly cursor?: string;
}

export interface RunCursor {
  readonly createdAt: number;
  readonly id: string;
}

export function encodeRunCursor(value: RunCursor): string {
  return `${value.createdAt}:${value.id}`;
}

export function decodeRunCursor(value: string): RunCursor | undefined {
  const separator = value.indexOf(":");
  if (separator <= 0) return undefined;
  const createdAt = Number(value.slice(0, separator));
  const id = value.slice(separator + 1);
  if (!Number.isInteger(createdAt) || id.length === 0) return undefined;
  return { createdAt, id };
}

export class RunRepository extends Repository {
  create(value: RunInsert) {
    return this.db.insert(run).values(value).returning().get();
  }
  get(id: string) {
    return this.db.select().from(run).where(eq(run.id, id)).get();
  }
  list() {
    return this.db.select().from(run).orderBy(desc(run.createdAt), desc(run.id)).all();
  }
  page(query: RunPageQuery) {
    const cursor = query.cursor === undefined ? undefined : decodeRunCursor(query.cursor);
    const search = query.query === undefined || query.query === "" ? undefined : `%${query.query}%`;
    const conditions = [
      query.statuses?.length ? inArray(run.status, [...query.statuses]) : undefined,
      query.kinds?.length ? inArray(run.kind, [...query.kinds]) : undefined,
      query.live
        ? or(
            inArray(run.status, UNFINISHED_STATUSES),
            and(
              inArray(run.status, ATTENTION_STATUSES),
              gte(run.finishedAt, query.attentionSince ?? 0),
            ),
          )
        : undefined,
      query.from === undefined ? undefined : gte(run.createdAt, query.from),
      query.to === undefined ? undefined : lte(run.createdAt, query.to),
      search === undefined
        ? undefined
        : or(
            like(run.title, search),
            like(run.id, search),
            like(run.subjectId, search),
            exists(
              this.db
                .select({ one: sql`1` })
                .from(step)
                .where(
                  and(
                    eq(step.runId, run.id),
                    or(like(step.nodeId, search), like(step.type, search)),
                  ),
                ),
            ),
          ),
      cursor === undefined
        ? undefined
        : or(
            lt(run.createdAt, cursor.createdAt),
            and(eq(run.createdAt, cursor.createdAt), lt(run.id, cursor.id)),
          ),
    ].filter((condition) => condition !== undefined);
    return this.db
      .select()
      .from(run)
      .where(conditions.length ? and(...conditions) : undefined)
      .orderBy(desc(run.createdAt), desc(run.id))
      .limit(query.limit)
      .all();
  }
  counts() {
    return {
      byStatus: this.db
        .select({ key: run.status, total: count() })
        .from(run)
        .groupBy(run.status)
        .all(),
      byKind: this.db.select({ key: run.kind, total: count() }).from(run).groupBy(run.kind).all(),
    };
  }
  unfinished() {
    return this.db.select().from(run).where(inArray(run.status, UNFINISHED_STATUSES)).all();
  }
  update(id: string, patch: Partial<RunInsert>) {
    this.db.update(run).set(patch).where(eq(run.id, id)).run();
  }
  steps(id: string) {
    return this.db.select().from(step).where(eq(step.runId, id)).orderBy(asc(step.id)).all();
  }
  succeededCounts(ids: readonly string[]) {
    if (ids.length === 0) return [];
    return this.db
      .select({ runId: step.runId, total: count() })
      .from(step)
      .where(and(inArray(step.runId, [...ids]), eq(step.status, "succeeded")))
      .groupBy(step.runId)
      .all();
  }
  runningNodes(ids: readonly string[]) {
    if (ids.length === 0) return [];
    return this.db
      .select({ runId: step.runId, nodeId: step.nodeId, startedAt: step.startedAt })
      .from(step)
      .where(and(inArray(step.runId, [...ids]), eq(step.status, "running")))
      .orderBy(asc(step.startedAt))
      .all();
  }
  addStep(value: StepInsert) {
    this.db.insert(step).values(value).run();
  }
  updateStep(id: string, patch: Partial<StepInsert>) {
    this.db.update(step).set(patch).where(eq(step.id, id)).run();
  }
  appendEvent(runId: string, event: HostEvent) {
    this.db.insert(runEvent).values({ runId, seq: event.seq, event }).run();
  }
  events(id: string) {
    return this.db
      .select()
      .from(runEvent)
      .where(eq(runEvent.runId, id))
      .orderBy(asc(runEvent.seq))
      .all()
      .map((row) => row.event);
  }
  logs(id: string): LogLineDto[] {
    return this.events(id)
      .filter((event) => event.type === "log")
      .map((event) => event.line);
  }
  nextSequence(id: string) {
    return (
      (this.db
        .select({ seq: runEvent.seq })
        .from(runEvent)
        .where(eq(runEvent.runId, id))
        .orderBy(desc(runEvent.seq))
        .limit(1)
        .get()?.seq ?? -1) + 1
    );
  }
  prunable(before: number, limit: number) {
    return this.db
      .select({ id: run.id })
      .from(run)
      .where(and(lt(run.finishedAt, before), isNull(run.prunedAt)))
      .orderBy(asc(run.finishedAt))
      .limit(limit)
      .all()
      .map((row) => row.id);
  }
  dropPayloads(ids: readonly string[], prunedAt: number) {
    if (ids.length === 0) return;
    const scope = [...ids];
    this.db.update(step).set({ input: null, output: null }).where(inArray(step.runId, scope)).run();
    this.db.delete(runEvent).where(inArray(runEvent.runId, scope)).run();
    this.db.update(run).set({ prunedAt }).where(inArray(run.id, scope)).run();
  }
  referenced() {
    return this.db
      .selectDistinct({ runId: message.runId })
      .from(message)
      .all()
      .map((row) => row.runId)
      .filter((id): id is string => id !== null);
  }
  removeFinished(except: readonly string[]) {
    const conditions = [inArray(run.status, FINISHED_STATUSES)];
    const rows = this.db
      .select({ id: run.id })
      .from(run)
      .where(and(...conditions))
      .all()
      .map((row) => row.id)
      .filter((id) => !except.includes(id));
    if (rows.length === 0) return 0;
    this.db.delete(run).where(inArray(run.id, rows)).run();
    return rows.length;
  }
}
