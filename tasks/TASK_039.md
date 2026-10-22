---
task: TASK_039
title: Scenario execution, node inspector, run log
phase: 7 — Scenarios and Agentic Build
status: todo
depends_on: [TASK_038, TASK_076]
unblocks: [TASK_040]
estimate: 4 h
---

# TASK_039 — Scenario execution, node inspector, run log

> **Post-migration note (2026-10-10).** This task runs after Phase M (TASK_058 – TASK_076):
> the app is Tauri 2 with a Rust host. Read every `apps/studio/src/host/…` path below as its
> Rust counterpart in `crates/zvs-host/src/…` (or `apps/studio/src-tauri/src/…` for
> window, webview and OS-shell code) using the path map in PASSPORT §5. Electron APIs named
> below map as PASSPORT §2 D10 describes. Host code is Rust, so the gates also include
> `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`, and
> host tests are Rust tests, not Vitest.

## Context

The editor can draw a scenario; this task runs it. Execution is a kernel run (TASK_022) over the validated graph, which means cancellation, checkpointing, retries, permissions and the Tasks page all come for free — that is decision D7 paying off for the second time.

This task also builds the two panels around the canvas from `/design/Scenario.dc.html`: the **node inspector** on the right (parameters, settings, notes, connected sub-nodes, Run this node, Disable) and the **run log strip** along the bottom (a step tree with timings on the left, and Input/Output/Logs tabs showing the selected step's JSON on the right).

Live execution feedback on the canvas is what makes the editor feel alive: the running node highlighted, traversed edges turning green, item counts appearing on edges, and a node that failed marked in place.

## Why

A scenario you cannot run and cannot watch is a diagram. This turns the editor into an automation tool.

## Depends on

- `TASK_038` — Scenario canvas editor.

## Read first

- `/design/Scenario.dc.html` — the inspector fields, the run log strip, the per-node check marks and item counts
- `/docs/PASSPORT.md` §11

## Scope

**In scope**
- Compiling a scenario version into a kernel graph
- Test-run from the editor with live canvas feedback
- Node inspector: parameters driven by each node type's schema, settings, connected sub-nodes
- Run a single node with pinned input
- Run log strip: step tree with timings, Input/Output/Logs tabs
- Activation wiring so a bound trigger starts a run
- Tests

**Out of scope** (do not do these, even if tempting)
- The agentic front-end (TASK_040)
- Scheduling recurring runs — the trigger binding exists; a scheduler is later
- Pinned test data sets beyond a single node's input

## Steps

1. Compile a `scenario_version` graph into the kernel's graph representation, resolving node types through the registry and attachments into the node's dependencies. Fail compilation loudly on anything the validator would have caught, as a second line of defence.
2. Test-run from the editor: start a kernel run, subscribe to its stream, and drive canvas feedback from `step` events — running node highlighted, completed nodes marked, edges traversed turned green with item counts, failed node marked with its error.
3. Node inspector: render parameters from the node type's input schema, so a new node type gets an editor for free. Settings tab holds retry policy, timeout and on-error behaviour. Connected sub-nodes list with their status, as drawn.
4. Run this node: execute a single node with input either pinned by the user or taken from the last run's upstream output — the fast iteration loop that makes building a scenario bearable.
5. Run log strip: step tree with per-step timings on the left; Input, Output and Logs tabs on the right rendering the selected step's JSON with syntax highlighting.
6. Activation: an active scenario with a bound trigger starts a run when that trigger fires. Guard against overlapping runs — either queue or skip, and make which one it does a per-scenario setting rather than a hidden default.
7. Errors: a failing node marks the run failed, the canvas shows where, and the log strip opens on the failing step. Retry from a failed step where the node is idempotent.
8. Tests: compilation maps attachments correctly; a run emits step events in dependency order; single-node run uses pinned input; overlapping-trigger behaviour matches the setting; failure marks the right node.

## Verify

```bash
pnpm --filter studio test
pnpm --filter studio dev
pnpm typecheck && pnpm lint && pnpm build
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

Manual check: Build and run the mockup's scenario end to end against a real provider and a real vector store. Watch the canvas light up, inspect a step's input and output in the log strip, break a node deliberately and confirm the failure is shown in place.

## Definition of done

- [ ] Scenarios compile to kernel graphs and run
- [ ] Live canvas feedback driven by real step events
- [ ] Inspector renders parameters from node schemas
- [ ] Single-node run with pinned input works
- [ ] Run log strip shows real step input, output and logs
- [ ] Overlap behaviour explicit, not accidental
- [ ] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass
- [ ] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
**Deviated from plan:**
**Next session should know:**
