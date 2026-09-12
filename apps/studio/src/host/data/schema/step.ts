import { integer, sqliteTable, text, uniqueIndex } from "drizzle-orm/sqlite-core";
import type { StepDto } from "@zvs/shared";
import { run } from "./run.ts";

export const step = sqliteTable(
  "step",
  {
    id: text("id").primaryKey(),
    runId: text("run_id")
      .notNull()
      .references(() => run.id, { onDelete: "cascade" }),
    nodeId: text("node_id").notNull(),
    type: text("type").notNull(),
    status: text("status").$type<StepDto["status"]>().notNull(),
    input: text("input", { mode: "json" }).$type<StepDto["input"]>(),
    output: text("output", { mode: "json" }).$type<Exclude<StepDto["output"], undefined>>(),
    error: text("error"),
    startedAt: integer("started_at").notNull(),
    finishedAt: integer("finished_at"),
    attempt: integer("attempt").notNull(),
  },
  (table) => [uniqueIndex("step_attempt_unq").on(table.runId, table.nodeId, table.attempt)],
);
export type StepEntity = typeof step.$inferSelect;
export type StepInsert = typeof step.$inferInsert;
