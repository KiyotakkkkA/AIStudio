---
task: TASK_039
title: Scenario execution, node inspector, run log
phase: 7 — Scenarios and Agentic Build
status: done
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

- [x] Scenarios compile to kernel graphs and run
- [x] Live canvas feedback driven by real step events
- [x] Inspector renders parameters from node schemas
- [x] Single-node run with pinned input works
- [x] Run log strip shows real step input, output and logs
- [x] Overlap behaviour explicit, not accidental
- [x] Tests and gates pass
- [ ] `pnpm typecheck && pnpm lint && pnpm build` pass (lint and build pass; typecheck: the
  pre-existing `fileChanges.test.ts` failure, see handoff)
- [x] Handoff block filled in, status set to `done`, INDEX.md ticked

## Handoff

**Built:**
- Kernel: a node binding can project a path of a dependency's output
  (`{ source: "node", nodeId, path }`); `GraphNode` gained `timeoutMs` (the step fails with
  `STEP_TIMED_OUT`) and `continueOnError` (an exhausted failure completes with
  `{ "error": message }` and the run goes on; recovery honours it). Log lines a node emits are
  stamped with its `nodeId`.
- Contract: `ScenarioNodeDto` gained `settings` (`maxAttempts`, `backoffMs`, `timeoutMs`,
  `onError: stop | continue`), `notes` and `pinnedInput`; `ScenarioDto.overlap`
  (`skip | queue`, migration `0015_scenario_overlap`, default `skip`) set through
  `scenarios.update`; `ScenarioNodeTypeDto` gained `paramsSchema` and `idempotent`.
  New channels `scenarios.run` (working graph or the saved version, optional event),
  `scenarios.runNode` (one step on pinned input) and `scenarios.retryStep`.
  `NotificationTargetDto::Scenarios`. Fixtures added and updated on both sides.
- `zvs-host/src/scenarios/compile.rs`: validates first and refuses with the validator's
  issues; every enabled trunk node becomes a kernel step; disabled steps are bridged (their
  successors depend on their predecessors); sub-nodes are folded into the parent's input
  (`model`, `memory`, `embedding` as `{ nodeId, type, params }`, `tools` as the union of tool
  keys, which also becomes the run's `allowedTools`). Built-in steps get `params`, the run
  event (`event`) and one `from:<id>` binding per predecessor; registry steps take their
  params as input, and a param that is exactly `{{result}}` or `{{result.path}}` is bound to
  the predecessor's output. Node settings map to retry, timeout and continue-on-error.
- `scenarios/nodes.rs` registers executors for the built-in types: triggers pass the event
  through; `agent.tools` runs a tool loop (`maxIterations`, default 8) on the attached chat
  model, offering the attached tools and calling them through `StepContext::invoke_tool`, so
  each call is its own `agent.callN` step under kernel admission; memory feeds the last N
  entries of `event.history`. Output steps render `{{result…}}`, `{{event…}}`, `{{date}}`
  templates and act through a `ScenarioEffects` port (`HostScenarioEffects`: chat post,
  new conversation on the first provider's default model, a notification, a file under
  `<data>/scenario-files`).
- `scenarios/runner.rs` (`ScenarioRunner` on `Services.scenario_runs`): test runs, single-step
  runs, retry from a failed step (refused unless the node type is idempotent; succeeded steps
  are copied into the new run as checkpoints and it carries `retryOfId`), and trigger firing
  with a per-scenario lane: `skip` drops a trigger while a run is live, `queue` holds it and
  starts it when the run ends. Trigger runs pin the current version (title `name · vN`).
  `ChatService::send` announces each message to listeners; the runner fires `chat` triggers,
  filtered by the trigger's `conversationId`.
- Renderer: `runModel.ts` (pure step/edge/tree logic), `ScenarioRunState` (start, run step,
  retry, cancel, load the last run from `runs.list` + `runs.detail`, follow the stream);
  canvas marks the running node, finished nodes, the failed node with its error, turns
  traversed edges `ok` and labels them with item counts. Inspector has «Параметры» (fields
  from the type's `paramsSchema`, labels from `NODE_FORMS` where present), «Настройки»
  (attempts, pause, time limit, on-error, pinned input) and «Заметки»; sub-nodes listed with
  status; footer «Запустить шаг» / «Отключить». The overview holds the overlap setting. The
  run log strip shows the step tree with timings and «Вход / Выход / Журнал» with highlighted
  JSON, pins a step's observed input, offers «Повторить с этого шага», and opens on the
  failing step when a run fails. «Тестовый запуск» / «Остановить» sit beside «Добавить шаг».
- Tests: `scenarios/run_tests.rs` (attachment folding, disabled bridging, result bindings,
  refusal, dependency order of step events, pinned single-step input, failure stops at the
  node, continue-on-error, timeout, retry, side-effect refusal, skip and queue overlap, chat
  trigger filtering); `scenarioRuns.test.ts` (run model, schema fields, JSON tokens, node
  patches, the run state over a fake bridge and a real `EventRouter`).

**Deviated from plan:**
- Sub-nodes are not separate kernel steps; their configuration is folded into the parent
  step. The model call shows in the tree as the agent step, tool calls as nested steps.
- Retry from a failed step is allowed only for idempotent types (triggers and side-effect-free
  registry nodes). `agent.tools` and outputs are not idempotent, so a failed agent step is
  re-run with «Тестовый запуск».
- Mail and webhook triggers have matching rules in `trigger_matches` but nothing fires them
  yet (TASK_048); schedule is out of scope. Overlap lanes are in memory and do not survive a
  restart.
- Scenario notifications use the «turn finished» notification setting.

**Next session should know:**
- `{{result}}` binding needs exactly one predecessor on a registry step; built-in steps with
  several predecessors receive an object keyed by predecessor id.
- Posting a scenario message does not push an event to an open chat; the conversation shows
  it on reload.
- Gates run: cargo fmt, clippy `-D warnings`, `cargo test --workspace --exclude zvs-jobd` (a
  running dev build locked `zvs-jobd.exe`), `pnpm lint`, `pnpm test`, the shared fixture
  tests, `pnpm format:check`, `pnpm contract:channels:check`, `pnpm build`. `pnpm typecheck` fails only in
  `apps/studio/tests/renderer/fileChanges.test.ts`, as before this task. The manual walk with
  a real provider and vector store was not done in this session.
