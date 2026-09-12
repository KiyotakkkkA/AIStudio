import { asc, desc, eq, inArray } from "drizzle-orm";
import type { HostEvent } from "@zvs/shared";
import { run, step, runEvent, type RunInsert, type StepInsert } from "../schema/index.ts";
import { Repository } from "./Repository.ts";

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
  unfinished() {
    return this.db
      .select()
      .from(run)
      .where(inArray(run.status, ["queued", "running", "blocked"]))
      .all();
  }
  update(id: string, patch: Partial<RunInsert>) {
    this.db.update(run).set(patch).where(eq(run.id, id)).run();
  }
  steps(id: string) {
    return this.db.select().from(step).where(eq(step.runId, id)).orderBy(asc(step.id)).all();
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
}
