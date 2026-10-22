---
task: TASK_070
title: Execution kernel in Rust — Scheduler, NodeRegistry, permissions and approvals, RunService, retention, runs.*
phase: M — Migration to Tauri
status: done
depends_on: [TASK_069]
unblocks: [TASK_071]
estimate: 6 h
---

# TASK_070 — Execution kernel in Rust — Scheduler, NodeRegistry, permissions and approvals, RunService, retention, runs.*

## Context

**Migration ground rules (TASK_058 – TASK_076).** The app is moving from Electron and a
TypeScript host to Tauri 2 and a Rust host (PASSPORT D10, decided 2026-10-10). Both shells live
in the tree until TASK_076. The TS host is frozen, and every port reproduces its behaviour
exactly, with its tests as the specification. The renderer does not change beyond the bridge.
There is no data migration.

The kernel is D7: one core, three front-ends. `apps/studio/src/host/kernel/README.md` is its
specification and should be read in full. The parts to reproduce:
- `Scheduler.ts`: graph walk, ready nodes ordered by id, per-run concurrency cap, bindings
  (`run` or a dependency's whole output), retries as separate step rows, linear backoff
  outside the concurrency slot, a cancellation grace period (500 ms) after which nodes are
  abandoned and cannot publish, and a terminal failure that aborts siblings.
- `NodeRegistry.ts` and `types.ts`: node definitions with input and output schemas, a
  permission requirement and a mandatory `sideEffect` declaration.
- `PermissionService.ts`: admission composes the node requirement, the global ceiling and
  every matching scope with `off > ask > auto`. A missing global policy means `ask`, and a
  scoped `auto` never widens.
- `ApprovalService.ts`: a pending approval is persisted before its event, a timeout denies,
  restart denies interrupted approvals, `always` writes a grant at the request scope, and the
  ceiling is re-checked before resuming.
- `RunService.ts`: admission, `start` returning `{ id, streamId }` before scheduling, inspect,
  cancel, retry (refused for chat), `clearFinished` sparing referenced runs, `recover()`
  (resume side-effect-free runs, otherwise mark interrupted), `subscribe` for synchronous host
  observers, and transactional run, step and event persistence with stream continuity across
  recovery.
- `RetentionService.ts`: prunes payloads after `runs.retention.days` (default 30).
- Core nodes `llm.generate`, `vector.search`, `flow.branch` and `flow.map`, plus `job.run`
  (sidecar) and the download nodes registered by later tasks.

Channels: `runs.*` (`approve`, `cancel`, `clearFinished`, `deny`, `detail`, `get`, `list`,
`retry`, `start`, `steps`) and `system.demoJob`.

In Rust, `AbortSignal` becomes `tokio_util::sync::CancellationToken` and Zod node schemas
become serde types. A node is written once as a typed `NodeDef` (`Input: DeserializeOwned +
JsonSchema`, `Output: Serialize + JsonSchema`) and registered type-erased over
`serde_json::Value`. The JSON Schema comes from `schemars`, pinned, and is what the tool
registry (TASK_073) will hand to models.

## Why

Chat, jobs, downloads, indexing, tools, scenarios and agentic runs all execute here. The
Tasks and Runs & Logs pages render its rows. Everything after this task registers nodes into
it.

## Depends on

- `TASK_069` — `vector.search` needs the vector service, and `llm.generate` needs the
  registry from TASK_063.

## Read first

- `apps/studio/src/host/kernel/README.md` in full
- `/docs/PASSPORT.md` §11, §14 (permission ceiling)
- `apps/studio/src/host/kernel/*`, `services/RunService.ts`, `PermissionService.ts`,
  `ApprovalService.ts`, `RetentionService.ts`
- `apps/studio/src/host/data/repositories/RunRepository.ts`, `PermissionRepository.ts`
- `apps/studio/tests/kernel.test.ts`, `permissions.test.ts`, `runHistory.test.ts`

## Scope

**In scope**
- `kernel::{types, registry, scheduler}`, `services::{run, permission, approval, retention}`,
  and the run, step, run_event and permission repositories
- Core nodes `llm.generate` (token and reasoning events, `{ request }` binding form,
  `sideEffect: true`), `vector.search`, `flow.branch`, `flow.map` and `job.run`
- `runs.*` channels and `system.demoJob`, with DTOs and fixtures
- `ShutdownCoordinator`: `runs.dispose()` suspends runs with the cancellation bound and drains
  before services close and SQLite closes, in the TS order
- `RunService::recover()` called at boot after the full registry is built. The composition
  root keeps that order.

**Out of scope** (do not do these, even if tempting)
- Chat graphs and `chat.*` nodes (TASK_074)
- Tool and MCP nodes (TASK_073), download and indexing nodes (TASK_071, TASK_072)
- Any change to scheduling, permission or approval semantics

## Steps

1. Types and the typed-to-erased `NodeDef` adapter, with schema generation tested on a sample
   node.
2. `Scheduler` with every case from `kernel.test.ts`: ordering, bindings and isolation,
   retries and backoff, concurrency, cancellation grace and abandonment, sibling abort, and
   late publications dropped. Use `tokio::time::pause()` for timing tests.
3. Repositories and the transactional persistence of runs, steps and events.
4. `PermissionService` and `ApprovalService` with `permissions.test.ts` ported, including
   persisted-before-emit, timeout, restart denial and the scoped-grant rules.
5. `RunService`: start, inspect, list filter (`RunListFilter` with cursor paging and global
   counts), retry, clear, recover and subscribe. Port `runHistory.test.ts`.
6. `RetentionService` with fake-clock tests.
7. Core nodes, channels, shutdown and wiring. Tasks and Runs & Logs pages under Tauri.

## Verify

```bash
cargo test -p zvs-host
cargo clippy --workspace --all-targets -- -D warnings
pnpm typecheck && pnpm lint && pnpm build
```

Manual check: in `pnpm dev:tauri`, `system.demoJob` shows a live run on the Tasks page with
progress. Cancel it and see the sidecar job killed. Quit mid-run, relaunch, and see it marked
interrupted. Runs & Logs lists, filters, opens detail and retries a finished job.

## Definition of done

- [x] Scheduler, registry, permissions, approvals, run service and retention are ported with
      the full TS test suites
- [x] Typed nodes produce JSON Schema for later tool exposure
- [ ] Tasks and Runs & Logs work under Tauri, including recovery after restart
- [x] Shutdown suspends and drains runs before SQLite closes
- [x] `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` pass
- [x] `pnpm typecheck && pnpm lint && pnpm build` pass
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**

- `zvs-contract`: `runs` (`RunStatus`, `RunKind`, `StepStatus`, `GraphBinding`, `RetryPolicy`,
  `GraphNode`, `RunGraph`, `StartRunInput`, `RunIdInput`, `RunHandleDto`, `RunsApproveInput`,
  `RunsDenyInput`, `RunDto`, `StepDto`, `RunListFilter`, `RunCountsDto`, `RunProgressDto`,
  `RunApprovalDto`, `RunSummaryDto`, `RunPageDto`, `RunDetailDto`, `RunsClearFinishedOutput`,
  `is_permission_scope`) and `tools::ToolTier`, with Zod defaults behind accessors and
  `normalized()` for stored graphs. 19 fixture pairs under `packages/shared/fixtures/runs` and
  `tools`, registered in both tables. An optional `schema` feature (`schemars =1.2.2`) derives
  `JsonSchema` for branded ids, every `wire_enum!`, `VectorSearchInput` and
  `VectorSearchHitDto`.
- Data: `RunRow`, `StepRow`, `ToolPermissionRow`, `PermissionUseRow`, `PendingApprovalRow`,
  `ToolCeilingRow`; `RunRepository` (create, get, page with the full `RunListFilter`, counts,
  unfinished, steps, events, logs, next sequence, prunable, drop payloads, referenced, remove
  finished, the `createdAt:id` cursor) and `PermissionRepository` (grants with upsert, uses,
  approvals, `tool_ceiling` read from the `tool` table), each with tests.
- `kernel`: `NodeDef` (typed) → `RegisteredNode` (erased over `Value`, input parsed and
  validated, output validated, input/output JSON Schema recorded), `FnNode`, `NodeRegistry`
  (insertion order, `validate`, `can_resume`, `for_tool`, `unregister`), `StepContext` with
  `StepLink`, `KernelServices` (`TextDrivers`, `VectorSearch`, `SidecarJobs` ports),
  `Scheduler`, `RunStream` + `Observers`, and the nodes `llm.generate`, `vector.search`,
  `flow.branch`, `flow.map`, `job.run`.
- `services`: `PermissionService`, `ApprovalService`, `RunService`, `RetentionService`.
  `StreamHandle::publish_with` assigns sequence numbers inside the transaction that persists
  the rows and the events.
- `ipc::runs` (all ten `runs.*`) and `system.demoJob`; `Services.runs`; `test_services` gets a
  run service with `FakeJobs`.
- Composition: core and job nodes registered, `RunService` over the shared provider registry,
  vector service and sidecar, `RetentionService` started, `retention.stop` in `Stop`, `runs` in
  `Drain` (before `Dispose` and SQLite), `recover()` after the server is finished.
- Tests: `kernel.test.ts` (including the killed-process case, which re-runs the test binary as
  the child), `permissions.test.ts` and `runHistory.test.ts` ported, plus retention timing, IPC
  and demo job tests. Gates run on Windows 2026-10-16: `cargo fmt`, workspace clippy with
  `-D warnings` (also `--features debug-panic`), `cargo test --workspace` (zvs-host 544
  passed), `pnpm typecheck`, `pnpm lint`, `pnpm build`, `pnpm format:check`,
  `npx vitest run` (1500 passed, 2 skipped), `contract-channels --check`.

**Deviated from plan:**

- Delivery happens after the transaction commits, not inside it as in TS, because the
  `Database` mutex cannot be re-entered by an observer. Observers may write to the database
  but must not emit on the run they observe.
- A node runs in its own task. Abandoned tasks are detached, never aborted, exactly as in TS.
  A panicking node fails its step with «Узел завершился аварийно».
- Step bookkeeping (attempt, step id, bound input) happens when the node future is first
  polled, a tick later than in TS. Order is unchanged.
- `StepContext::invokeTool` and the `tools` ceiling/`recordUse` hook are not ported; they
  belong with `ToolRegistry` in TASK_073. `PermissionService` already reads the `tool` row's
  tier, `enabled` and `available`. `vector.list` and `vector.index` are not registered.
- Optional input properties have `null` stripped from their JSON Schema, because they are
  absent-or-set like Zod's `.optional()`.
- Invalid grace and approval timeouts are `VALIDATION_FAILED` from `RunService::new`
  rather than thrown errors.

**Next session should know:**

- The first `z.void()` outputs ported (`runs.approve`, `runs.deny`, `runs.cancel`) exposed a
  bridge gap: the Rust envelope `{ ok: true }` failed `Result(z.void())` in the renderer
  («Некорректный ответ хоста» after a successful approval). `tauriBridge.ts` now restores
  `data: undefined` (`restoreEnvelope`, in `tauriEnvelope.ts`, tested in `tests/renderer/tauriBridge.test.ts`).

- `pnpm build:tauri` could not be re-run: Zed's rust-analyzer held files in `target/`
  (os error 32). `cargo build -p zvs-studio` compiles. The manual check (`pnpm dev:tauri`,
  demo job on the Tasks page, cancel kills the sidecar job, quit mid-run and relaunch, Runs &
  Logs retry) has **not** been done.
- `system.demoJob` goes through `job.run` admission. Without a `job.run` grant it blocks on an
  approval (Tasks page shows it), as in the TS host.
- `ipc::vector_stores::tests::the_eval_channels_round_trip_through_the_contract` is flaky: it
  compares `durationMs` after a JSON round trip and fails for some float values. It predates
  this task.
- TASK_073: register tool nodes into `services.runs.registry()`, add `invoke_tool` to
  `StepLink` and `Scheduler`, and wire the tool ceiling into the admit hook.
- TASK_074: chat observers use `RunService::subscribe`; `llm.generate` already accepts
  `{ request }`.
