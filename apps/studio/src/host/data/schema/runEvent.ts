import { integer, sqliteTable, text, primaryKey } from "drizzle-orm/sqlite-core";
import type { HostEvent } from "@zvs/shared";
import { run } from "./run.ts";

export const runEvent = sqliteTable(
  "run_event",
  {
    runId: text("run_id")
      .notNull()
      .references(() => run.id, { onDelete: "cascade" }),
    seq: integer("seq").notNull(),
    event: text("event", { mode: "json" }).$type<HostEvent>().notNull(),
  },
  (table) => [primaryKey({ columns: [table.runId, table.seq] })],
);
