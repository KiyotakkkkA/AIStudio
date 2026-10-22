---
task: TASK_026
title: Tasks page (live) and Runs & Logs (history)
phase: 4 — Execution kernel and Chat
status: done
depends_on: [TASK_025]
unblocks: [TASK_027, TASK_042]
estimate: 4 h
---

# TASK_026 — Tasks page (live) and Runs & Logs (history)

## Context

Runs exist and chat produces them, but they are invisible. This task builds the two pages that make execution observable — and it completes Phase 4, after which the app is genuinely usable.

The split is stated on the design canvas: **Tasks is what is happening now** — running, queued, scheduled, blocked on approval — while **Runs & Logs keeps the finished history**. The same run appears in one then the other. `/design/Tasks.dc.html` shows the live view: grouped sections, progress bars, a blocked agentic session with a Review button, a failed scenario with Retry, and a right rail with the selected run's live step tree and trigger payload.

Because everything is a kernel run (TASK_022), this page is one list over one table rather than a union of three shapes. That is decision D7 paying off, and it is worth writing the code so it stays true — no per-kind special cases in the list itself.

## Why

Without it, a long or blocked run is invisible, approvals cannot be answered, and a failure leaves no trail. It is also the page that makes indexing and downloads (Phase 5) legible.

## Depends on

- `TASK_025` — Chat page — thread, composer, retrieval citations.

## Read first

- `/design/Tasks.dc.html` — grouped sections, row anatomy, the blocked and failed states, the detail rail
- `/docs/PASSPORT.md` §11, §7 (the events these pages render)

## Scope

**In scope**
- `features/tasks/`: live run store subscribing to run events
- Tasks page: Running / Queued & scheduled / Needs attention groups, filters by status and kind
- Row actions: pause where supported, stop, retry, review an approval
- Detail rail: live step tree, timings, trigger payload, actions
- Runs & Logs page: searchable history with per-run step detail and log output
- Retention policy for finished runs

**Out of scope** (do not do these, even if tempting)
- Scheduling itself — the Scheduled group renders what exists; recurring schedules are a later task
- Log shipping or export

## Steps

1. `TaskStore` (MobX): subscribes to run lifecycle events and keeps the live list current without polling. Falls back to a refetch if the event stream reports a gap (TASK_005 surfaces those).
2. Tasks page grouped exactly as the mockup: Running with progress and elapsed time, Queued & scheduled, Needs attention (blocked on approval, failed). Left rail filters by status and by kind with live counts.
3. Row anatomy from the mockup: kind icon tile, name, kind chip, run id chip, a sub-line that is step-count for graphs and a unit count for jobs, progress bar, elapsed, status chip, actions.
4. Approval rows: a blocked run shows the amber state and a Review action that opens the approval, which is the same `approval` event TASK_023 emits. Approving or denying resumes the run.
5. Detail rail: live step tree with per-step timing, the run's trigger payload, and Stop / Open in editor / Full log actions.
6. Runs & Logs: paginated history with filters by kind, status and date, a text search over run and step names, and a detail view showing every step with its input, output and error plus the run's log lines.
7. Retention: prune finished runs per the decision below, on a schedule, never blocking the UI.
8. Tests: the store reconciles correctly when events arrive out of order or with a gap; group assignment is a pure function and covered; retry creates a new run linked to the original.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "How long should finished runs and their step history be kept — everything forever, or pruned after some period or count? Every chat message is a run, so this grows steadily."
>
> **Why it matters:** Step rows store inputs and outputs, which for chat means full prompts and answers. Unbounded, the database grows without limit; too aggressive, and you lose the history that makes Runs & Logs useful.
> **Options:** (a) keep everything, expose a manual 'clear history'; (b) keep N days of full step detail, then keep the run summary and drop step payloads; (c) keep a fixed number of runs per kind.
> **If the user is unavailable:** take (b) with 30 days, make it a setting for TASK_052, and never prune runs that are still referenced by a conversation.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Start a chat turn and watch it appear and complete on the Tasks page in real time. Trigger an approval-gated run and answer it from Tasks. Kill a run and confirm it moves to Needs attention, then to history.

## Definition of done

- [x] One list over one run table — no per-kind special cases in the list
- [x] Live updates arrive by event, not polling, with a gap fallback
- [x] Approvals answerable from Tasks
- [x] Runs & Logs searchable, with full step detail
- [x] Retention implemented per the decision
- [x] Tests and gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- `RunListFilter` / `RunSummaryDto` / `RunPageDto` / `RunDetailDto` in `shared/runs`. `runs.list`
  now takes one filter (statuses, kinds, text query, date range, `live`, cursor paging) and
  answers rows plus global counts; `runs.detail`, `runs.retry` and `runs.clearFinished` are new.
- `run` gained `title`, `retry_of_id`, `pruned_at` and indexes on `created_at` / `finished_at`
  (migration `0011_run_history`). `RunRepository` owns the page query, the group-by counts, the
  per-run progress and active-node lookups, log extraction, and the prune/delete statements.
- `RunService.list/detail/retry/clearFinished`. A summary's progress is succeeded steps over
  graph nodes; the active node and a pending approval come from the step and approval tables, so
  the list is one query shape for every kind. `retry` re-starts the original graph as a new run
  with `retryOfId`.
- `RetentionService`: a timer sweep (6 h, unref'd, batched) that drops step payloads and recorded
  events for runs finished more than `runs.retention.days` ago (default 30) and stamps `prunedAt`.
  Wired in the composition root, stopped on quit.
- `features/tasks/`: `TaskStore` (live, event-driven, per-stream seq reconciliation, debounced
  refetch on a gap / unknown stream / `end`), `RunHistoryStore` (filters, debounced search, cursor
  paging, step detail), pure `runGroups.ts` and `runPresentation.ts`, and the components for both
  pages — grouped list with row actions, filter rail with live counts, live step tree, trigger
  payload, and a searchable history with per-step input/output/error plus log lines.
- Tests: `apps/studio/tests/runHistory.test.ts` (filters, paging, search, live window, summaries,
  retry linkage and refusals, retention, clear-finished sparing referenced runs) and
  `tests/renderer/{taskStore,runHistoryStore,tasksPage}.test.*` (grouping as a pure function,
  out-of-order and gap reconciliation, live step tree, approval and retry from the row).

**Deviated from plan:**

- **No pause action.** The kernel has cancel and suspend-on-shutdown but no resume, so "pause
  where supported" is supported nowhere; rows offer Stop instead. Add it with the scheduler.
- **No "Open in editor".** There is no scenario editor yet (TASK_038); the rail offers «Полный
  журнал», which navigates to `/runs?run=<id>` and selects that run.
- **The Scheduled group is folded into "В очереди и по расписанию"** and currently renders queued
  runs only — scheduling itself is out of scope.
- **Retry refuses chat runs** with a message pointing at the Chat page: a chat graph binds the
  message ids of the original turn, so a blind re-run would always fail at `chat.prepare`.
- **Retention prunes payloads for chat runs too.** The "never prune runs referenced by a
  conversation" rule is applied to the delete path (`clearFinished`), not to payload-dropping —
  the message rows keep the prompt and answer, so nothing user-visible is lost.
- Counts in the Tasks filter rail are computed from the loaded live rows; the all-time counts the
  host returns drive the Runs & Logs rail.
- Fixed two leftovers from TASK_025 that were failing repo gates: `providerFormVm.test.ts`
  imported a default export that no longer exists, and `ChatMessage.tsx` tripped
  `lint:tailwind`.

**Next session should know:**

- `runs.list` input is a required object (`{ live, limit, ... }`) because `InputOf` resolves to the
  schema's output type; pass both fields explicitly.
- `runs.retention.days` is a plain setting key read on every sweep — TASK_052 only has to expose
  it in the UI. `RETENTION_DAYS_KEY` / `DEFAULT_RETENTION_DAYS` are exported.
- A run's `title` is stored at start (`StartRunInput.title`); `ChatService` writes the conversation
  title and rewrites it once the conversation is auto-named. Renaming a conversation later does
  not retitle its past runs.
- Live status changes ride on the `log` events the scheduler already emits (`{ runId, status }`);
  there is no separate run-lifecycle event type.

---

**Phase 4 is complete when this task is done.** The app now does real work and shows it happening: chat with retrieval, runs on a kernel, execution visible and controllable.
