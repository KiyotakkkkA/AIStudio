---
task: TASK_040
title: Agentic session engine — planning loop and self-extending runs
phase: 7 — Scenarios and Agentic Build
status: todo
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

- [ ] Planning loop runs, revises and terminates on goal or budget
- [ ] Tool calls honour the registry and approval flow
- [ ] Plan edits applied at step boundaries
- [ ] Budgets enforced with a clear stop reason
- [ ] Workspace paths validated host-side, never trusted from the model
- [ ] Loop detection stops non-convergent sessions
- [ ] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
