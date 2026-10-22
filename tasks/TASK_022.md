---
task: TASK_022
title: Kernel core — Run, Step, scheduler, cancellation, checkpointing
phase: 4 — Execution kernel and Chat
status: done
depends_on: [TASK_021]
unblocks: [TASK_023]
estimate: 4 h
---

# TASK_022 — Kernel core — Run, Step, scheduler, cancellation, checkpointing

## Context

This is the most consequential task in the backlog. PASSPORT decision D7 says chat, scenarios and agentic sessions are not three systems — they are three front-ends over **one execution kernel**. A chat turn is a short graph, a scenario is a user-authored DAG, an agentic session is a self-extending plan. All three are a **Run** made of **Steps** emitting **Events**.

That decision is what makes the Tasks page one list instead of a union of three shapes, Runs & Logs one history, and cancellation, retry and progress written once. It also means this task has to be general enough on day one, because five later tasks assume its shape.

The kernel lives in `host/kernel/` and owns: the run lifecycle, a scheduler that walks the graph respecting concurrency limits, cooperative cancellation through `AbortSignal`, persistence of every step with its input, output, timing and error, and a checkpoint after each step so a run survives an app restart. It does **not** own node implementations — that is TASK_023.

## Why

Chat (TASK_024), Tasks (TASK_026), indexing (TASK_028), scenarios (TASK_039), agentic (TASK_040) and the browser (TASK_046) are all kernel front-ends. Everything downstream inherits whatever shape this task establishes.

## Depends on

- `TASK_021` — Vector Stores page — list, detail, test search.

## Read first

- `/docs/PASSPORT.md` §11 (the kernel — `NodeDef` and `StepContext` are specified there; implement them as written), §7 (the event channel this emits on), §2 D7

## Scope

**In scope**
- `run` and `step` tables plus migration
- `Run` lifecycle: queued → running → (blocked) → succeeded | failed | cancelled
- Scheduler walking a graph with concurrency limits and per-node retry policy
- Cooperative cancellation propagated through `AbortSignal`
- Checkpoint after each step; resume an interrupted run on next boot
- Event emission on the TASK_005 bus for every state change
- `RunService` and channels: start, cancel, list, get
- Thorough tests — this is the least forgiving code in the project

**Out of scope** (do not do these, even if tempting)
- Node implementations and the permission model (TASK_023)
- Any front-end — no chat, no scenarios yet
- Distributed or multi-process execution; everything runs in the host

## Steps

1. `run` table: id, kind (`chat` | `scenario` | `agentic` | `job` | `browser`), subjectId (the conversation, scenario or session it belongs to), status, graph JSON, input JSON, outcome JSON, streamId, startedAt, finishedAt, error. `step` table: id, runId, nodeId, type, status, input, output, error, startedAt, finishedAt, attempt.
2. `Run` model and a `Scheduler` that walks the graph: resolve ready nodes (dependencies satisfied), respect a concurrency cap, execute, record, repeat. Deterministic ordering for equal-priority nodes so runs are reproducible.
3. `StepContext` exactly as PASSPORT §11 specifies: `runId`, `signal`, `emit`, `requestApproval`, `services`. Keep `services` narrow and explicit — a context that exposes everything becomes an ambient global.
4. Cancellation: one `AbortController` per run; cancelling aborts in-flight steps and marks the run cancelled. Steps must be written to honour the signal, and the kernel must not wait indefinitely for one that does not — apply a grace period, then mark it abandoned and log loudly.
5. Checkpointing: persist step results as they complete. On boot, find runs left `running` and either resume them from the last checkpoint or mark them interrupted, depending on the decision below.
6. Retry: per-node policy (max attempts, backoff). Retries are recorded as separate step rows with an incrementing attempt, so the history shows what actually happened rather than hiding failures.
7. Events: emit `step`, `progress`, `log` and the terminal `end` on the run's stream. Every run has exactly one stream id, created at start.
8. `RunService` plus channels `runs.start|cancel|list|get|steps`.
9. Tests, extensively: a diamond-shaped graph executes in dependency order; concurrency cap is respected; cancellation mid-run stops promptly and marks correctly; a step that ignores its signal is abandoned after the grace period; retry records separate attempts; a killed process resumes or marks interrupted per the decision; events are emitted in the right order with monotonic seq.

## ❓ DECISION — ask the user before implementing this part

**Resolved by the user:** auto-resume only runs whose nodes are all side-effect-free (option b).
Every node declares `sideEffectFree`; missing definitions and any value other than `true`
prevent automatic recovery. Queued, running and blocked runs follow this policy on boot.

> **Question to ask, verbatim:**
> "If the app is closed or crashes while a run is in progress, should it resume that run automatically on the next launch, or mark it interrupted and let you decide whether to restart it?"
>
> **Why it matters:** Automatic resume is friendlier for long indexing jobs but dangerous for anything with side effects — a scenario that sends mail could send it twice if the crash happened after the send but before the checkpoint.
> **Options:** (a) never auto-resume — mark interrupted, offer a Restart button on the Tasks page; (b) auto-resume only runs whose nodes are all marked side-effect-free (indexing, embedding), mark the rest interrupted; (c) always auto-resume.
> **If the user is unavailable:** take (b), and require every node in TASK_023 to declare whether it has side effects. It is the honest answer and the extra field costs nothing.

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: Start a long synthetic run from a scratch channel, cancel it from another call, and confirm it stops within a second and lands as `cancelled` with its steps recorded. Kill the app mid-run and confirm the boot behaviour matches the decision.

## Definition of done

- [x] Runs execute a graph in dependency order with a concurrency cap
- [x] Cancellation is prompt and correctly recorded; unresponsive steps are abandoned, not waited on forever
- [x] Every step persisted with input, output, timing, error and attempt
- [x] Checkpoint/resume behaves per the decision above
- [x] Events emitted on one stream per run, in order
- [x] Tests cover ordering, concurrency, cancellation, retry, crash recovery
- [x] Gates pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:** Run/Step DTOs, validated DAG input bindings, migration `0007_kernel` and the
RunRepository; deterministic scheduler with per-run concurrency, separate retry attempts,
linear backoff, injected clock, cooperative cancellation and a 500 ms abandonment grace.
RunService and all five channels are wired into startup and shutdown. Checkpoints and run
events are durable; recovery preserves stream IDs, sequence numbers and completed outputs.
Approval and admission hooks provide the TASK_023 seam. No code comments were added.

**Deviated from plan:** Added `run_event` alongside `run` and `step` so streams remain
replayable and monotonic after process death. Added explicit `interrupted` run status,
creation time and persisted concurrency. A minimal registry is necessary to validate graphs
and inspect recovery eligibility in this task; node implementations, stored permissions and
approval decisions remain TASK_023. The side-effect declaration uses positive
`sideEffectFree: boolean` rather than TASK_023's provisional `sideEffect` spelling. Crash and
cancellation checks are automated against synthetic host runs, including actually killing
child host processes with SQLite checkpoints, without adding a scratch production channel.

**Next session should know:** Read `apps/studio/src/host/kernel/README.md`. Populate the
registry before `RunService.recover()`; production currently registers no node implementations.
Use `NodeRegistry.register/get/validate`, the five-member StepContext, and the injected
`admit`/`approval` hooks. Permission-requiring nodes fail closed without admission. Only
`sideEffectFree === true` permits auto-resume, and every node must declare it. Dependency
bindings inject full outputs into named input properties. Retried and crash-abandoned
attempts remain visible as separate rows. Shutdown drains kernel work before disposing its
service dependencies and database. TASK_026 owns restart UI and history retention.

**Validation:** `pnpm --filter studio test`: 309 passed, 2 pre-existing skips, including 31
kernel cases for DAG order, concurrency, cancellation, abandonment, retry/backoff, schemas,
approval waits, isolated inputs/outputs, stream integrity, persisted recovery and actual
killed-process recovery for both safe and unsafe nodes.
`pnpm typecheck`, `pnpm lint` and `pnpm build` all pass. Build reports non-failing dependency
annotation/directive warnings and the existing empty site-preload chunk.
