import { integer, sqliteTable, text, index } from "drizzle-orm/sqlite-core";
import type { RunDto, RunGraph, RunOutcomeDto } from "@zvs/shared";

export const run = sqliteTable(
  "run",
  {
    id: text("id").primaryKey(),
    kind: text("kind").$type<RunDto["kind"]>().notNull(),
    subjectId: text("subject_id"),
    title: text("title"),
    retryOfId: text("retry_of_id"),
    status: text("status").$type<RunDto["status"]>().notNull(),
    graph: text("graph", { mode: "json" }).$type<RunGraph>().notNull(),
    input: text("input", { mode: "json" }).$type<RunDto["input"]>(),
    outcome: text("outcome", { mode: "json" }).$type<RunOutcomeDto>(),
    streamId: text("stream_id").notNull().unique(),
    concurrency: integer("concurrency").notNull(),
    createdAt: integer("created_at").notNull(),
    startedAt: integer("started_at"),
    finishedAt: integer("finished_at"),
    prunedAt: integer("pruned_at"),
    error: text("error"),
  },
  (table) => [
    index("run_status_idx").on(table.status),
    index("run_created_idx").on(table.createdAt, table.id),
    index("run_finished_idx").on(table.finishedAt),
  ],
);
export type RunEntity = typeof run.$inferSelect;
export type RunInsert = typeof run.$inferInsert;
