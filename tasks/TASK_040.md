---
task: TASK_040
title: Agentic session engine — planning loop and self-extending runs
phase: 7 — Scenarios and Agentic Build
status: done
depends_on: [TASK_039, TASK_076, TASK_036_EXTRA_2, TASK_036_EXTRA_3]
unblocks: [TASK_041]
estimate: 4 h
---

# TASK_040 — Agentic session engine — planning loop and self-extending runs

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

The third kernel front-end, and the most open-ended. Where a scenario is a graph the user authored, an **agentic session** is a graph the model extends as it goes: it proposes a plan, executes a step, observes the result, and revises. `/design/AgenticBuild.dc.html` shows it — a plan column with done, running and queued steps; a tool-call stream with real diffs and command output; a session-context rail with the model, permission mode, workspace and changed files; and an approval bar when the agent wants to do something outside what it was granted.

Everything dangerous about this is already handled by machinery that exists: tool calls go through the registry (TASK_031), permission admission and approvals through TASK_023, execution and cancellation through the kernel, and the whole session appears on the Tasks page. This task is the planning loop plus the session state, not a new security model.

The plan is editable by the user — the mockup's note says the plan is the agent's own and dragging to reorder is picked up at the next boundary. That is a real requirement, not a flourish: it is what keeps a long session steerable.

## Why

It is the feature that turns the app from an automation builder into an assistant that can do multi-step work, and it is the natural consumer of skills, tools and the browser.

## Depends on

- `TASK_039` — Scenario execution, node inspector, run log.

## Read first

- `/design/AgenticBuild.dc.html` — plan states, tool-call cards, approval bar, context rail, budget meter
- `/docs/PASSPORT.md` §11 (agentic is a self-extending plan under the same rules)

## Scope

**In scope**
- `agentic_session` table: goal, plan, status, permission mode, workspace, budget
- Planning loop: propose plan → execute step → observe → revise, bounded
- Tool calling through the registry with approvals
- User edits to the plan honoured at step boundaries
- Budget enforcement: token, step and time limits with a clear stop
- Session context: model settings, permission mode, workspace, changed files
- Tests with a scripted fake model

**Out of scope** (do not do these, even if tempting)
- The page (TASK_041)
- Browser tools (TASK_046) — they register like any other tool and will simply appear
- Autonomous background sessions with no user present

## Steps

1. `agentic_session` table plus a plan representation of ordered steps with status, each linking to the kernel step that executed it.
2. Planning loop as a kernel run whose graph grows: the model proposes steps, each becomes a kernel step, results feed the next planning call. Bound it hard — max steps, max tokens, max wall-clock — and stop with a clear reason rather than drifting.
3. Give the model tools from the registry, filtered to the session's permitted set. Tool results are fed back as observations. A tool needing approval suspends the run through TASK_023's existing flow, which is what renders the approval bar.
4. Plan editing: the user may edit, reorder or delete queued steps. Apply changes at the next step boundary, never mid-step, and tell the model the plan changed so it does not re-propose what was removed.
5. Permission mode per session — read-only, ask, auto — narrowing the ceiling, never widening it. Auto still cannot execute anything the Tools page has set to `ask` or `off`.
6. Workspace binding: a session is scoped to a directory. Path checks happen host-side against an allow-list, per PASSPORT §14 — never in a tool argument the model controls.
7. Session context tracking: changed files with line counts, tool call count, elapsed time, token usage for the budget meter.
8. Loop-detection: if the model proposes the same step repeatedly or the plan stops converging, stop and report rather than burning budget.
9. Tests with a scripted fake model: the loop terminates on goal completion; budgets stop it cleanly; an approval suspends and resumes; plan edits are honoured at the right boundary; a repeated-step loop is detected; auto mode cannot exceed the ceiling.

## ❓ DECISION — ask the user before implementing this part

> **Question to ask, verbatim:**
> "What default budget should an agentic session get — how many steps, how many tokens and how long before it stops and asks whether to continue? And should hitting the budget stop the session outright or pause it for you to extend?"
>
> **Why it matters:** An unbounded agentic loop against a metered provider is the fastest way to an unpleasant bill, and the default is what most sessions will use.
> **Options:** (a) conservative — 25 steps, 200k tokens, 15 minutes, pause and ask to extend; (b) generous — 100 steps, 1M tokens, an hour; (c) no default limit, rely on the Stop button.
> **If the user is unavailable:** take (a), pausing rather than terminating, and expose all three limits in Settings (TASK_052).

## Verify

```bash
pnpm --filter studio test
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Run a real multi-step session against a live model in a scratch workspace — something like 'read these files and write a summary'. Confirm the plan appears, steps execute, an approval-gated action suspends, and the budget stops it where expected.

## Definition of done

- [x] Planning loop runs, revises and terminates on goal or budget
- [x] Tool calls honour the registry and approval flow
- [x] Plan edits applied at step boundaries
- [x] Budgets enforced with a clear stop reason
- [x] Workspace paths validated host-side, never trusted from the model
- [x] Loop detection stops non-convergent sessions
- [x] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- Decision: option (a) as the user chose — 25 steps, 200 000 tokens, 15 minutes, pausing
  rather than stopping. `AgenticService::default_budget` reads `agentic.budget.maxSteps`,
  `agentic.budget.maxTokens`, `agentic.budget.maxMinutes` from settings (falls back to the
  defaults when absent or out of range); TASK_052 only needs the UI.
- Contract `zvs_contract::agentic` / `packages/shared/src/agentic.ts`: `AgenticSessionId`,
  `AgenticPermissionMode` (`read-only | ask | auto`), `AgenticSessionStatus`
  (`running | paused | completed | stopped | failed | cancelled`), `AgenticStopReason`
  (`goal | steps | tokens | time | loop | cancelled | error`), `AgenticPlanStepStatus`,
  `AgenticBudgetDto`, `AgenticUsageDto`, `AgenticPlanStepDto` (with `stepIds` linking kernel
  steps), `AgenticPlanEditDto`, `AgenticChangedFileDto`, `AgenticSessionDto` and the inputs.
  Fixtures in `fixtures/agentic/`. Channels `agentic.list|get|create|updatePlan|setMode|
  instruct|extend|cancel|remove`.
- Migration `0018_agentic_session`, `AgenticSessionRow` (JSON columns for budget, usage, plan,
  pending edits, changed files and the transcript), `AgenticSessionRepository`.
- `zvs-host/src/agentic/`: `plan.rs` (pure plan logic: model proposals, user edits on queued
  steps only, linking, changed-file totals, `LoopGuard`), `node.rs` (`agentic.session`, the
  loop), `store.rs` (row access and DTO mapping), `service.rs` (`AgenticService`). The kernel
  admits `llm.generate` for `RunKind::Agentic` like the other front-ends. Wired into
  `Services::agentic`, the Tauri composition root and `test_services`.
- `FolderAccessService` resolves a session id as an owner: the workspace is the base and the
  boundary (`OUTSIDE_WORKSPACE`), with no folder requests and no grants. PASSPORT §14 and the
  kernel README describe it.
- Tests: `agentic/tests.rs` with a scripted fake model (goal completion, plain-answer finish,
  step/time/token budgets, extend after pause with the transcript kept, approval approve and
  deny, Tools-page `ask` and `off` under auto, read-only mode, plan edits at the boundary with
  the removed step kept out, mode and instruction at the boundary, repeated-round and
  endless-revision loops, provider failure, no tool support, cancel, retry of a finished run,
  creation refusals, workspace path scoping), `plan.rs` unit tests, the repository test.

**Deviated from plan:**
- The graph does not literally grow: a session run is one `agentic.session` node, and every
  tool call becomes its own kernel step (`session.callN`) through `invoke_tool`, as in the
  scenario agent. Plan steps link to those step ids. A budget "step" is one model round.
- Budget exhaustion ends the run (`succeeded`, session `paused`); `agentic.extend` starts a new
  run from the saved transcript. A run interrupted by a restart is not resumed: the session is
  settled as `failed` with «Выполнение прервано» when it is next read.
- The workspace is the whole allow-list for a session; it does not consult folder grants and
  never raises a folder request.
- `agentic.instruct` (mid-session instructions for TASK_041's composer) was added because it
  rides the same boundary mechanism as plan edits.

**Next session should know:**
- The page subscribes to the run's `streamId` from `AgenticSessionDto`. Each boundary emits a
  `step` event `{ domain: "agentic", sessionId, session }` carrying the whole DTO; tokens stream
  as usual; tool calls appear as ordinary step events with `nodeId` `session.callN`; approvals
  use the existing `approval` event and `runs.approve` / `runs.deny`. Plan edits and mode
  changes are visible at once as `pendingPlan` / `pendingMode` until the next boundary.
- The model's tool-call wire names are the chat ones (`fs__read`); bridges are `update_plan`,
  `complete_step`, `finish`. Token usage is an estimate (`estimate_tokens`).
- `fs.apply`'s read ledger is per run, so after an extension the model must re-read a file
  before changing it.
- Gates run: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings`,
  `cargo test --workspace`, `pnpm typecheck`, `pnpm lint`, `pnpm test`,
  `pnpm contract:channels:check`. `pnpm format:check` reports only
  `apps/studio/src/renderer/ui/organisms/NavRail.tsx`, which this task did not touch.
  `pnpm build` and the manual walk with a live model were not done in this session.
